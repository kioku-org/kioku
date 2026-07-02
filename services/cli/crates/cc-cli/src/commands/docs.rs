use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;
use std::path::Path;

pub async fn run(ctx: AppContext, path: Option<String>, delete: Option<String>) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);

    if let Some(document_id) = delete {
        client.delete_document(&document_id).await?;
        println!("Deleted document {document_id}");
        return Ok(());
    }

    if let Some(file) = path {
        let file_path = Path::new(&file);
        client.upload_document(file_path).await?;
        println!("Uploaded {file}");
        return Ok(());
    }

    let docs = client.list_documents().await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&docs)?);
        return Ok(());
    }

    if docs.is_empty() {
        println!("No documents.");
    }
    for d in &docs {
        let id = d.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let name = d
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| d.get("filename").and_then(|v| v.as_str()))
            .unwrap_or("untitled");
        println!("{} — {}", id, name);
    }
    Ok(())
}
