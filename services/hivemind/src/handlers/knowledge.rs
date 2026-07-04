use axum::extract::{Multipart, State};
use axum::response::Json;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::repos::knowledge::KnowledgeRepo;
use crate::services::knowledge::KnowledgeService;
use crate::services::{docx, ocr, pdf, pptx};
use crate::types::KnowledgeDocumentOut;
use crate::AppState;

const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024; // 50MB

/// Supported upload formats: extension -> chunk_type label.
const SUPPORTED_EXTENSIONS: &[(&str, &str)] = &[
    (".pdf", "pdf_document"),
    (".docx", "docx_document"),
    (".pptx", "pptx_document"),
    (".txt", "text_document"),
    (".md", "text_document"),
];

fn chunk_type_for_filename(filename: &str) -> Option<&'static str> {
    let lower = filename.to_lowercase();
    SUPPORTED_EXTENSIONS
        .iter()
        .find(|(ext, _)| lower.ends_with(ext))
        .map(|(_, chunk_type)| *chunk_type)
}

/// DOCX/PPTX are zip containers; neither docx-rs nor python-pptx cap uncompressed size, so an
/// untrusted upload could be a zip bomb (small compressed file, huge decompressed footprint).
/// Reject before decompression if the archive *claims* more than this — read from the zip
/// central directory, doesn't actually decompress anything.
const MAX_UNCOMPRESSED_SIZE: u64 = 300 * 1024 * 1024; // 300MB

/// Sums the *uncompressed* size of every entry from the zip central directory, without
/// decompressing any of them.
fn total_uncompressed_size(file_data: &[u8]) -> Result<u64, AppError> {
    let cursor = std::io::Cursor::new(file_data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| AppError::BadRequest(format!("Not a valid Office document: {}", e)))?;
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index_raw(i) {
            total += f.size();
        }
    }
    Ok(total)
}

fn check_zip_bomb(file_data: &[u8]) -> Result<(), AppError> {
    let total = total_uncompressed_size(file_data)?;
    if total > MAX_UNCOMPRESSED_SIZE {
        return Err(AppError::BadRequest(format!(
            "Document expands to {} bytes uncompressed (max {} bytes)",
            total, MAX_UNCOMPRESSED_SIZE
        )));
    }
    Ok(())
}

/// Extract plain text from uploaded document bytes based on its detected format. PDFs whose
/// text layer looks too thin (scanned/image-only) fall back to OCR.
fn extract_text(chunk_type: &str, file_data: &[u8]) -> Result<String, AppError> {
    match chunk_type {
        "pdf_document" => {
            let text = pdf::extract_text_from_bytes(file_data)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("PDF extraction failed: {}", e)))?;
            if ocr::should_ocr(&text) {
                match ocr::ocr_pdf_bytes(file_data) {
                    Ok(ocr_text) if !ocr_text.trim().is_empty() => Ok(format!("{text}\n{ocr_text}")),
                    Ok(_) => Ok(text),
                    Err(e) => {
                        tracing::warn!("PDF OCR fallback failed, keeping text-layer only: {}", e);
                        Ok(text)
                    }
                }
            } else {
                Ok(text)
            }
        }
        "docx_document" => {
            check_zip_bomb(file_data)?;
            docx::extract_text_from_bytes(file_data)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("DOCX extraction failed: {}", e)))
        }
        "pptx_document" => {
            check_zip_bomb(file_data)?;
            pptx::extract_text_from_bytes(file_data)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("PPTX extraction failed: {}", e)))
        }
        _ => String::from_utf8(file_data.to_vec())
            .map_err(|e| AppError::BadRequest(format!("File is not valid UTF-8 text: {}", e))),
    }
}

/// Upload a document (PDF, DOCX, PPTX, plain text, or markdown) to the knowledge base.
/// Extracts text, chunks it, embeds vectors into Qdrant, and persists metadata.
pub async fn upload_document(
    State(state): State<AppState>,
    auth: AuthContext,
    mut multipart: Multipart,
) -> Result<Json<KnowledgeDocumentOut>, AppError> {
    let mut filename = String::from("unknown");
    let mut file_data = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            if let Some(fname) = field.file_name() {
                filename = fname.to_string();
            }
            file_data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file bytes: {}", e)))?
                .to_vec();

            if file_data.len() > MAX_UPLOAD_SIZE {
                return Err(AppError::BadRequest(format!(
                    "File too large: {} bytes (max {} bytes)",
                    file_data.len(),
                    MAX_UPLOAD_SIZE
                )));
            }
        }
    }

    if file_data.is_empty() {
        return Err(AppError::BadRequest("No file uploaded".into()));
    }

    let Some(chunk_type) = chunk_type_for_filename(&filename) else {
        return Err(AppError::BadRequest(
            "Unsupported file type. Supported: .pdf, .txt, .md".into(),
        ));
    };

    let now = crate::util::now_ms();
    let doc_id = uuid::Uuid::new_v4();

    // Create document record as "processing"
    let repo = KnowledgeRepo::new(state.db.clone());
    repo.create_document(
        doc_id,
        auth.workspace_id,
        auth.user_id,
        &filename,
        file_data.len() as i64,
        "processing",
        now,
    )
    .await?;

    let text = extract_text(chunk_type, &file_data)?;

    if text.trim().is_empty() {
        repo.update_document_status(doc_id, "empty", 0, now).await?;
        return Err(AppError::BadRequest("No text content found in file".into()));
    }

    // Chunk the text
    let docs = pdf::chunk_document_text(&text, auth.workspace_id, doc_id, &filename, chunk_type);
    let chunk_count = docs.len() as i32;

    // Ingest into Postgres + Qdrant
    KnowledgeService::ingest_document_chunks(
        &state.db,
        &state.vector_store,
        auth.workspace_id,
        doc_id,
        &docs,
        chunk_type,
    )
    .await?;

    // Update document status to "completed"
    repo.update_document_status(doc_id, "completed", chunk_count, now)
        .await?;

    // Return the document
    let doc = repo
        .get_document(doc_id, auth.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Document not found".into()))?;

    Ok(Json(doc))
}

