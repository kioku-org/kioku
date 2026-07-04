//! Internal callback handlers — /bots/internal/callback/*. Faithful port of callbacks.py's
//! status-transition logic (the Pack J classifier, the state machine, forensic-field capture).
//!
//! ponytail: `finalize_recording_master` (recording_finalizer.py) and `run_all_tasks`
//! (post_meeting.py) are stubbed — those subsystems aren't ported yet (see meeting-api-rs task
//! list). Every status transition here is real and correct; the recording-file finalization
//! and post-meeting hooks (webhook delivery, hivemind ingest, transcription aggregation) don't
//! run yet as a result.

use crate::classifier::classify_stopped_exit;
use crate::meeting_status::{publish_meeting_status_change, schedule_status_webhook_task, update_meeting_status, StatusUpdateOptions};
use crate::models::Meeting;
use crate::schemas::{failure_stage_from_status, MeetingCompletionReason, MeetingStatus};
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;
use serde_json::{json, Value};

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({"detail": detail}))).into_response()
}

async fn stub_finalize_recording_master(meeting_id: i32) {
    tracing::debug!(meeting_id, "finalize_recording_master not yet ported — skipping");
}

async fn stub_run_all_tasks(meeting_id: i32) {
    tracing::debug!(meeting_id, "post_meeting run_all_tasks not yet ported — skipping");
}

