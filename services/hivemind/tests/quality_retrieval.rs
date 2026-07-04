//! Gold-set retrieval quality eval for Hivemind's knowledge search (#71).
//!
//! Ingests a small labeled set of meetings with distinctive, non-overlapping content, then for
//! each gold question checks whether the expected source meeting is retrieved — recall@k for
//! k in {1, 3, 5} and top-1 "source accuracy". This is the actual TKT4 "lab result" artifact:
//! a measured number, not just a pass/fail smoke test.
//!
//! Run with: `cargo test --test quality_retrieval -- --nocapture` (needs a live embedding
//! service, same as the other embedding-dependent tests in this crate — skips otherwise).

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

async fn register_and_get_token(suffix: &str) -> String {
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
    body["token"].as_str().unwrap().to_string()
}

struct GoldEntry {
    id: &'static str,
    meeting_title: &'static str,
    transcript: &'static str,
    query: &'static str,
}

/// Fresh, distinctive content per entry — avoids ambiguous cross-matches between gold
/// questions when all 15 are ingested into the same test company.
const GOLD_SET: &[GoldEntry] = &[
    GoldEntry { id: "g01", meeting_title: "Vendor Contract Renewal", transcript: "We are renewing the Salesforce contract for 18 months at a 7% discount, contingent on adding the Einstein Analytics add-on.", query: "What discount did we get on the Salesforce renewal?" },
    GoldEntry { id: "g02", meeting_title: "Warehouse Safety Audit", transcript: "The forklift certification for the Denver warehouse expired last month; all operators need recertification by end of quarter.", query: "Which warehouse has expired forklift certifications?" },
    GoldEntry { id: "g03", meeting_title: "Customer Churn Review", transcript: "Churn in the SMB segment rose to 4.2% this quarter, mostly driven by pricing complaints from customers on the legacy Bronze tier.", query: "Why is SMB churn increasing?" },
    GoldEntry { id: "g04", meeting_title: "Hiring Plan Q1", transcript: "We are opening three new backend engineering roles in the payments team and one staff-level role for the platform team.", query: "How many backend engineering roles are open in payments?" },
    GoldEntry { id: "g05", meeting_title: "Office Lease Negotiation", transcript: "The Austin office lease renewal came in at $34 per square foot, up from $29, and the landlord agreed to a two-year cap on increases.", query: "What is the new price per square foot for the Austin office?" },
    GoldEntry { id: "g06", meeting_title: "Incident Postmortem", transcript: "The outage was caused by a misconfigured connection pool limit in the payments database that exhausted connections under load.", query: "What caused the payments database outage?" },
    GoldEntry { id: "g07", meeting_title: "Brand Refresh Kickoff", transcript: "The new logo will ship with a teal and charcoal palette, replacing the old blue, and rollout is planned for the annual conference in September.", query: "What colors are in the new brand palette?" },
    GoldEntry { id: "g08", meeting_title: "Supplier Risk Assessment", transcript: "Our primary aluminum supplier in Vietnam has a single-source risk; procurement is qualifying a second supplier in Mexico as backup.", query: "Where is procurement qualifying a backup aluminum supplier?" },
    GoldEntry { id: "g09", meeting_title: "Patent Filing Update", transcript: "The provisional patent for the adaptive noise-cancellation algorithm was filed last Tuesday and the full application is due within 12 months.", query: "What algorithm does the new provisional patent cover?" },
    GoldEntry { id: "g10", meeting_title: "Employee Benefits Review", transcript: "Starting next enrollment period, the dental plan adds orthodontic coverage for dependents up to age 26.", query: "What new dental coverage is being added for dependents?" },
    GoldEntry { id: "g11", meeting_title: "Data Retention Policy", transcript: "Support ticket transcripts will now be purged after 18 months instead of being kept indefinitely, per the updated data retention policy.", query: "How long are support ticket transcripts retained now?" },
    GoldEntry { id: "g12", meeting_title: "Localization Sprint Planning", transcript: "German and Japanese translations are complete; Portuguese and Korean are still in progress and blocked on missing glossary terms.", query: "Which language translations are blocked on glossary terms?" },
    GoldEntry { id: "g13", meeting_title: "Fleet Maintenance Schedule", transcript: "Delivery van 14 needs a transmission replacement and will be out of rotation for approximately two weeks starting Monday.", query: "Which delivery van needs a transmission replacement?" },
    GoldEntry { id: "g14", meeting_title: "Conference Sponsorship Decision", transcript: "We are sponsoring DevConf at the Gold tier for $40,000, which includes a booth and two speaking slots.", query: "What sponsorship tier are we taking at DevConf?" },
    GoldEntry { id: "g15", meeting_title: "Firmware Rollback Plan", transcript: "Firmware version 3.2.1 caused battery drain on the sensor units, so we are rolling back to 3.1.9 for all units shipped after March.", query: "Which firmware version caused battery drain on the sensors?" },
];

