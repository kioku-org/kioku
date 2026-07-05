//! Server-side master-recording finalizer. Faithful port of recording_finalizer.py — builds a
//! single playable master.webm/wav from the per-chunk objects already in storage.
//!
//! No-fallback contract (preserved from the Python original): 0 chunks found -> log + return,
//! never fabricate an empty master. Concat/upload failure -> propagate the error so the exit
//! callback that invoked this can retry (matches the original's "no try/except that swallows").
//! Idempotent: if `<prefix>/master.<format>` already exists, skip.

use crate::storage::StorageClient;
use serde_json::{json, Value};
use std::path::PathBuf;

const WEBM_MAGIC: &[u8] = &[0x1A, 0x45, 0xDF, 0xA3];
const WAV_HEADER_BYTES: usize = 44;

fn detect_format(head: &[u8], declared_format: &str) -> anyhow::Result<&'static str> {
    let actual = if head.starts_with(WEBM_MAGIC) {
        "webm"
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WAVE" {
        "wav"
    } else {
        anyhow::bail!("Unrecognized chunk format: declared={declared_format:?} head={head:?} (expected EBML 1A45DFA3 or RIFF...WAVE)");
    };
    if !declared_format.is_empty() && declared_format.to_lowercase() != actual {
        anyhow::bail!("Chunk format mismatch: file extension claims {declared_format:?} but bytes look like {actual:?}");
    }
    Ok(actual)
}

/// Returns (fmt_chunk_16_bytes, declared_data_size). Rejects any non-canonical WAV layout the
/// bot's PulseAudioCapture writer doesn't produce — fail loudly rather than silently mis-concat.
fn parse_wav_header(buf: &[u8]) -> anyhow::Result<([u8; 16], u32)> {
    if buf.len() < WAV_HEADER_BYTES {
        anyhow::bail!("WAV chunk shorter than 44-byte header: {} bytes", buf.len());
    }
    if &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        anyhow::bail!("WAV chunk missing RIFF/WAVE signature: head={:?}", &buf[0..12]);
    }
    if &buf[12..16] != b"fmt " {
        anyhow::bail!("WAV chunk missing fmt chunk: head[12:16]={:?}", &buf[12..16]);
    }
    if &buf[36..40] != b"data" {
        anyhow::bail!("WAV chunk has non-canonical layout: data tag expected at offset 36, found {:?}", &buf[36..40]);
    }
    let mut fmt_chunk = [0u8; 16];
    fmt_chunk.copy_from_slice(&buf[20..36]);
    let declared_data_size = u32::from_le_bytes(buf[40..44].try_into().unwrap());
    Ok((fmt_chunk, declared_data_size))
}

/// RIFF-aware merge: strip each chunk's 44-byte header, sum the data payloads, prepend one
/// corrected master header. The fmt chunk is copied verbatim from the first chunk; a mismatch
/// in any later chunk is an error (mixed sample rates/channels can't be validly concatenated).
fn build_wav_master(chunks: &[Vec<u8>]) -> anyhow::Result<Vec<u8>> {
    if chunks.is_empty() {
        anyhow::bail!("build_wav_master requires at least one chunk");
    }
    let (fmt_chunk, _) = parse_wav_header(&chunks[0])?;
    let mut payloads: Vec<&[u8]> = Vec::with_capacity(chunks.len());
    for (i, c) in chunks.iter().enumerate() {
        let (c_fmt, c_declared) = parse_wav_header(c)?;
        if c_fmt != fmt_chunk {
            anyhow::bail!("WAV fmt chunk mismatch at chunk index {i}: first={fmt_chunk:?} this={c_fmt:?}");
        }
        let payload = &c[WAV_HEADER_BYTES..];
        if c_declared as usize != payload.len() {
            tracing::warn!(chunk = i, declared = c_declared, actual = payload.len(), "WAV chunk declared size mismatch — using actual body length");
        }
        payloads.push(payload);
    }

    let total_data: usize = payloads.iter().map(|p| p.len()).sum();
    let mut out = Vec::with_capacity(44 + total_data);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + total_data) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&fmt_chunk);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(total_data as u32).to_le_bytes());
    for p in payloads {
        out.extend_from_slice(p);
    }
    Ok(out)
}

