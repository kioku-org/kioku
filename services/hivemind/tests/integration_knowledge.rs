use reqwest::Client;
use serde_json::json;

fn base_url() -> String {
    std::env::var("HIVEMIND_URL").unwrap_or_else(|_| "http://localhost:9100".into())
}

fn client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap()
}

async fn embedding_available() -> bool {
    let url =
        std::env::var("EMBEDDING_API_URL").unwrap_or_else(|_| "http://localhost:11434".into());
    Client::new()
        .get(format!("{}/api/tags", url))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
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
    assert!(
        resp.status().is_success(),
        "Register failed: {}",
        resp.status()
    );
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
    assert!(
        resp.status().is_success() || resp.status() == 400,
        "Empty query should return 200 (empty) or 400, got {}",
        resp.status()
    );
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
    assert!(
        !resp.status().is_success(),
        "Missing query should be rejected"
    );
}

#[tokio::test]
async fn knowledge_upload_non_pdf_rejected() {
    let c = client();
    let (token, _) = register_and_get_token("knn_upload").await;

    let resp = c
        .post(format!("{}/knowledge/documents", base_url()))
        .header("Authorization", auth_header(&token))
        .multipart(reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::text("not a pdf").file_name("test.txt"),
        ))
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
        .multipart(reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::text("").file_name("empty.pdf"),
        ))
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
    assert!(
        results.as_array().unwrap().is_empty(),
        "New company should have no search results"
    );
}

// ─── Embedding-dependent tests ──────────────────────────────────────────────
// These require a running embedding service (e.g. Ollama). They skip in CI
// and run on RunPod where the embedding service is available.

#[tokio::test]
async fn meeting_ingest_then_search() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
    let (token, _) = register_and_get_token("mtg_search").await;
    let now = chrono::Utc::now().timestamp_millis();

    let resp = c
        .post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Q4 Product Roadmap Review",
            "date": now,
            "duration_seconds": 3600,
            "participants": ["Alice", "Bob", "Carol"],
            "transcript": [
                {"speaker": "Alice", "text": "We need to finalize the Q4 product roadmap. The key priorities are mobile app redesign, API v3 migration, and the new analytics dashboard.", "start_time": 0.0, "end_time": 15.0},
                {"speaker": "Bob", "text": "The mobile redesign is 70% complete. We are using React Native for the new UI components. ETA for completion is end of October.", "start_time": 15.0, "end_time": 35.0},
                {"speaker": "Carol", "text": "For the analytics dashboard, we have chosen Apache Superset as the visualization layer. The proof of concept showed a 40% improvement in query latency.", "start_time": 35.0, "end_time": 60.0}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Ingest failed: {}",
        resp.status()
    );
    let meeting: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(meeting["title"], "Q4 Product Roadmap Review");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let search_resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({"query": "mobile redesign React Native", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(
        search_resp.status().is_success(),
        "Search failed: {}",
        search_resp.status()
    );
    let results: serde_json::Value = search_resp.json().await.unwrap();
    let arr = results.as_array().unwrap();
    assert!(
        !arr.is_empty(),
        "Should find results for mobile redesign query"
    );

    let first = &arr[0];
    let chunk_text = first["chunk"]["text"].as_str().unwrap_or("").to_lowercase();
    assert!(
        chunk_text.contains("mobile")
            || chunk_text.contains("redesign")
            || chunk_text.contains("react"),
        "Top result should contain mobile/redesign/React, got: {}",
        chunk_text
    );
    assert!(
        first["meeting"]["id"].is_string(),
        "Meeting ID should be present"
    );
}

#[tokio::test]
async fn meeting_ingest_search_scoped_to_company() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
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
                {"speaker": "AliceA", "text": "The secret project codename Phoenix will launch in March. The budget is 2 million dollars.", "start_time": 0.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    let (token_b, _) = register_and_get_token("scope_b").await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", auth_header(&token_b))
        .json(&json!({"query": "secret project Phoenix", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let results: serde_json::Value = resp.json().await.unwrap();
    for result in results.as_array().unwrap() {
        let text = result["chunk"]["text"]
            .as_str()
            .unwrap_or("")
            .to_lowercase();
        assert!(
            !text.contains("phoenix"),
            "Company B should not see Company A's data"
        );
    }
}

#[tokio::test]
async fn knowledge_search_result_format() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
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
    for result in results.as_array().unwrap() {
        assert!(result["chunk"].is_object(), "Result must have chunk object");
        assert!(
            result["meeting"].is_object(),
            "Result must have meeting object"
        );
        assert!(
            result["score"].is_number(),
            "Result must have numeric score"
        );
        assert!(result["chunk"]["text"].is_string(), "Chunk must have text");
        let score = result["score"].as_f64().unwrap();
        assert!(
            (0.0..=1.0).contains(&score),
            "Score should be 0-1, got {}",
            score
        );
        assert!(result["meeting"]["id"].is_string(), "Meeting must have id");
    }
}

#[tokio::test]
async fn knowledge_search_ranking_accuracy() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
    let (token, _) = register_and_get_token("knn_rank").await;
    let now = chrono::Utc::now().timestamp_millis();

    c.post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Machine Learning Discussion",
            "date": now,
            "duration_seconds": 1800,
            "participants": ["MLLead"],
            "transcript": [
                {"speaker": "MLLead", "text": "We are implementing a transformer-based architecture for document classification. The model uses multi-head attention with 12 layers and achieves 94% F1 score.", "start_time": 0.0, "end_time": 20.0}
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
                {"speaker": "CMO", "text": "The marketing budget for next quarter is 500k dollars. We plan to allocate 200k to digital advertising.", "start_time": 0.0, "end_time": 20.0}
            ]
        }))
        .send()
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

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
}