async fn ingest_gold_entry(c: &Client, token: &str, entry: &GoldEntry) {
    let now = chrono::Utc::now().timestamp_millis();
    let resp = c
        .post(format!("{}/meetings", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "title": entry.meeting_title,
            "date": now,
            "duration_seconds": 600,
            "participants": ["Speaker"],
            "transcript": [
                {"speaker": "Speaker", "text": entry.transcript, "start_time": 0.0, "end_time": 20.0}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Failed to ingest gold entry {}: {}",
        entry.id,
        resp.status()
    );
}

async fn search_ranks(c: &Client, token: &str, query: &str, limit: usize) -> Vec<String> {
    let resp = c
        .post(format!("{}/knowledge/search", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({"query": query, "limit": limit}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Search failed: {}", resp.status());
    let results: serde_json::Value = resp.json().await.unwrap();
    results
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["meeting"]["title"].as_str().map(String::from))
        .collect()
}

#[tokio::test]
async fn gold_set_retrieval_quality() {
    if !embedding_available().await {
        eprintln!("SKIP: embedding service not available");
        return;
    }
    let c = client();
    let token = register_and_get_token("gold_retrieval").await;

    for entry in GOLD_SET {
        ingest_gold_entry(&c, &token, entry).await;
    }
    // Batch indexing settle time — 15 entries ingested back-to-back, more headroom than the
    // single-meeting 2-3s sleeps elsewhere in this suite.
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;

    let mut hit_at_1 = 0usize;
    let mut hit_at_3 = 0usize;
    let mut hit_at_5 = 0usize;
    let mut rows = Vec::new();

    for entry in GOLD_SET {
        let ranked_titles = search_ranks(&c, &token, entry.query, 5).await;
        let rank = ranked_titles.iter().position(|t| t == entry.meeting_title);

        if rank == Some(0) {
            hit_at_1 += 1;
        }
        if matches!(rank, Some(r) if r < 3) {
            hit_at_3 += 1;
        }
        if rank.is_some() {
            hit_at_5 += 1;
        }
        rows.push((entry.id, entry.meeting_title, rank));
    }

    let n = GOLD_SET.len();
    let pct = |hits: usize| (hits as f64 / n as f64) * 100.0;

    println!("\n=== Gold-set retrieval quality ({} questions) ===", n);
    for (id, title, rank) in &rows {
        let rank_str = match rank {
            Some(r) => format!("rank {}", r + 1),
            None => "not in top 5".to_string(),
        };
        println!("  {:<5} {:<32} {}", id, title, rank_str);
    }
    println!(
        "recall@1 (source accuracy): {}/{} ({:.0}%)",
        hit_at_1, n, pct(hit_at_1)
    );
    println!("recall@3: {}/{} ({:.0}%)", hit_at_3, n, pct(hit_at_3));
    println!("recall@5: {}/{} ({:.0}%)", hit_at_5, n, pct(hit_at_5));

    // Thresholds, not 100% — embedding-based semantic search on paraphrased queries won't be
    // perfect, and this is a regression gate, not a claim of flawless retrieval. Tune these
    // based on the embedding model actually in use if they prove too strict/loose in practice.
    assert!(
        pct(hit_at_5) >= 80.0,
        "recall@5 {:.0}% below 80% threshold — retrieval quality regression?",
        pct(hit_at_5)
    );
    assert!(
        pct(hit_at_3) >= 65.0,
        "recall@3 {:.0}% below 65% threshold",
        pct(hit_at_3)
    );
}
