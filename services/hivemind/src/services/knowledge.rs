use uuid::Uuid;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use langchain_rust::schemas::Document;

use crate::errors::AppError;
use crate::types::{KnowledgeSearchResult, TranscriptSegment};
use crate::services::vector::HivemindVectorStore;

const CHUNK_SIZE: usize = 400;
const CHUNK_OVERLAP: usize = 80;

pub struct KnowledgeService;

impl KnowledgeService {
    /// Chunk a transcript into langchain-rust Documents with metadata.
    pub fn chunk_transcript(
        segments: &[TranscriptSegment],
        meeting_id: Uuid,
        meeting_date: i64,
    ) -> Vec<Document> {
        let mut docs = Vec::new();

        for seg in segments {
            let sub_chunks = split_text(&seg.text, CHUNK_SIZE, CHUNK_OVERLAP);
            for text in sub_chunks {
                let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
                metadata.insert("meeting_id".to_string(), serde_json::json!(meeting_id.to_string()));
                metadata.insert("start".to_string(), serde_json::json!(seg.start_time));
                metadata.insert("end".to_string(), serde_json::json!(seg.end_time));
                metadata.insert("chunk_type".to_string(), serde_json::json!("transcript"));

                if !seg.speaker.is_empty() {
                    metadata.insert("speaker".to_string(), serde_json::json!(seg.speaker));
                }
                metadata.insert(
                    "timestamp".to_string(),
                    serde_json::json!(meeting_date + (seg.start_time * 1000.0) as i64),
                );

                docs.push(Document {
                    page_content: text,
                    metadata,
                    score: 0.0,
                });
            }
        }
        docs
    }

    /// Ingest chunked documents into both Postgres (for persistence) and Qdrant (for vector search).
    pub async fn ingest_documents(
        db: &PgPool,
        vector_store: &HivemindVectorStore,
        company_id: Uuid,
        meeting_id: Uuid,
        docs: &[Document],
    ) -> Result<(), AppError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_millis() as i64;

        for doc in docs {
            let speaker = doc.metadata.get("speaker").and_then(|v| v.as_str()).map(String::from);
            let timestamp = doc.metadata.get("timestamp").and_then(|v| v.as_i64());
            let metadata_json = serde_json::to_value(&doc.metadata).unwrap_or(serde_json::json!({}));

            sqlx::query(
                "INSERT INTO knowledge_chunks (id, meeting_id, text, speaker, timestamp, chunk_type, metadata, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(Uuid::new_v4())
            .bind(meeting_id)
            .bind(&doc.page_content)
            .bind(&speaker)
            .bind(timestamp)
            .bind("transcript")
            .bind(&metadata_json)
            .bind(now)
            .execute(db)
            .await
            .map_err(AppError::from)?;
        }

        vector_store
            .add_documents_for_company(company_id, docs)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to add documents to vector store: {}", e)))?;

        Ok(())
    }

    /// Ingest PDF document chunks into Postgres and Qdrant.
    pub async fn ingest_pdf_documents(
        db: &PgPool,
        vector_store: &HivemindVectorStore,
        company_id: Uuid,
        document_id: Uuid,
        docs: &[Document],
    ) -> Result<(), AppError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_millis() as i64;

        for doc in docs {
            let _chunk_index = doc.metadata.get("chunk_index").and_then(|v| v.as_i64());
            let metadata_json = serde_json::to_value(&doc.metadata).unwrap_or(serde_json::json!({}));

            sqlx::query(
                "INSERT INTO knowledge_chunks (id, document_id, text, chunk_type, metadata, created_at) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::new_v4())
            .bind(document_id)
            .bind(&doc.page_content)
            .bind("pdf_document")
            .bind(&metadata_json)
            .bind(now)
            .execute(db)
            .await
            .map_err(AppError::from)?;
        }

        vector_store
            .add_documents_for_company(company_id, docs)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to add PDF documents to vector store: {}", e)))?;

        Ok(())
    }