#[tokio::test]
async fn knowledge_search_limit_respected() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
    let (token, _) = register_and_get_token("knn_limit").await;
    let now = chrono::Utc::now().timestamp_millis();

    for i in 0..5 {
        c.post(format!("{}/meetings", base_url()))
            .header("Authorization", auth_header(&token))
            .json(&json!({
                "title": format!("Meeting {} about infrastructure scaling", i),
                "date": now + (i as i64 * 86400000),
                "duration_seconds": 600,
                "participants": ["Dev"],
                "transcript": [
                    {"speaker": "Dev", "text": format!("Discussion {} about Kubernetes pod scaling, horizontal pod autoscaler, and cluster node pool management.", i), "start_time": 0.0, "end_time": 10.0}
                ]
            }))
            .send()
            .await
            .unwrap();
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

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
    assert!(
        arr.len() <= 2,
        "Should respect limit of 2, got {}",
        arr.len()
    );
}

#[tokio::test]
async fn meeting_search_after_delete() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
    let (token, _) = register_and_get_token("mtg_del").await;
    let now = chrono::Utc::now().timestamp_millis();

    let resp = c
        .post(format!("{}/meetings", base_url()))
        .header("Authorization", auth_header(&token))
        .json(&json!({
            "title": "Deletion Test Meeting",
            "date": now,
            "duration_seconds": 300,
            "participants": ["Eve"],
            "transcript": [
                {"speaker": "Eve", "text": "This meeting contains unique text about zebra migrations in the Serengeti.", "start_time": 0.0, "end_time": 10.0}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let meeting: serde_json::Value = resp.json().await.unwrap();
    let meeting_id = meeting["id"].as_str().unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let del_resp = c
        .delete(format!("{}/meetings/{}", base_url(), meeting_id))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(del_resp.status().is_success(), "Delete should succeed");

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

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
        let text = result["chunk"]["text"]
            .as_str()
            .unwrap_or("")
            .to_lowercase();
        assert!(
            !text.contains("serengeti"),
            "Deleted meeting content should not appear"
        );
    }
}

#[tokio::test]
async fn meeting_search_finds_content_across_chunks() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
    let (token, _) = register_and_get_token("chunk_overlap").await;
    let now = chrono::Utc::now().timestamp_millis();

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
    assert!(
        !results.as_array().unwrap().is_empty(),
        "Should find results across chunks"
    );
}

#[tokio::test]
async fn meeting_search_includes_speaker() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
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
        let has_speaker = arr.iter().any(|r| r["chunk"]["speaker"].is_string());
        assert!(has_speaker, "Results should include speaker attribution");
    }
}

#[tokio::test]
async fn knowledge_search_concurrent_queries() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
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