async fn find_meeting_by_session(state: &AppState, session_uid: &str) -> Option<Meeting> {
    if let Some(rest) = session_uid.strip_prefix("bs:") {
        let meeting_id: i32 = rest.parse().ok()?;
        return sqlx::query_as("SELECT * FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.ok().flatten();
    }
    let meeting_id: Option<i32> = sqlx::query_scalar("SELECT meeting_id FROM meeting_sessions WHERE session_uid = $1")
        .bind(session_uid)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    match meeting_id {
        Some(id) => sqlx::query_as("SELECT * FROM meetings WHERE id = $1").bind(id).fetch_optional(&state.db).await.ok().flatten(),
        None => None,
    }
}

fn stop_requested(meeting: &Meeting) -> bool {
    meeting.data.get("stop_requested").and_then(|v| v.as_bool()).unwrap_or(false)
}

async fn refetch(state: &AppState, meeting_id: i32) -> Option<Meeting> {
    sqlx::query_as("SELECT * FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(&state.db).await.ok().flatten()
}

// ---------------------------------------------------------------------------
// started / joining / awaiting_admission — the three simple lifecycle callbacks
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BotStartupCallbackPayload {
    pub connection_id: String,
    pub container_id: Option<String>,
}

pub async fn bot_startup_callback(State(state): State<AppState>, Json(payload): Json<BotStartupCallbackPayload>) -> Response {
    let Some(meeting) = find_meeting_by_session(&state, &payload.connection_id).await else {
        return Json(json!({"status": "error", "detail": "Meeting session not found"})).into_response();
    };
    if stop_requested(&meeting) {
        return Json(json!({"status": "ignored", "detail": "stop requested"})).into_response();
    }

    let old_status = meeting.status.clone();
    let current = MeetingStatus::parse(&meeting.status).unwrap_or(MeetingStatus::Failed);

    if matches!(current, MeetingStatus::Requested | MeetingStatus::Joining | MeetingStatus::AwaitingAdmission | MeetingStatus::Failed) {
        // v0.10.5 Pack X: REQUESTED -> ACTIVE directly is illegal; drive through JOINING first.
        if current == MeetingStatus::Requested {
            let _ = update_meeting_status(&state.db, meeting.id, MeetingStatus::Joining, StatusUpdateOptions::default()).await;
        }
        if update_meeting_status(&state.db, meeting.id, MeetingStatus::Active, StatusUpdateOptions::default()).await.unwrap_or(false) {
            if let Some(cid) = &payload.container_id {
                let _ = sqlx::query("UPDATE meetings SET bot_container_id = $1, start_time = now() WHERE id = $2")
                    .bind(cid)
                    .bind(meeting.id)
                    .execute(&state.db)
                    .await;
            } else {
                let _ = sqlx::query("UPDATE meetings SET start_time = now() WHERE id = $1").bind(meeting.id).execute(&state.db).await;
            }
        }
    } else if current == MeetingStatus::Active {
        if let Some(cid) = &payload.container_id {
            let _ = sqlx::query("UPDATE meetings SET bot_container_id = $1 WHERE id = $2").bind(cid).bind(meeting.id).execute(&state.db).await;
        }
    }

    let Some(fresh) = refetch(&state, meeting.id).await else {
        return Json(json!({"status": "error", "detail": "meeting vanished"})).into_response();
    };
    if fresh.status == "active" && old_status != "active" {
        publish_meeting_status_change(&state, fresh.id, "active", &fresh.platform, fresh.platform_specific_id.as_deref().unwrap_or(""), fresh.user_id, None).await;
        schedule_status_webhook_task(fresh.id, &old_status, "active").await;
    }

    Json(json!({"status": "startup processed", "meeting_id": fresh.id, "meeting_status": fresh.status})).into_response()
}

pub async fn bot_joining_callback(State(state): State<AppState>, Json(payload): Json<BotStartupCallbackPayload>) -> Response {
    let Some(meeting) = find_meeting_by_session(&state, &payload.connection_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Meeting session not found");
    };
    if stop_requested(&meeting) {
        return Json(json!({"status": "ignored", "detail": "stop requested"})).into_response();
    }
    let old_status = meeting.status.clone();
    let success = update_meeting_status(&state.db, meeting.id, MeetingStatus::Joining, StatusUpdateOptions::default()).await.unwrap_or(false);
    if success {
        publish_meeting_status_change(&state, meeting.id, "joining", &meeting.platform, meeting.platform_specific_id.as_deref().unwrap_or(""), meeting.user_id, None).await;
        schedule_status_webhook_task(meeting.id, &old_status, "joining").await;
    }
    let fresh = refetch(&state, meeting.id).await.unwrap_or(meeting);
    Json(json!({"status": "joining processed", "meeting_id": fresh.id, "meeting_status": fresh.status})).into_response()
}

pub async fn bot_awaiting_admission_callback(State(state): State<AppState>, Json(payload): Json<BotStartupCallbackPayload>) -> Response {
    let Some(meeting) = find_meeting_by_session(&state, &payload.connection_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Meeting session not found");
    };
    if stop_requested(&meeting) {
        return Json(json!({"status": "ignored", "detail": "stop requested"})).into_response();
    }
    let old_status = meeting.status.clone();
    let success = update_meeting_status(&state.db, meeting.id, MeetingStatus::AwaitingAdmission, StatusUpdateOptions::default()).await.unwrap_or(false);
    if success {
        publish_meeting_status_change(&state, meeting.id, "awaiting_admission", &meeting.platform, meeting.platform_specific_id.as_deref().unwrap_or(""), meeting.user_id, None).await;
        schedule_status_webhook_task(meeting.id, &old_status, "awaiting_admission").await;
    }
    let fresh = refetch(&state, meeting.id).await.unwrap_or(meeting);
    Json(json!({"status": "awaiting_admission processed", "meeting_id": fresh.id, "meeting_status": fresh.status})).into_response()
}

// ---------------------------------------------------------------------------
// exited — bot process exit
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BotExitCallbackPayload {
    pub connection_id: String,
    pub exit_code: i32,
    #[serde(default = "default_reason")]
    pub reason: Option<String>,
    pub error_details: Option<Value>,
    pub platform_specific_error: Option<String>,
    pub completion_reason: Option<String>,
}
fn default_reason() -> Option<String> {
    Some("self_initiated_leave".to_string())
}

pub async fn bot_exit_callback(State(state): State<AppState>, Json(payload): Json<BotExitCallbackPayload>) -> Response {
    let Some(meeting) = find_meeting_by_session(&state, &payload.connection_id).await else {
        tracing::error!(session_uid = %payload.connection_id, "Exit callback: session not found");
        return Json(json!({"status": "error", "detail": "Meeting session not found"})).into_response();
    };
    let meeting_id = meeting.id;
    let old_status = meeting.status.clone();
    let current_status = MeetingStatus::parse(&meeting.status).unwrap_or(MeetingStatus::Failed);

    let new_status: Option<String>;

    if payload.exit_code == 0 {
        let pending = meeting.data.get("pending_completion_reason").and_then(|v| v.as_str()).and_then(MeetingCompletionReason::parse);
        let provided_reason = pending
            .or_else(|| payload.completion_reason.as_deref().and_then(MeetingCompletionReason::parse))
            .unwrap_or(MeetingCompletionReason::Stopped);
        let mut meta = json!({"exit_code": payload.exit_code});
        if let Some(e) = &payload.platform_specific_error {
            meta["platform_specific_error"] = json!(e);
        }
        stub_finalize_recording_master(meeting_id).await;
        let success = update_meeting_status(
            &state.db,
            meeting_id,
            MeetingStatus::Completed,
            StatusUpdateOptions {
                completion_reason: Some(provided_reason),
                transition_reason: payload.reason.as_deref(),
                transition_metadata: Some(meta),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(false);
        new_status = success.then(|| "completed".to_string());
    } else if current_status == MeetingStatus::Stopping {
        let provided_reason = payload.completion_reason.as_deref().and_then(MeetingCompletionReason::parse).unwrap_or(MeetingCompletionReason::Stopped);

        // Reuse the bot's deferred classification from status_change if present — its
        // view at graceful-leave time is authoritative for segment counts/duration.
        let bot_class = meeting.data.get("bot_exit_classification");
        let (target_status, classified_reason) = if let Some(bc) = bot_class.filter(|v| v.get("target_status").is_some()) {
            let ts = bc.get("target_status").and_then(|v| v.as_str()).and_then(MeetingStatus::parse);
            let cr = bc.get("completion_reason").and_then(|v| v.as_str()).and_then(MeetingCompletionReason::parse);
            match ts {
                Some(ts) => (ts, cr.unwrap_or(provided_reason)),
                None => classify_stopped_exit(&state.db, meeting_id, &meeting.data, meeting.start_time, meeting.end_time, provided_reason).await,
            }
        } else {
            classify_stopped_exit(&state.db, meeting_id, &meeting.data, meeting.start_time, meeting.end_time, provided_reason).await
        };

        let meta = json!({"exit_code": payload.exit_code, "original_reason": payload.reason, "pack_j_classification": classified_reason.as_str()});
        stub_finalize_recording_master(meeting_id).await;
        let success = update_meeting_status(
            &state.db,
            meeting_id,
            target_status,
            StatusUpdateOptions { completion_reason: Some(classified_reason), transition_reason: payload.reason.as_deref(), transition_metadata: Some(meta), ..Default::default() },
        )
        .await
        .unwrap_or(false);
        new_status = success.then(|| target_status.as_str().to_string());
    } else {
        let bot_reason_map: &[(&str, MeetingCompletionReason)] = &[
            ("self_initiated_leave", MeetingCompletionReason::Stopped),
            ("evicted", MeetingCompletionReason::Evicted),
            ("removed_by_host", MeetingCompletionReason::Evicted),
            ("removed_by_admin", MeetingCompletionReason::Evicted),
            ("left_alone", MeetingCompletionReason::LeftAlone),
            ("left_alone_timeout", MeetingCompletionReason::LeftAlone),
            ("startup_alone_timeout", MeetingCompletionReason::LeftAlone),
            ("meeting_ended_by_host", MeetingCompletionReason::Stopped),
            ("normal_completion", MeetingCompletionReason::Stopped),
            ("post_join_setup_error", MeetingCompletionReason::Stopped),
            ("admission_timeout", MeetingCompletionReason::AwaitingAdmissionTimeout),
            ("admission_rejected_by_admin", MeetingCompletionReason::AwaitingAdmissionRejected),
            ("admission_false_positive", MeetingCompletionReason::Stopped),
            ("stop_requested_pre_admission", MeetingCompletionReason::StoppedBeforeAdmission),
            ("missing_meeting_url", MeetingCompletionReason::ValidationError),
            ("join_meeting_error", MeetingCompletionReason::JoinFailure),
        ];
        let derived = payload
            .completion_reason
            .as_deref()
            .and_then(MeetingCompletionReason::parse)
            .or_else(|| payload.reason.as_deref().and_then(|r| bot_reason_map.iter().find(|(k, _)| *k == r).map(|(_, v)| *v)))
            .unwrap_or(MeetingCompletionReason::Stopped);

        let (target_status, classified_reason) = classify_stopped_exit(&state.db, meeting_id, &meeting.data, meeting.start_time, meeting.end_time, derived).await;

        let mut meta = json!({"exit_code": payload.exit_code, "original_reason": payload.reason, "pack_j_classification": classified_reason.as_str()});
        if let Some(e) = &payload.platform_specific_error {
            meta["platform_specific_error"] = json!(e);
        }

        let mut opts = StatusUpdateOptions { completion_reason: Some(classified_reason), transition_reason: payload.reason.as_deref(), transition_metadata: Some(meta), ..Default::default() };
        let error_details_owned;
        if target_status == MeetingStatus::Failed {
            opts.failure_stage = Some(failure_stage_from_status(current_status));
            error_details_owned = format!("Bot exited with code {}{}", payload.exit_code, payload.reason.as_deref().map(|r| format!("; reason: {r}")).unwrap_or_default());
            opts.error_details = Some(&error_details_owned);
        }
        stub_finalize_recording_master(meeting_id).await;
        let success = update_meeting_status(&state.db, meeting_id, target_status, opts).await.unwrap_or(false);
        new_status = success.then(|| target_status.as_str().to_string());
    }

    let Some(fresh) = refetch(&state, meeting_id).await else {
        return Json(json!({"status": "error", "detail": "meeting vanished"})).into_response();
    };

    // Clean up browser_session Redis keys — unconditional, mirrors Python's chat-persistence
    // race guard (status update can legitimately fail on an idempotent re-fire).
    let mut redis = state.redis.clone();
    if let Some(token) = fresh.data.get("session_token").and_then(|v| v.as_str()) {
        let _: Result<(), _> = redis::AsyncCommands::del(&mut redis, format!("browser_session:{token}")).await;
    }
    let _: Result<(), _> = redis::AsyncCommands::del(&mut redis, format!("browser_session:{}", fresh.id)).await;

    if let Some(ns) = &new_status {
        publish_meeting_status_change(&state, fresh.id, ns, &fresh.platform, fresh.platform_specific_id.as_deref().unwrap_or(""), fresh.user_id, None).await;
        schedule_status_webhook_task(fresh.id, &old_status, ns).await;
    }
    stub_run_all_tasks(fresh.id).await;

    Json(json!({"status": "callback processed", "meeting_id": fresh.id, "final_status": fresh.status})).into_response()
}

// ---------------------------------------------------------------------------
// status_change — unified callback for all bot-reported status changes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BotStatusChangePayload {
    pub connection_id: String,
    pub container_id: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub exit_code: Option<i32>,
    pub error_details: Option<Value>,
    pub platform_specific_error: Option<String>,
    pub completion_reason: Option<String>,
}

pub async fn bot_status_change_callback(State(state): State<AppState>, Json(payload): Json<BotStatusChangePayload>) -> Response {
    let Some(meeting) = find_meeting_by_session(&state, &payload.connection_id).await else {
        return json_error(StatusCode::NOT_FOUND, "Meeting session not found");
    };
    let Some(new_status) = MeetingStatus::parse(&payload.status) else {
        return json_error(StatusCode::BAD_REQUEST, "Unknown status value");
    };

    if stop_requested(&meeting) && !matches!(new_status, MeetingStatus::Completed | MeetingStatus::Failed) {
        schedule_status_webhook_task(meeting.id, &meeting.status, new_status.as_str()).await;
        return Json(json!({"status": "ignored", "detail": "stop requested"})).into_response();
    }

    let old_status = meeting.status.clone();
    let current_status = MeetingStatus::parse(&meeting.status).unwrap_or(MeetingStatus::Failed);
    let mut success: Option<bool> = None;

    if new_status == MeetingStatus::Completed {
        let pending = meeting.data.get("pending_completion_reason").and_then(|v| v.as_str()).and_then(MeetingCompletionReason::parse);
        let effective_reason = pending.or_else(|| payload.completion_reason.as_deref().and_then(MeetingCompletionReason::parse));

        if current_status == MeetingStatus::Stopping {
            if let Some(reason) = effective_reason {
                let (target_status, classified_reason) = classify_stopped_exit(&state.db, meeting.id, &meeting.data, meeting.start_time, meeting.end_time, reason).await;

                // Orphan-window fix: defer the STOPPING -> terminal transition until
                // runtime-api's exit_callback fires (container may still be running).
                // Persist the classification so exit_callback reuses it instead of
                // re-classifying from a possibly-different snapshot.
                let mut data = meeting.data.clone();
                data["bot_exit_classification"] = json!({
                    "target_status": target_status.as_str(),
                    "completion_reason": classified_reason.as_str(),
                    "bot_reported_reason": reason.as_str(),
                    "bot_signaled_at": chrono::Utc::now().to_rfc3339(),
                });
                let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(&state.db).await;

                return Json(json!({
                    "status": "deferred",
                    "detail": "bot signaled exit; waiting for runtime-api exit_callback to flip status",
                    "meeting_id": meeting.id,
                    "meeting_status": "stopping",
                }))
                .into_response();
            }
        }

        // Pack D: non-STOPPING states with an explicit failure reason route to FAILED
        // directly (no deferred orphan-window detour needed outside STOPPING).
        const CANONICAL_FAILURE: &[MeetingCompletionReason] = &[
            MeetingCompletionReason::AwaitingAdmissionTimeout,
            MeetingCompletionReason::AwaitingAdmissionRejected,
            MeetingCompletionReason::JoinFailure,
            MeetingCompletionReason::Evicted,
            MeetingCompletionReason::MaxBotTimeExceeded,
            MeetingCompletionReason::ValidationError,
            MeetingCompletionReason::StoppedBeforeAdmission,
            MeetingCompletionReason::StoppedWithNoAudio,
        ];
        let target_status = if effective_reason.map(|r| CANONICAL_FAILURE.contains(&r)).unwrap_or(false) {
            MeetingStatus::Failed
        } else {
            MeetingStatus::Completed
        };

        let ok = update_meeting_status(
            &state.db,
            meeting.id,
            target_status,
            StatusUpdateOptions { completion_reason: effective_reason, ..Default::default() },
        )
        .await
        .unwrap_or(false);
        success = Some(ok);
        if ok {
            stub_run_all_tasks(meeting.id).await;
        }
    } else if new_status == MeetingStatus::Failed {
        let error_details_owned = payload.error_details.as_ref().map(|v| v.to_string());
        let ok = update_meeting_status(
            &state.db,
            meeting.id,
            MeetingStatus::Failed,
            StatusUpdateOptions {
                completion_reason: payload.completion_reason.as_deref().and_then(MeetingCompletionReason::parse),
                failure_stage: Some(failure_stage_from_status(current_status)),
                error_details: error_details_owned.as_deref(),
                ..Default::default()
            },
        )
        .await
        .unwrap_or(false);
        success = Some(ok);
        if ok {
            stub_run_all_tasks(meeting.id).await;
        }
    } else if new_status == MeetingStatus::Active {
        if matches!(current_status, MeetingStatus::Requested | MeetingStatus::Joining | MeetingStatus::AwaitingAdmission | MeetingStatus::Failed | MeetingStatus::NeedsHumanHelp) {
            let ok = update_meeting_status(&state.db, meeting.id, MeetingStatus::Active, StatusUpdateOptions::default()).await.unwrap_or(false);
            if ok {
                if let Some(cid) = &payload.container_id {
                    let _ = sqlx::query("UPDATE meetings SET bot_container_id = $1, start_time = now() WHERE id = $2").bind(cid).bind(meeting.id).execute(&state.db).await;
                } else {
                    let _ = sqlx::query("UPDATE meetings SET start_time = now() WHERE id = $1").bind(meeting.id).execute(&state.db).await;
                }
            }
            success = Some(ok);
        } else if current_status == MeetingStatus::Active {
            if let Some(cid) = &payload.container_id {
                let _ = sqlx::query("UPDATE meetings SET bot_container_id = $1 WHERE id = $2").bind(cid).bind(meeting.id).execute(&state.db).await;
            }
            let fresh = refetch(&state, meeting.id).await.unwrap_or(meeting);
            return Json(json!({"status": "container_updated", "meeting_id": fresh.id, "meeting_status": fresh.status})).into_response();
        } else {
            success = Some(false);
        }
    } else if new_status == MeetingStatus::NeedsHumanHelp {
        let ok = update_meeting_status(&state.db, meeting.id, MeetingStatus::NeedsHumanHelp, StatusUpdateOptions::default()).await.unwrap_or(false);
        if ok {
            let session_token = meeting.data.get("session_token").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| {
                use rand::Rng;
                let bytes: Vec<u8> = (0..18).map(|_| rand::thread_rng().gen()).collect();
                use base64::Engine;
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
            });
            let escalation_reason = payload.reason.clone().unwrap_or_else(|| "unknown".to_string());
            let escalated_at = chrono::Utc::now().to_rfc3339();
            let mut data = meeting.data.clone();
            data["session_token"] = json!(session_token);
            data["escalation"] = json!({
                "reason": escalation_reason,
                "escalated_at": escalated_at,
                "session_token": session_token,
                "vnc_url": format!("/b/{session_token}"),
            });
            let _ = sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&data).bind(meeting.id).execute(&state.db).await;

            let mut redis = state.redis.clone();
            let container_name = payload.container_id.clone().or_else(|| meeting.bot_container_id.clone());
            let sess_val = json!({"container_name": container_name, "meeting_id": meeting.id, "user_id": meeting.user_id, "escalation": true}).to_string();
            let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut redis, format!("browser_session:{session_token}"), sess_val.clone(), 86400).await;
            let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut redis, format!("browser_session:{}", meeting.id), sess_val, 86400).await;
        }
        success = Some(ok);
    } else {
        let ok = update_meeting_status(&state.db, meeting.id, new_status, StatusUpdateOptions::default()).await.unwrap_or(false);
        if !ok {
            return Json(json!({"status": "error", "detail": "Failed to update meeting status"})).into_response();
        }
        success = Some(ok);
    }

    let fresh = refetch(&state, meeting.id).await.unwrap_or(meeting);

    if success == Some(false) {
        return Json(json!({"status": "error", "detail": format!("Invalid transition: {old_status} -> {}", new_status.as_str()), "meeting_id": fresh.id, "meeting_status": fresh.status})).into_response();
    }

    if success == Some(true) || (new_status == MeetingStatus::Active && fresh.status == "active") {
        let extra = if new_status == MeetingStatus::NeedsHumanHelp {
            fresh.data.get("escalation").map(|e| json!({"escalation_reason": e.get("reason"), "vnc_url": e.get("vnc_url"), "escalated_at": e.get("escalated_at")}))
        } else {
            None
        };
        publish_meeting_status_change(&state, fresh.id, new_status.as_str(), &fresh.platform, fresh.platform_specific_id.as_deref().unwrap_or(""), fresh.user_id, extra).await;
    }

    if success == Some(true) {
        schedule_status_webhook_task(fresh.id, &old_status, new_status.as_str()).await;
    }

    Json(json!({"status": "processed", "meeting_id": fresh.id, "meeting_status": fresh.status})).into_response()
}