    /// Search knowledge using vector similarity via Qdrant.
    /// Returns results from both meeting transcripts and uploaded documents.
    pub async fn search(
        db: &PgPool,
        vector_store: &HivemindVectorStore,
        company_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, AppError> {
        let docs = vector_store
            .search_for_company(company_id, query, limit)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Vector search failed: {}", e)))?;

        let mut results = Vec::new();

        for doc in docs {
            let meeting_id = doc
                .metadata
                .get("meeting_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let document_id = doc
                .metadata
                .get("document_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let chunk_type = doc
                .metadata
                .get("chunk_type")
                .and_then(|v| v.as_str())
                .unwrap_or("transcript");

            let meeting_json = if let Some(mid) = meeting_id {
                let row = sqlx::query(
                    "SELECT id, title, date FROM meetings WHERE id = $1 AND company_id = $2",
                )
                .bind(mid)
                .bind(company_id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten();

                if let Some(row) = row {
                    use sqlx::Row;
                    serde_json::json!({
                        "id": row.get::<Uuid, _>("id"),
                        "title": row.get::<String, _>("title"),
                        "date": row.get::<i64, _>("date"),
                    })
                } else {
                    serde_json::json!({
                        "id": mid.to_string(),
                        "title": "Unknown",
                        "date": 0,
                    })
                }
            } else if let Some(did) = document_id {
                let row = sqlx::query(
                    "SELECT id, filename, status FROM knowledge_documents WHERE id = $1 AND company_id = $2",
                )
                .bind(did)
                .bind(company_id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten();

                if let Some(row) = row {
                    use sqlx::Row;
                    serde_json::json!({
                        "id": row.get::<Uuid, _>("id"),
                        "title": row.get::<String, _>("filename"),
                        "status": row.get::<String, _>("status"),
                        "date": 0,
                    })
                } else {
                    serde_json::json!({
                        "id": did.to_string(),
                        "title": "Unknown document",
                        "date": 0,
                    })
                }
            } else {
                serde_json::json!({ "id": "", "title": "Unknown", "date": 0 })
            };

            let mut chunk_json = serde_json::json!({
                "text": doc.page_content,
                "chunk_type": chunk_type,
                "score": doc.score,
            });

            if let Some(speaker) = doc.metadata.get("speaker").and_then(|v| v.as_str()) {
                chunk_json["speaker"] = serde_json::json!(speaker);
            }
            if let Some(ts) = doc.metadata.get("timestamp").and_then(|v| v.as_i64()) {
                chunk_json["timestamp"] = serde_json::json!(ts);
            }
            if let Some(mid) = meeting_id {
                chunk_json["meeting_id"] = serde_json::json!(mid.to_string());
            }
            if let Some(did) = document_id {
                chunk_json["document_id"] = serde_json::json!(did.to_string());
            }
            if let Some(name) = doc.metadata.get("document_name").and_then(|v| v.as_str()) {
                chunk_json["document_name"] = serde_json::json!(name);
            }

            chunk_json["metadata"] = serde_json::json!(doc.metadata);

            results.push(KnowledgeSearchResult {
                chunk: chunk_json,
                meeting: meeting_json,
                score: doc.score,
            });
        }

        Ok(results)
    }
}

pub fn split_text(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let end = (i + size).min(words.len());
        let chunk = words[i..end].join(" ");
        if !chunk.trim().is_empty() {
            chunks.push(chunk.trim().to_string());
        }
        i += size - overlap;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_text_basic() {
        let text = "The quick brown fox jumps over the lazy dog";
        let chunks = split_text(text, 4, 1);
        assert!(!chunks.is_empty(), "Should produce at least one chunk");
        for chunk in &chunks {
            let word_count = chunk.split_whitespace().count();
            assert!(word_count <= 4, "Chunk should not exceed size limit");
        }
    }

    #[test]
    fn test_split_text_overlap() {
        let text = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10";
        let chunks = split_text(text, 4, 2);
        // Verify overlap: chunk[i] should share last 2 words with chunk[i+1]'s first 2 words
        for i in 0..chunks.len().saturating_sub(1) {
            let end_words: Vec<&str> = chunks[i].split_whitespace().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect();
            let start_words: Vec<&str> = chunks[i + 1].split_whitespace().take(2).collect();
            assert_eq!(end_words, start_words, "Adjacent chunks should overlap by 2 words");
        }
    }

    #[test]
    fn test_split_text_short_text() {
        let text = "short text";
        let chunks = split_text(text, 400, 80);
        assert_eq!(chunks.len(), 1, "Short text should produce exactly one chunk");
        assert_eq!(chunks[0], "short text");
    }

    #[test]
    fn test_split_text_empty() {
        let chunks = split_text("", 400, 80);
        assert!(chunks.is_empty(), "Empty text should produce no chunks");
    }

    #[test]
    fn test_split_text_single_word() {
        let chunks = split_text("hello", 400, 80);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello");
    }

    #[test]
    fn test_split_text_preserves_content() {
        let words: Vec<String> = (0..500).map(|i| format!("word{}", i)).collect();
        let text = words.join(" ");
        let chunks = split_text(&text, 400, 80);
        let reconstructed = chunks.join(" ");
        let original_words: Vec<&str> = text.split_whitespace().collect();
        let reconstructed_words: Vec<&str> = reconstructed.split_whitespace().collect();
        // Every original word should appear in reconstructed text
        for word in &original_words {
            assert!(reconstructed_words.contains(word), "Word '{}' should be in reconstructed text", word);
        }
    }

    #[test]
    fn test_split_text_large_overlap() {
        let text = "a b c d e f g h i j";
        let chunks = split_text(text, 5, 4);
        // With size=5 and overlap=4, step is 1, so many chunks
        assert!(chunks.len() >= 5, "Large overlap should produce many chunks");
    }
}
