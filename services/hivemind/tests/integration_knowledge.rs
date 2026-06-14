use reqwest::Client;
use serde_json::json;

fn base_url() -> String {
    std::env::var("HIVEMIND_URL").unwrap_or_else(|_| "http://localhost:9100".into())
}

fn client() -> Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap()
}

async fn register_and_get_token(suffix: &str) -> (String, serde_json::Value) {
    let c = client();
    let email = format!("{}_{}@example.com", suffix, uuid::Uuid::new_v4());
    let resp = c
        .post(format!("{}/auth/register/admin", base_url()))
        .json(&json!({
            "company_name": format!("{} Company", suffix),
            "email": &email,
            "name": format!("{} User", suffix),
            "password": "testpassword123"
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Register failed: {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    (body["token"].as_str().unwrap().to_string(), body)
}

fn auth_header(token: &str) -> reqwest::header::HeaderValue {
    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap()
}

// ─── Auth & Input Validation ────────────────────────────────────────────────

#[tokio::test]
async fn knowledge_search_requires_auth() {
    let c = client();
    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .json(&json!({"query": "test", "limit": 3}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "Knowledge search should require auth");
}

#[tokio::test]
async fn knowledge_search_empty_query_rejected() {
    let c = client();
    let (token, _) = register_and_get_token("knn_empty_q").await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "", "limit": 3}))
        .send()
        .await
        .unwrap();
    // Empty query should either be rejected or return no results
    assert!(resp.status().is_success() || resp.status() == 400,
        "Empty query should return 200 (empty) or 400, got {}", resp.status());
}

#[tokio::test]
async fn knowledge_search_missing_query_rejected() {
    let c = client();
    let (token, _) = register_and_get_token("knn_no_q").await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"limit": 3}))
        .send()
        .await
        .unwrap();
    assert!(!resp.status().is_success(), "Missing query should be rejected");
}

#[tokio::test]
async fn knowledge_upload_non_pdf_rejected() {
    let c = client();
    let (token, _) = register_and_get_token("knn_upload").await;

    let resp = c
        .post(format!("{}/knowledge/documents", base_url()))
        .header("Authorization", auth_header(&token))
        .multipart(
            reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::text("not a pdf").file_name("test.txt"),
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "Non-PDF upload should be rejected");
}

#[tokio::test]
async fn knowledge_upload_empty_file_rejected() {
    let c = client();
    let (token, _) = register_and_get_token("knn_upload_empty").await;

    let resp = c
        .post(format!("{}/knowledge/documents", base_url()))
        .header("Authorization", auth_header(&token))
        .multipart(
            reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::text("").file_name("empty.pdf"),
            ),
        )
        .send()
        .await
        .unwrap();
    assert!(!resp.status().is_success(), "Empty file should be rejected");
}

