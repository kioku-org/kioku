use crate::context::AppContext;
use crate::session::{make_client, require_auth};
use anyhow::Result;
use std::path::Path;

/// Resolve a document identifier (exact id, or an unambiguous prefix of one)
/// against the document list.
pub fn resolve_document_delete_target(
    docs: &[serde_json::Value],
    id_or_prefix: &str,
) -> Result<String> {
    let ids: Vec<&str> = docs
        .iter()
        .filter_map(|d| d.get("id").and_then(|v| v.as_str()))
        .collect();

    if let Some(id) = ids.iter().find(|id| **id == id_or_prefix) {
        return Ok(id.to_string());
    }

    let matches: Vec<&&str> = ids
        .iter()
        .filter(|id| id.starts_with(id_or_prefix))
        .collect();
    match matches.as_slice() {
        [only] => Ok(only.to_string()),
        [] => Err(anyhow::anyhow!(
            "document `{}` not found — run `kioku docs` to inspect valid ids",
            id_or_prefix
        )),
        _ => Err(anyhow::anyhow!(
            "document id `{}` is ambiguous — matches multiple documents, use a longer prefix",
            id_or_prefix
        )),
    }
}

pub async fn run(ctx: AppContext, path: Option<String>, delete: Option<String>) -> Result<()> {
    let auth = require_auth()?;
    let client = make_client(&auth);

    if let Some(id_or_prefix) = delete {
        let docs = client.list_documents().await?;
        let document_id = resolve_document_delete_target(&docs, &id_or_prefix)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_fixture(id: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "filename": "doc.pdf" })
    }

    #[test]
    fn resolve_document_delete_target_accepts_exact_id() {
        let docs = vec![doc_fixture("abc-defg-hij")];
        let id = resolve_document_delete_target(&docs, "abc-defg-hij").unwrap();
        assert_eq!(id, "abc-defg-hij");
    }

    #[test]
    fn resolve_document_delete_target_accepts_unambiguous_prefix() {
        let docs = vec![doc_fixture("abc-defg-hij")];
        let id = resolve_document_delete_target(&docs, "abc-de").unwrap();
        assert_eq!(id, "abc-defg-hij");
    }

    #[test]
    fn resolve_document_delete_target_errors_for_unknown_value() {
        let docs = vec![doc_fixture("abc-defg-hij")];
        let err = resolve_document_delete_target(&docs, "zzz")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "document `zzz` not found — run `kioku docs` to inspect valid ids"
        );
    }

    #[test]
    fn resolve_document_delete_target_errors_for_ambiguous_prefix() {
        let docs = vec![doc_fixture("abc-defg-hij"), doc_fixture("abc-1234567")];
        let err = resolve_document_delete_target(&docs, "abc")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "document id `abc` is ambiguous — matches multiple documents, use a longer prefix"
        );
    }
}