/// Ingest a raw content dump (conversation logs, diffs, notes — anything) into the knowledge
/// base for semantic search. Was previously reachable only via the (now-consolidated) MCP
/// server's `session` tool; exposed as a real REST endpoint so that tool is a thin proxy like
/// every other one, instead of the one place that needed direct DB/vector-store access.
pub async fn ingest_session(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<crate::types::KnowledgeSessionIngestRequest>,
) -> Result<Json<crate::types::KnowledgeSessionIngestResponse>, AppError> {
    if req.content.trim().is_empty() {
        return Err(AppError::BadRequest("content is empty".into()));
    }

    let session_id = uuid::Uuid::new_v4();
    let now = crate::util::now_ms();
    let date = req.date.unwrap_or(now);
    let tags = serde_json::to_value(&req.tags).unwrap_or(serde_json::json!([]));
    let preview: String = req.content.chars().take(500).collect();

    sqlx::query(
        "INSERT INTO coding_sessions (id, workspace_id, user_id, title, summary, decisions, tags, date, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(session_id)
    .bind(auth.workspace_id)
    .bind(auth.user_id)
    .bind(&req.title)
    .bind(&preview)
    .bind(serde_json::json!([]))
    .bind(&tags)
    .bind(date)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::from)?;

    let raw_chunks = crate::services::knowledge::split_text_paragraphs(&req.content, 400);
    let metadata = serde_json::json!({"chunk_type": "session", "session_id": session_id.to_string(), "timestamp": date});

    for chunk_text in &raw_chunks {
        sqlx::query(
            "INSERT INTO knowledge_chunks (id, session_id, text, chunk_type, metadata, created_at) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(session_id)
        .bind(chunk_text)
        .bind("session")
        .bind(&metadata)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(AppError::from)?;
    }

    let docs: Vec<langchain_rust::schemas::Document> = raw_chunks
        .iter()
        .map(|chunk| langchain_rust::schemas::Document {
            page_content: chunk.clone(),
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("chunk_type".to_string(), serde_json::json!("session"));
                m.insert("session_id".to_string(), serde_json::json!(session_id.to_string()));
                m.insert("timestamp".to_string(), serde_json::json!(date));
                m
            },
            score: 0.0,
        })
        .collect();

    state
        .vector_store
        .add_documents_for_workspace(auth.workspace_id, &docs)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

    Ok(Json(crate::types::KnowledgeSessionIngestResponse { session_id, chunk_count: raw_chunks.len() }))
}

/// List all knowledge documents for the workspace.
pub async fn list_documents(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Vec<KnowledgeDocumentOut>>, AppError> {
    let repo = KnowledgeRepo::new(state.db.clone());
    let documents = repo.list_documents(auth.workspace_id).await?;
    Ok(Json(documents))
}

/// Delete a knowledge document and its vector embeddings.
pub async fn delete_document(
    State(state): State<AppState>,
    auth: AuthContext,
    axum::extract::Path(document_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<()>, AppError> {
    let repo = KnowledgeRepo::new(state.db.clone());

    // Verify document belongs to workspace
    let doc = repo
        .get_document(document_id, auth.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Document not found".into()))?;

    // Delete vector embeddings from Qdrant
    state
        .vector_store
        .delete_for_document(document_id)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to delete vectors: {}", e)))?;

    // Delete from Postgres
    repo.delete_document(document_id, auth.workspace_id).await?;

    tracing::info!(
        document_id = %document_id,
        workspace_id = %auth.workspace_id,
        filename = %doc.filename,
        "Knowledge document deleted"
    );

    Ok(Json(()))
}

#[cfg(test)]
mod zip_guard_tests {
    use super::*;

    fn build_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, content) in entries {
                writer.start_file(*name, zip::write::FileOptions::default()).unwrap();
                std::io::Write::write_all(&mut writer, content).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn sums_uncompressed_size_across_entries() {
        let zip = build_test_zip(&[("a.xml", &[0u8; 100]), ("b.xml", &[0u8; 250])]);
        assert_eq!(total_uncompressed_size(&zip).unwrap(), 350);
    }

    #[test]
    fn small_zip_passes_the_bomb_check() {
        let zip = build_test_zip(&[("word/document.xml", b"<xml>hello</xml>")]);
        assert!(check_zip_bomb(&zip).is_ok());
    }

    #[test]
    fn garbage_bytes_are_rejected_as_invalid_archive() {
        let err = total_uncompressed_size(b"not a zip file at all");
        assert!(err.is_err());
    }
}