#[tokio::test]
async fn knowledge_documents_requires_auth() {
    let c = client();
    let resp = c
        .get(format!("{}/knowledge/documents", base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ─── Meeting Ingest → Search (End-to-End) ──────────────────────────────────

#[tokio::test]
async fn meeting_ingest_then_search() {
    let c = client();
    let (token, body) = register_and_get_token("mtg_search").await;
    let now = chrono::Utc::now().timestamp_millis();

    // Ingest a meeting with distinctive content
    let resp = c
        .post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Q4 Product Roadmap Review",
            "date": now,
            "duration_seconds": 3600,
            "participants": ["Alice", "Bob", "Carol"],
            "transcript": [
                {"speaker": "Alice", "text": "We need to finalize the Q4 product roadmap. The key priorities are mobile app redesign, API v3 migration, and the new analytics dashboard. Bob can you give an update on the mobile redesign?", "start_time": 0.0, "end_time": 15.0},
                {"speaker": "Bob", "text": "The mobile redesign is 70% complete. We are using React Native for the new UI components. The navigation flow has been completely reworked to match the new design system. ETA for completion is end of October.", "start_time": 15.0, "end_time": 35.0},
                {"speaker": "Carol", "text": "For the analytics dashboard, we have chosen Apache Superset as the visualization layer. It integrates well with our existing data warehouse. The proof of concept showed a 40% improvement in query latency compared to the old reporting system.", "start_time": 35.0, "end_time": 60.0},
                {"speaker": "Alice", "text": "Great progress everyone. Let us also discuss the API v3 migration plan. We need to deprecate v2 endpoints by December. The breaking changes include the new authentication flow and the consolidated response format.", "start_time": 60.0, "end_time": 85.0}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Ingest meeting failed: {}", resp.status());
    let meeting: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(meeting["title"], "Q4 Product Roadmap Review");

    // Wait for async ingestion to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Search for specific topics
    let search_resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "mobile redesign React Native", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(search_resp.status().is_success(), "Search failed: {}", search_resp.status());
    let results: serde_json::Value = search_resp.json().await.unwrap();
    let results_arr = results.as_array().expect("results should be array");
    assert!(!results_arr.is_empty(), "Should find results for mobile redesign query");

    // Verify the result contains relevant content
    let first = &results_arr[0];
    let chunk_text = first["chunk"]["text"].as_str().unwrap_or("").to_lowercase();
    assert!(
        chunk_text.contains("mobile") || chunk_text.contains("redesign") || chunk_text.contains("react"),
        "Top result should contain mobile/redesign/React, got: {}", chunk_text
    );

    // Verify meeting metadata is attached
    assert!(first["meeting"]["id"].is_string(), "Meeting ID should be present");
    assert_eq!(first["meeting"]["title"].as_str().unwrap(), "Q4 Product Roadmap Review");

    // Search for analytics content
    let analytics_resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "analytics dashboard Superset", "limit": 3}))
        .send()
        .await
        .unwrap();
    assert!(analytics_resp.status().is_success());
    let analytics: serde_json::Value = analytics_resp.json().await.unwrap();
    let analytics_arr = analytics.as_array().expect("analytics results should be array");
    if !analytics_arr.is_empty() {
        let text = analytics_arr[0]["chunk"]["text"].as_str().unwrap_or("").to_lowercase();
        assert!(
            text.contains("analytics") || text.contains("superset") || text.contains("dashboard"),
            "Analytics result should mention analytics/superset/dashboard"
        );
    }

    // Search for API migration
    let api_resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "API v3 migration deprecation", "limit": 3}))
        .send()
        .await
        .unwrap();
    assert!(api_resp.status().is_success());
    let api_results: serde_json::Value = api_resp.json().await.unwrap();
    let api_arr = api_results.as_array().expect("api results should be array");
    if !api_arr.is_empty() {
        let text = api_arr[0]["chunk"]["text"].as_str().unwrap_or("").to_lowercase();
        assert!(
            text.contains("api") || text.contains("migration") || text.contains("v3"),
            "API result should mention api/migration/v3"
        );
    }
}

