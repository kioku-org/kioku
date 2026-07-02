use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;

pub async fn run(ctx: AppContext, query: String, limit: u32) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);
    let results = client.knowledge_search(&query, limit).await?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    if results.is_empty() {
        println!("No results.");
    }
    for (i, r) in results.iter().enumerate() {
        let text = r.chunk.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let preview: String = text.chars().take(300).collect();
        let ellipsis = if text.len() > 300 { "…" } else { "" };
        println!("{}. [score {:.2}]  {}{}", i + 1, r.score, preview, ellipsis);
        println!();
    }
    Ok(())
}
