use crate::context::AppContext;
use crate::google_calendar;
use crate::session::require_auth;
use anyhow::Result;

pub async fn run(ctx: AppContext, week: bool, date: Option<String>) -> Result<()> {
    // Kioku auth is required first — Calendar access is layered on top of it.
    require_auth()?;

    let (time_min, time_max) = if let Some(date_str) = date {
        let parsed = google_calendar::parse_date_ddmmyyyy(&date_str)?;
        google_calendar::range_for_date(parsed)
    } else if week {
        google_calendar::range_week()
    } else {
        google_calendar::range_today()
    };

    let access_token = google_calendar::ensure_valid_token_or_connect().await?;
    let events = google_calendar::list_events(&access_token, time_min, time_max).await?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&events)?);
        return Ok(());
    }

    if events.is_empty() {
        println!("No meetings.");
    }
    for e in &events {
        let link = e.link.as_deref().unwrap_or("(no link)");
        println!("{} — {}", e.start, e.summary);
        println!("  {link}");
    }
    Ok(())
}