/// Streamed byte-concat of WebM chunks to a local temp file. WHY byte-concat and not ffmpeg
/// concat: the bot's MediaRecorder chunk 0 is self-describing (EBML+Segment+first Cluster);
/// chunks 1..N are Cluster-only, not standalone WebM containers. Stacking Cluster elements
/// inside the Segment (plain concatenation) yields a valid container; ffmpeg's concat demuxer
/// would silently drop the Cluster-only inputs. Bounded memory: each chunk streams to disk via
/// StorageClient::download_file_to_path, then copies into the output file and is deleted.
async fn build_webm_master_streaming_file(storage: &dyn StorageClient, chunk_keys: &[String]) -> anyhow::Result<PathBuf> {
    if chunk_keys.is_empty() {
        anyhow::bail!("build_webm_master_streaming_file requires at least one chunk");
    }
    let out_path = std::env::temp_dir().join(format!("webm-master-{}.webm", uuid::Uuid::new_v4()));
    let chunk_dir = std::env::temp_dir().join(format!("webm-chunks-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&chunk_dir).await?;

    let result = async {
        let mut out = tokio::fs::File::create(&out_path).await?;
        use tokio::io::AsyncWriteExt;
        for (idx, key) in chunk_keys.iter().enumerate() {
            let chunk_path = chunk_dir.join(format!("{idx:06}.webm"));
            storage.download_file_to_path(key, &chunk_path).await?;
            let data = tokio::fs::read(&chunk_path).await?;
            out.write_all(&data).await?;
            tokio::fs::remove_file(&chunk_path).await.ok();
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&chunk_dir).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&out_path).await;
        return Err(e);
    }
    Ok(out_path)
}

/// Best-effort ffmpeg pass to inject a proper duration tag into the concatenated WebM (#302).
/// On ANY failure — ffmpeg missing, non-zero exit, timeout, empty output — falls back to the
/// unmodified byte-concat file. This is the explicitly-approved fallback: the recording still
/// plays without the duration tag, which is strictly better than failing the whole finalize.
async fn inject_webm_duration_file(src_path: &std::path::Path) -> PathBuf {
    let dst_path = std::env::temp_dir().join(format!("webm-duration-{}.webm", uuid::Uuid::new_v4()));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-fflags", "+genpts", "-i"])
            .arg(src_path)
            .args(["-c", "copy"])
            .arg(&dst_path)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => match tokio::fs::metadata(&dst_path).await {
            Ok(m) if m.len() > 0 => dst_path,
            _ => {
                tracing::warn!("[FINALIZER] webm.duration_inject.empty_output — falling back (#302)");
                let _ = tokio::fs::remove_file(&dst_path).await;
                src_path.to_path_buf()
            }
        },
        Ok(Ok(output)) => {
            tracing::warn!(rc = ?output.status.code(), stderr = %String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(300)]), "[FINALIZER] webm.duration_inject.failed — falling back (#302)");
            let _ = tokio::fs::remove_file(&dst_path).await;
            src_path.to_path_buf()
        }
        Ok(Err(_)) => {
            tracing::warn!("[FINALIZER] webm.duration_inject.ffmpeg_missing — falling back (#302)");
            let _ = tokio::fs::remove_file(&dst_path).await;
            src_path.to_path_buf()
        }
        Err(_) => {
            tracing::warn!("[FINALIZER] webm.duration_inject.timeout (>120s) — falling back (#302)");
            let _ = tokio::fs::remove_file(&dst_path).await;
            src_path.to_path_buf()
        }
    }
}

/// storage_path convention (recordings.rs): recordings/<user>/<rec>/<session>/<media_type>/<seq:06d>.<ext>
fn chunk_prefix(storage_path: &str) -> anyhow::Result<&str> {
    storage_path.rsplit_once('/').map(|(prefix, _)| prefix).ok_or_else(|| anyhow::anyhow!("Invalid storage_path (no separator): {storage_path:?}"))
}

fn master_path(prefix: &str, fmt: &str) -> String {
    format!("{prefix}/master.{fmt}")
}

fn is_master_key(key: &str) -> bool {
    key.rsplit('/').next().map(|tail| tail.starts_with("master.")).unwrap_or(false)
}

fn media_content_type(media_type: &str, media_format: &str) -> &'static str {
    let fmt = media_format.to_lowercase();
    let typ = media_type.to_lowercase();
    if fmt == "webm" {
        return if typ == "audio" { "audio/webm" } else { "video/webm" };
    }
    if fmt == "wav" {
        return "audio/wav";
    }
    "application/octet-stream"
}