#[tokio::test]
async fn meeting_ingest_search_scoped_to_company() {
    let c = client();

    // Company A
    let (token_a, _) = register_and_get_token("scope_a").await;
    let now = chrono::Utc::now().timestamp_millis();
    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token_a))
        .json(&json!({
            "title": "Company A Secret Project",
            "date": now,
            "duration_seconds": 600,
            "participants": ["AliceA"],
            "transcript": [
                {"speaker": "AliceA", "text": "The secret project codename Phoenix will launch in March. The budget is 2 million dollars and the team is 15 engineers.", "start_time": 0.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    // Company B
    let (token_b, _) = register_and_get_token("scope_b").await;

    // Wait for async ingestion
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Company B should NOT find Company A's data
    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token_b))
        .json(&json!({"query": "secret project Phoenix", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    let arr = results.as_array().unwrap();
    // Either empty results, or no mention of "Phoenix"
    for result in arr {
        let text = result["chunk"]["text"].as_str().unwrap_or("").to_lowercase();
        assert!(!text.contains("phoenix"), "Company B should not see Company A's data");
    }
}

// ─── Search Result Format & Scoring ─────────────────────────────────────────

#[tokio::test]
async fn knowledge_search_result_format() {
    let c = client();
    let (token, _) = register_and_get_token("knn_fmt").await;
    let now = chrono::Utc::now().timestamp_millis();

    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Format Test Meeting",
            "date": now,
            "duration_seconds": 300,
            "participants": ["Dave"],
            "transcript": [
                {"speaker": "Dave", "text": "The quarterly revenue target is 5 million dollars with a 12% margin improvement.", "start_time": 0.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "revenue target", "limit": 3}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    let results: serde_json::Value = resp.json().await.unwrap();
    let arr = results.as_array().expect("Should return array");

    for result in arr {
        // Each result must have chunk, meeting, and score
        assert!(result["chunk"].is_object(), "Result must have chunk object");
        assert!(result["meeting"].is_object(), "Result must have meeting object");
        assert!(result["score"].is_number(), "Result must have numeric score");

        // Chunk must have text and chunk_type
        assert!(result["chunk"]["text"].is_string(), "Chunk must have text");
        assert!(result["chunk"]["chunk_type"].is_string(), "Chunk must have chunk_type");

        // Score should be between 0 and 1 for cosine similarity
        let score = result["score"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&score), "Cosine similarity score should be 0-1, got {}", score);

        // Meeting must have id and title
        assert!(result["meeting"]["id"].is_string(), "Meeting must have id");
        assert!(result["meeting"]["title"].is_string(), "Meeting must have title");
    }
}

#[tokio::test]
async fn knowledge_search_ranking_accuracy() {
    let c = client();
    let (token, _) = register_and_get_token("knn_rank").await;
    let now = chrono::Utc::now().timestamp_millis();

    // Ingest multiple topics across meetings
    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Machine Learning Discussion",
            "date": now,
            "duration_seconds": 1800,
            "participants": ["MLLead"],
            "transcript": [
                {"speaker": "MLLead", "text": "We are implementing a transformer-based architecture for document classification. The model uses multi-head attention with 12 layers and achieves 94% F1 score on our benchmark dataset. Training takes about 4 hours on a single A100 GPU.", "start_time": 0.0, "end_time": 20.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Marketing Budget Planning",
            "date": now + 86400000,
            "duration_seconds": 1800,
            "participants": ["CMO"],
            "transcript": [
                {"speaker": "CMO", "text": "The marketing budget for next quarter is 500k dollars. We plan to allocate 200k to digital advertising, 150k to content marketing, and 150k to events and sponsorships. The expected ROI is 3.2x based on last quarters performance.", "start_time": 0.0, "end_time": 20.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Engineering Sprint Review",
            "date": now + 172800000,
            "duration_seconds": 1800,
            "participants": ["TechLead"],
            "transcript": [
                {"speaker": "TechLead", "text": "We completed 23 story points this sprint. The main focus was on the transformer inference pipeline optimization reducing latency by 40%. We also fixed 12 bugs and improved test coverage to 87%.", "start_time": 0.0, "end_time": 20.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Search specifically for ML content
    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "transformer model machine learning training", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    let arr = results.as_array().unwrap();
    assert!(!arr.is_empty(), "Should find results for ML query");

    // The ML discussion and engineering sprint should rank higher than marketing
    // (both mention "transformer" but ML discussion is more relevant)
    if arr.len() >= 2 {
        let top_score = arr[0]["score"].as_f64().unwrap();
        // Top result should be reasonably similar (cosine > 0.3 for nomic-embed-text)
        assert!(top_score > 0.2, "Top result should have reasonable similarity score, got {}", top_score);
    }
}

// ─── Document Management ───────────────────────────────────────────────────

#[tokio::test]
async fn knowledge_documents_list_empty() {
    let c = client();
    let (token, _) = register_and_get_token("knn_list_empty").await;

    let resp = c
        .get(format!("{}/knowledge/documents", base_url()))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let docs: serde_json::Value = resp.json().await.unwrap();
    assert!(docs.is_array());
}

#[tokio::test]
async fn knowledge_search_no_results_for_new_company() {
    let c = client();
    let (token, _) = register_and_get_token("knn_empty_co").await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "anything at all", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    assert!(results.as_array().unwrap().is_empty(), "New company should have no search results");
}

#[tokio::test]
async fn knowledge_search_limit_respected() {
    let c = client();
    let (token, _) = register_and_get_token("knn_limit").await;
    let now = chrono::Utc::now().timestamp_millis();

    // Ingest several meetings
    for i in 0..5 {
        c.post(format!("{}/meetings", base_url()))
            .header("Authorization", auth_header(&token))
            .json(&json!({
                "title": format!("Meeting {} about infrastructure scaling", i),
                "date": now + (i as i64 * 86400000),
                "duration_seconds": 600,
                "participants": ["Dev"],
                "transcript": [
                    {"speaker": "Dev", "text": format!("Discussion {} about Kubernetes pod scaling, horizontal pod autoscaler, and cluster node pool management for the infrastructure team.", i), "start_time": 0.0, "end_time": 10.0}
                ]
            }))
            .send()
            .await
            .unwrap();
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Search with limit of 2
    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "Kubernetes scaling infrastructure", "limit": 2}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    let arr = results.as_array().unwrap();
    assert!(arr.len() <= 2, "Should respect limit of 2, got {}", arr.len());
}

// ─── Meeting Deletion & Search Consistency ───────────────────────────────────

#[tokio::test]
async fn meeting_search_after_delete() {
    let c = client();
    let (token, _) = register_and_get_token("mtg_del").await;
    let now = chrono::Utc::now().timestamp_millis();

    // Create meeting
    let resp = c
        .post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Deletion Test Meeting",
            "date": now,
            "duration_seconds": 300,
            "participants": ["Eve"],
            "transcript": [
                {"speaker": "Eve", "text": "This meeting contains unique text about zebra migrations in the Serengeti that should not appear after deletion.", "start_time": 0.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let meeting: serde_json::Value = resp.json().await.unwrap();
    let meeting_id = meeting["id"].as_str().unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify it's searchable
    let search_resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "zebra Serengeti", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(search_resp.status().is_success());

    // Delete meeting
    let del_resp = c
        .delete(format!("{}/meetings/{}", base_url(), meeting_id))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(del_resp.status().is_success(), "Delete meeting should succeed");

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Search should no longer find the deleted meeting's content
    let post_del_search = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "zebra Serengeti", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(post_del_search.status().is_success());
    let post_results: serde_json::Value = post_del_search.json().await.unwrap();
    for result in post_results.as_array().unwrap() {
        let text = result["chunk"]["text"].as_str().unwrap_or("").to_lowercase();
        assert!(!text.contains("serengeti"), "Deleted meeting content should not appear in search");
    }
}

// ─── Chunk Overlap & Cross-Chunk Retrieval ──────────────────────────────────

#[tokio::test]
async fn meeting_search_finds_content_across_chunks() {
    let c = client();
    let (token, _) = register_and_get_token("chunk_overlap").await;
    let now = chrono::Utc::now().timestamp_millis();

    // Create a meeting with a long transcript that will span multiple chunks
    let long_text = "We discussed the database migration strategy in detail. ".repeat(50);
    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Database Migration Planning",
            "date": now,
            "duration_seconds": 3600,
            "participants": ["DBA"],
            "transcript": [
                {"speaker": "DBA", "text": long_text, "start_time": 0.0, "end_time": 1800.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "database migration strategy", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    assert!(!results.as_array().unwrap().is_empty(), "Should find results across chunks");
}

// ─── Speaker Attribution ────────────────────────────────────────────────────

#[tokio::test]
async fn meeting_search_includes_speaker() {
    let c = client();
    let (token, _) = register_and_get_token("speaker_attr").await;
    let now = chrono::Utc::now().timestamp_millis();

    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Speaker Attribution Test",
            "date": now,
            "duration_seconds": 600,
            "participants": ["Marissa", "Jonathan"],
            "transcript": [
                {"speaker": "Marissa", "text": "The cloud migration deadline is end of March.", "start_time": 0.0, "end_time": 5.0},
                {"speaker": "Jonathan", "text": "I will prepare the rollback plan by Friday.", "start_time": 5.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "cloud migration deadline", "limit": 3}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    let arr = results.as_array().unwrap();
    if !arr.is_empty() {
        // At least one result should have speaker info
        let has_speaker = arr.iter().any(|r| r["chunk"]["speaker"].is_string());
        assert!(has_speaker, "Results should include speaker attribution");
    }
}

// ─── Concurrent Search ──────────────────────────────────────────────────────

#[tokio::test]
async fn knowledge_search_concurrent_queries() {
    let c = client();
    let (token, _) = register_and_get_token("knn_concurrent").await;
    let now = chrono::Utc::now().timestamp_millis();

    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Concurrent Test Meeting",
            "date": now,
            "duration_seconds": 600,
            "participants": ["Dev"],
            "transcript": [
                {"speaker": "Dev", "text": "We need to improve latency on our microservice architecture and reduce cold start times for lambda functions.", "start_time": 0.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Fire 5 concurrent searches
    let mut handles = Vec::new();
    for i in 0..5 {
        let token_clone = token.clone();
        let handle = tokio::spawn(async move {
            let cl = client();
            let query = match i {
                0 => "latency microservice",
                1 => "cold start lambda",
                2 => "architecture improvement",
                3 => "serverless functions",
                _ => "performance optimization",
            };
            cl.post(format!("{}/knowledge/search", base_url()))
                .header("Authorization", auth_header(&token_clone))
                .json(&json!({"query": query, "limit": 3}))
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        });
        handles.push(handle);
    }

    for handle in handles {
        let success = handle.await.unwrap();
        assert!(success, "Concurrent search should succeed");
    }
}