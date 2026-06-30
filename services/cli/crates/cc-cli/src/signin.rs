use std::io::{self, Write};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

pub enum Provider {
    Google,
    Github,
}

impl Provider {
    pub fn id(&self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::Github => "github",
        }
    }
}

// ── Provider selector UI ──────────────────────────────────────────────────────

pub fn select_provider() -> Result<Provider> {
    // Fallback for non-interactive environments
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Ok(Provider::Google);
    }

    let result = select_provider_inner();

    // Always restore terminal — ignore secondary errors
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), Show, Clear(ClearType::All), MoveTo(0, 0));

    result
}

fn select_provider_inner() -> Result<Provider> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, Hide, Clear(ClearType::All), MoveTo(0, 0))?;

    let mut selected = 0usize; // 0 = Google, 1 = GitHub

    loop {
        draw(&mut out, selected)?;

        match event::read()? {
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                match (code, modifiers) {
                    (KeyCode::Left, _) | (KeyCode::Char('h'), KeyModifiers::NONE) => {
                        selected = 0;
                    }
                    (KeyCode::Right, _) | (KeyCode::Char('l'), KeyModifiers::NONE) => {
                        selected = 1;
                    }
                    (KeyCode::Enter, _) => {
                        return Ok(if selected == 0 {
                            Provider::Google
                        } else {
                            Provider::Github
                        });
                    }
                    (KeyCode::Esc, _)
                    | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                        anyhow::bail!("cancelled");
                    }
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        anyhow::bail!("cancelled");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn draw(out: &mut impl Write, selected: usize) -> Result<()> {
    execute!(
        out,
        MoveTo(0, 0),
        Clear(ClearType::All),
        // Row 1 – header
        MoveTo(2, 1),
        SetAttribute(Attribute::Bold),
        Print("𓄿  kioku"),
        SetAttribute(Attribute::Reset),
        SetForegroundColor(Color::DarkGrey),
        Print("  ·  sign in"),
        ResetColor,
        // Row 3 – prompt
        MoveTo(2, 3),
        SetForegroundColor(Color::DarkGrey),
        Print("Sign in with:"),
        ResetColor,
        // Row 5 – options
        MoveTo(2, 5),
    )?;

    let labels = ["  Google  ", "  GitHub  "];
    for (i, label) in labels.iter().enumerate() {
        if i == selected {
            execute!(
                out,
                SetAttribute(Attribute::Reverse),
                SetAttribute(Attribute::Bold),
                Print(label),
                SetAttribute(Attribute::Reset),
            )?;
        } else {
            execute!(
                out,
                SetForegroundColor(Color::DarkGrey),
                Print(label),
                ResetColor,
            )?;
        }
        if i == 0 {
            execute!(out, Print("   "))?;
        }
    }

    execute!(
        out,
        // Row 7 – hint
        MoveTo(2, 7),
        SetForegroundColor(Color::DarkGrey),
        Print("← → · h l  to switch    ↵  open browser    q  cancel"),
        ResetColor,
    )?;

    out.flush()?;
    Ok(())
}

// ── OAuth callback server ─────────────────────────────────────────────────────

/// Waits for the browser to redirect to `http://localhost:<port>/callback?...`
/// Returns (token, user_id, email, name, company_id, role).
pub async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<OAuthResult> {
    use tokio::time::{timeout, Duration};

    let (mut stream, _) = timeout(Duration::from_secs(120), listener.accept())
        .await
        .map_err(|_| anyhow::anyhow!("Timed out after 2 minutes — browser sign-in not completed"))?
        .map_err(|e| anyhow::anyhow!("Socket error: {}", e))?;

    let mut buf = vec![0u8; 16_384];
    let n = timeout(Duration::from_secs(5), stream.read(&mut buf))
        .await
        .map_err(|_| anyhow::anyhow!("Timed out reading callback request"))?
        .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;

    let raw = std::str::from_utf8(&buf[..n]).unwrap_or("");

    // Parse "GET /callback?key=val&... HTTP/1.1"
    let path = raw
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP request in callback"))?;

    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let params: std::collections::HashMap<&str, &str> = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .collect();

    // Validate CSRF state
    let state = params.get("state").copied().unwrap_or("");
    if state != expected_state {
        let _ = respond(&mut stream, 400, "Bad Request", CSRF_HTML).await;
        anyhow::bail!("State mismatch — possible CSRF. Please try again.");
    }

    let decode = |key: &str| -> String {
        params
            .get(key)
            .map(|v| urlencoding::decode(v).unwrap_or_default().into_owned())
            .unwrap_or_default()
    };

    let token = decode("token");
    if token.is_empty() {
        let _ = respond(&mut stream, 400, "Bad Request", "Missing token").await;
        anyhow::bail!("No token in callback — dashboard may not be configured correctly");
    }

    let email = decode("email");
    let name = decode("name");

    // Respond with a success page before closing
    respond(&mut stream, 200, "OK", &success_html(&name, &email)).await?;

    Ok(OAuthResult {
        token,
        user_id: decode("user_id"),
        email,
        name,
        company_id: decode("company_id"),
        role: decode("role"),
    })
}

pub struct OAuthResult {
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub company_id: String,
    pub role: String,
}

async fn respond(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    reason: &str,
    body: &str,
) -> Result<()> {
    let content_type = if body.trim_start().starts_with('<') {
        "text/html"
    } else {
        "text/plain"
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

fn success_html(name: &str, email: &str) -> String {
    let display = if name.is_empty() { email } else { name };
    format!(
        r#"<!DOCTYPE html><html><head><title>kioku</title><style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:system-ui,sans-serif;background:#0c0c0c;color:#fafaf9;display:flex;align-items:center;justify-content:center;min-height:100vh}}
.card{{text-align:center;padding:2rem}}
.crow{{font-size:3rem;margin-bottom:1rem}}
h1{{font-size:1.5rem;font-weight:700;margin-bottom:.5rem}}
p{{color:#737373;font-size:.9rem}}
</style></head><body>
<div class="card">
  <div class="crow">𓄿</div>
  <h1>signed in as {display}</h1>
  <p>You can close this tab and return to your terminal.</p>
</div></body></html>"#
    )
}

const CSRF_HTML: &str = r#"<!DOCTYPE html><html><head><title>kioku — error</title></head>
<body><h2>Sign-in error</h2><p>State mismatch detected. Please retry <code>kioku signin</code>.</p></body></html>"#;