/// Build and upload the master for one media_files entry. Returns Some(master_path) on success,
/// None if no chunks were found under the prefix (no-fallback: caller must not treat this as an
/// error, just leave storage_path alone).
async fn finalize_one_media_file(storage: &dyn StorageClient, media_file_id: &str, storage_path: &str, declared_format: &str, media_type: &str) -> anyhow::Result<Option<String>> {
    let prefix = chunk_prefix(storage_path)?;
    let fmt = declared_format.to_lowercase();
    if fmt != "webm" && fmt != "wav" {
        anyhow::bail!("Unsupported format for master finalization: {declared_format:?} (expected webm or wav)");
    }

    let master_key = master_path(prefix, &fmt);
    if storage.file_exists(&master_key).await? {
        tracing::info!(media_file_id, master_key, "[FINALIZER] master already exists, skipping");
        return Ok(Some(master_key));
    }

    let all_keys = storage.list_objects_bounded(&format!("{prefix}/"), 10_000).await?;
    let chunk_keys: Vec<String> = all_keys.into_iter().filter(|k| !is_master_key(k)).collect();
    if chunk_keys.is_empty() {
        tracing::warn!(media_file_id, prefix, "[FINALIZER] no chunks under prefix — skipping master build");
        return Ok(None);
    }

    tracing::info!(media_file_id, format = %fmt, chunks = chunk_keys.len(), prefix, "[FINALIZER] building master");

    let first_chunk_bytes = storage.download_file(&chunk_keys[0]).await?;
    let actual_fmt = detect_format(&first_chunk_bytes[..first_chunk_bytes.len().min(12)], &fmt)?;
    drop(first_chunk_bytes);

    if actual_fmt == "webm" {
        let concat_path = build_webm_master_streaming_file(storage, &chunk_keys).await?;
        let final_path = inject_webm_duration_file(&concat_path).await;
        let upload_result = storage.upload_file_path(&master_key, &final_path, media_content_type(media_type, "webm")).await;
        if final_path != concat_path {
            let _ = tokio::fs::remove_file(&final_path).await;
        }
        let _ = tokio::fs::remove_file(&concat_path).await;
        upload_result?;
        tracing::info!(media_file_id, master_key, chunks = chunk_keys.len(), "[FINALIZER] master uploaded (size streamed)");
    } else {
        let mut chunks = Vec::with_capacity(chunk_keys.len());
        for k in &chunk_keys {
            chunks.push(storage.download_file(k).await?);
        }
        let master_bytes = build_wav_master(&chunks)?;
        let size = master_bytes.len();
        storage.upload_file(&master_key, master_bytes, "audio/wav").await?;
        tracing::info!(media_file_id, master_key, size, chunks = chunk_keys.len(), "[FINALIZER] master uploaded");
    }

    Ok(Some(master_key))
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Build master.{webm|wav} from chunks in storage for every eligible media_files entry across
/// meeting.data.recordings. Idempotent — safe to call on every exit callback / sweep retry.
pub async fn finalize_recording_master(db: &sqlx::PgPool, config: &crate::config::Config, meeting_id: i32) -> anyhow::Result<()> {
    let storage = crate::storage::create_storage_client(config).await?;

    let data: Option<Value> = sqlx::query_scalar("SELECT data FROM meetings WHERE id = $1").bind(meeting_id).fetch_optional(db).await?;
    let Some(mut meeting_data) = data else {
        tracing::info!(meeting_id, "[FINALIZER] no Meeting row; nothing to finalize");
        return Ok(());
    };

    let mut rec_list: Vec<Value> = meeting_data.get("recordings").and_then(Value::as_array).cloned().unwrap_or_default();
    if rec_list.is_empty() {
        tracing::info!(meeting_id, "[FINALIZER] no recordings in meeting.data; nothing to finalize");
        return Ok(());
    }

    let mut finalized_any = false;

    for rec_payload in rec_list.iter_mut() {
        if rec_payload.get("status").and_then(Value::as_str) == Some("failed") {
            continue;
        }
        let Some(media_files) = rec_payload.get_mut("media_files").and_then(Value::as_array_mut) else { continue };
        if media_files.is_empty() {
            continue;
        }

        for mf in media_files.iter_mut() {
            let mf_type = mf.get("type").and_then(Value::as_str).unwrap_or("").to_string();
            let mf_format = mf.get("format").and_then(Value::as_str).unwrap_or("").to_lowercase();
            let mf_path = mf.get("storage_path").and_then(Value::as_str).unwrap_or("").to_string();
            let mf_id = mf.get("id").map(|v| v.to_string()).unwrap_or_default();

            if mf_type != "audio" && mf_type != "video" {
                continue;
            }
            if mf_path.is_empty() || mf_format.is_empty() {
                tracing::warn!(meeting_id, mf_id, "[FINALIZER] [DATA] missing path/format — skipping");
                continue;
            }
            if mf_format != "webm" && mf_format != "wav" {
                tracing::warn!(meeting_id, mf_id, format = %mf_format, "[FINALIZER] [DATA] unsupported format — skipping");
                continue;
            }

            let master_key = finalize_one_media_file(storage.as_ref(), &mf_id, &mf_path, &mf_format, &mf_type).await.inspect_err(|e| {
                tracing::error!(meeting_id, mf_id, error = %e, "[FINALIZER] [DATA] failed");
            })?;

            let Some(master_key) = master_key else { continue };
            if mf.get("storage_path").and_then(Value::as_str) == Some(master_key.as_str()) {
                continue; // idempotent re-run
            }

            mf["storage_path"] = json!(master_key);
            if mf.get("finalized_at").and_then(Value::as_str).is_none() {
                mf["finalized_at"] = json!(now_iso());
            }
            mf["finalized_by"] = json!("recording_finalizer.master");
            mf["is_final"] = json!(true);
            finalized_any = true;
            tracing::info!(meeting_id, mf_id, master_key, "[FINALIZER] [DATA] storage_path -> master");
        }

        let has_audio_master = media_files.iter().any(|mf| mf.get("type").and_then(Value::as_str) == Some("audio") && mf.get("finalized_by").and_then(Value::as_str) == Some("recording_finalizer.master"));
        let has_video_master = media_files.iter().any(|mf| mf.get("type").and_then(Value::as_str) == Some("video") && mf.get("finalized_by").and_then(Value::as_str) == Some("recording_finalizer.master"));
        if let Some(recording_id) = rec_payload.get("id").cloned() {
            if has_audio_master || has_video_master {
                rec_payload["playback_url"] = json!({
                    "audio": if has_audio_master { Some(format!("/recordings/{recording_id}/master?type=audio")) } else { None },
                    "video": if has_video_master { Some(format!("/recordings/{recording_id}/master?type=video")) } else { None },
                });
                finalized_any = true;
            }
        }
    }

    if finalized_any {
        meeting_data["recordings"] = json!(rec_list);
        sqlx::query("UPDATE meetings SET data = $1 WHERE id = $2").bind(&meeting_data).bind(meeting_id).execute(db).await?;
        tracing::info!(meeting_id, "[FINALIZER] committed master storage_path update(s) to meeting.data");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_chunk(payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&((36 + payload.len()) as u32).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x3E, 0, 0, 0, 0x7D, 0, 0, 2, 0, 16, 0]); // arbitrary but consistent fmt body
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn detect_format_recognizes_webm_magic() {
        assert_eq!(detect_format(WEBM_MAGIC, "webm").unwrap(), "webm");
    }

    #[test]
    fn detect_format_recognizes_wav_magic() {
        let chunk = wav_chunk(b"hello");
        assert_eq!(detect_format(&chunk[..12], "wav").unwrap(), "wav");
    }

    #[test]
    fn detect_format_rejects_mismatch() {
        let chunk = wav_chunk(b"hello");
        assert!(detect_format(&chunk[..12], "webm").is_err());
    }

    #[test]
    fn detect_format_rejects_garbage() {
        assert!(detect_format(b"not a media file", "webm").is_err());
    }

    #[test]
    fn build_wav_master_concatenates_payloads_and_fixes_header() {
        let c1 = wav_chunk(b"AAAA");
        let c2 = wav_chunk(b"BBBBBB");
        let master = build_wav_master(&[c1, c2]).unwrap();
        let (_, declared) = parse_wav_header(&master).unwrap();
        assert_eq!(declared, 10); // 4 + 6 bytes of payload
        assert_eq!(&master[44..], b"AAAABBBBBB");
    }

    #[test]
    fn build_wav_master_rejects_fmt_mismatch() {
        let c1 = wav_chunk(b"AAAA");
        let mut c2 = wav_chunk(b"BBBB");
        c2[22] = 2; // corrupt a byte inside the fmt chunk body
        assert!(build_wav_master(&[c1, c2]).is_err());
    }

    #[test]
    fn chunk_prefix_and_master_path() {
        let path = "recordings/1/2/session-abc/audio/000001.webm";
        let prefix = chunk_prefix(path).unwrap();
        assert_eq!(prefix, "recordings/1/2/session-abc/audio");
        assert_eq!(master_path(prefix, "webm"), "recordings/1/2/session-abc/audio/master.webm");
    }

    #[test]
    fn is_master_key_detects_master_files() {
        assert!(is_master_key("recordings/1/2/s/audio/master.webm"));
        assert!(!is_master_key("recordings/1/2/s/audio/000001.webm"));
    }

    #[test]
    fn media_content_type_mapping() {
        assert_eq!(media_content_type("audio", "webm"), "audio/webm");
        assert_eq!(media_content_type("video", "webm"), "video/webm");
        assert_eq!(media_content_type("audio", "wav"), "audio/wav");
    }
}
