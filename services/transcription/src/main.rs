//! Vexa-compatible transcription service on the kiku engine.
//!
//! Rust rewrite of main.py's HTTP surface: OpenAI-Whisper-shaped
//! `POST /v1/audio/transcriptions` (multipart WAV in, verbose_json out) backed
//! by kiku — cloud STT via OpenRouter by default (chirp-3 / gpt-4o-*), local
//! whisper.cpp behind the `local-whisper` feature. Response shaping mirrors
//! chirp.py: RMS gain for quiet Meet audio, sentence-split segments with
//! pro-rated timestamps (the confirm loop needs per-segment ends), and a
//! whisper-valid language label (meeting-api rejects "unknown").

use std::io::Cursor;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use kiku::config::Config;
use kiku::engine::Engine;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tracing::{info, warn};

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.into())
}

struct App {
    engine: Engine,
    cloud: bool,
    semaphore: Semaphore,
    reserved_slots: usize,
    retry_after: String,
    api_token: String,
    worker_id: String,
    backend: String,
    model: String,
    target_rms: f32,
    language_label: String,
    timeout_s: f64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .init();

    let backend = env_or("STT_BACKEND", "chirp").to_lowercase();
    let model = match backend.as_str() {
        "chirp" => env_or("CHIRP_MODEL", "google/chirp-3"),
        "gpt4o" => env_or("GPT4O_MODEL", "openai/gpt-4o-mini-transcribe"),
        // local whisper.cpp: ggml model name or path (needs --features local-whisper)
        _ => env_or("MODEL_SIZE", "large-v3-turbo"),
    };
    let cloud = matches!(backend.as_str(), "chirp" | "gpt4o");
    let cfg = Config {
        use_local: !cloud,
        openrouter_url: env_or("OPENROUTER_URL", "https://openrouter.ai/api/v1"),
        openrouter_api_key: std::env::var("OPENROUTER_API_KEY").ok().filter(|v| !v.trim().is_empty()),
        model: model.clone(),
    };
    let worker_id = env_or("WORKER_ID", "1");
    let engine = Engine::from_config(&cfg)?;
    info!("Worker {worker_id} ready - {backend} backend ({model})");

    let api_token = env_or("API_TOKEN", "");
    if api_token.is_empty() {
        warn!("API_TOKEN not configured - allowing all requests");
    }

    let max_active: usize = env_or("MAX_ACTIVE_REQUESTS", &env_or("MAX_CONCURRENT_TRANSCRIPTIONS", "20")).parse()?;
    let app = Arc::new(App {
        engine,
        cloud,
        semaphore: Semaphore::new(max_active),
        reserved_slots: env_or("REALTIME_RESERVED_SLOTS", "1").parse()?,
        retry_after: env_or("BUSY_RETRY_AFTER_S", "1"),
        api_token,
        worker_id,
        backend,
        model,
        target_rms: env_or("CHIRP_TARGET_RMS", "0.1").parse()?,
        language_label: env_or("CHIRP_LANGUAGE_LABEL", "en"),
        timeout_s: env_or("OPENROUTER_TIMEOUT_S", "60").parse()?,
    });

    let router = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/v1/audio/transcriptions", post(transcribe))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
    axum::serve(listener, router).await?;
    Ok(())
}

fn service_info(app: &App) -> Value {
    json!({
        "service": "Vexa Transcription Service",
        "worker_id": app.worker_id,
        "backend": app.backend,
        "model": app.model,
        "status": "ready",
        "endpoints": {"transcribe": "/v1/audio/transcriptions", "health": "/health"},
    })
}

async fn root(State(app): State<Arc<App>>) -> Json<Value> {
    Json(service_info(&app))
}

async fn health(State(app): State<Arc<App>>) -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "worker_id": app.worker_id,
        "backend": app.backend,
        "model": app.model,
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
    }))
}

fn err(status: StatusCode, detail: impl Into<String>) -> Response {
    (status, Json(json!({"detail": detail.into()}))).into_response()
}

fn busy(app: &App, detail: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", app.retry_after.clone())],
        Json(json!({"detail": detail})),
    )
        .into_response()
}

fn authorized(app: &App, headers: &HeaderMap) -> bool {
    if app.api_token.is_empty() {
        return true;
    }
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
    header("X-API-Key") == app.api_token
        || header("Authorization").strip_prefix("Bearer ").map(str::trim) == Some(app.api_token.as_str())
}

async fn transcribe(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if !authorized(&app, &headers) {
        return err(StatusCode::UNAUTHORIZED, "Invalid or missing API token");
    }

    let mut file: Option<Vec<u8>> = None;
    let mut model = String::new();
    let mut language: Option<String> = None;
    let mut tier = headers
        .get("X-Transcription-Tier")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("realtime")
        .to_string();
    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let name = field.name().unwrap_or("").to_string();
                match name.as_str() {
                    "file" => match field.bytes().await {
                        Ok(b) => file = Some(b.to_vec()),
                        Err(e) => return err(StatusCode::BAD_REQUEST, format!("failed to read file: {e}")),
                    },
                    "model" => model = field.text().await.unwrap_or_default(),
                    "language" => {
                        language = field.text().await.ok().filter(|l| !l.trim().is_empty());
                    }
                    "transcription_tier" => {
                        if let Ok(t) = field.text().await {
                            tier = t;
                        }
                    }
                    // temperature / prompt / response_format / VAD knobs are
                    // local-whisper tuning; kiku's engine takes none of them.
                    _ => {}
                }
            }
            Ok(None) => break,
            Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid multipart body: {e}")),
        }
    }
    if model.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "Model parameter is required");
    }
    let Some(file) = file else {
        return err(StatusCode::BAD_REQUEST, "file field is required");
    };

    // Fail-fast admission (FAIL_FAST_WHEN_BUSY in main.py): never queue, let
    // the bot keep buffering and resend a bigger window later.
    if tier.trim().eq_ignore_ascii_case("deferred")
        && app.semaphore.available_permits() <= app.reserved_slots
    {
        return busy(&app, "Deferred tier is out of capacity. Please retry later.");
    }
    let Ok(_permit) = app.semaphore.try_acquire() else {
        return busy(&app, "Service busy. Please retry later.");
    };

    let start = std::time::Instant::now();
    info!("Worker {} received request - tier={tier}, {} bytes", app.worker_id, file.len());

    let (wav, duration) = match preprocess_wav(&file, app.target_rms) {
        Ok(x) => x,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("Failed to decode audio file: {e}")),
    };

    // Timeout guards the permit: OpenRouter hangs indefinitely on some
    // model/payload combos (gpt-4o JSON path, probed 2026-07-17).
    let fut = tokio::time::timeout(
        std::time::Duration::from_secs_f64(app.timeout_s),
        app.engine.transcribe(&wav, "wav", language.as_deref()),
    );
    // Local whisper inference is CPU/GPU-bound inside the future's poll:
    // block_in_place keeps it off the async workers. (The timeout can't
    // preempt a blocking poll — it only bites on the cloud path.)
    #[cfg(feature = "local-whisper")]
    let result = tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut));
    #[cfg(not(feature = "local-whisper"))]
    let result = fut.await;
    let transcript = match result {
        Err(_) => {
            warn!("Worker {} transcription timed out after {}s", app.worker_id, app.timeout_s);
            return err(StatusCode::GATEWAY_TIMEOUT, "transcription timed out");
        }
        Ok(Ok(t)) => t,
        Ok(Err(e)) => {
            warn!("Worker {} transcription failed: {e:#}", app.worker_id);
            return err(StatusCode::BAD_GATEWAY, format!("transcription failed: {e:#}"));
        }
    };

    let text = transcript.text.trim().to_string();
    // Label must be a whisper-valid code; cloud models don't echo detection
    // and meeting-api's validation silently drops segments on e.g. "unknown".
    let language_label = language.unwrap_or_else(|| {
        if app.cloud {
            app.language_label.clone()
        } else {
            transcript.language.clone()
        }
    });
    let segments: Vec<Value> = if app.cloud {
        // Cloud providers return no timestamps — synthesize confirmable segments.
        split_segments(&text, duration)
    } else {
        // Local whisper: real per-segment timestamps + word timing pass through.
        transcript.segments.iter().enumerate()
            .map(|(i, s)| {
                let mut v = segment_json(i, s.start, s.end, s.text.trim());
                if !s.words.is_empty() {
                    v["words"] = s.words.iter()
                        .map(|w| json!({
                            "word": w.word, "start": w.start,
                            "end": w.end, "probability": w.probability,
                        }))
                        .collect::<Vec<_>>()
                        .into();
                }
                v
            })
            .collect()
    };

    info!(
        "Worker {} completed in {:.2}s - duration: {duration:.2}s, chars: {}",
        app.worker_id, start.elapsed().as_secs_f64(), text.len(),
    );
    Json(json!({
        "text": text,
        "language": language_label,
        "language_probability": 0.0,
        "duration": duration,
        "segments": segments,
    }))
    .into_response()
}

/// Decode a PCM WAV, downmix to mono, gain quiet audio up to `target_rms`
/// (chirp's front-end discards quiet Meet capture, see chirp.py), and
/// re-encode as 16-bit mono WAV. Returns (wav bytes, duration seconds).
fn preprocess_wav(bytes: &[u8], target_rms: f32) -> anyhow::Result<(Vec<u8>, f64)> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
    let spec = reader.spec();
    // ponytail: PCM WAV only — the bot client always sends 16-bit WAV; the
    // old ffmpeg webm fallback goes here if another caller ever needs it.
    let raw: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader.samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0)).collect::<Result<_, _>>()?,
        (hound::SampleFormat::Int, 24) => reader.samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 8_388_608.0)).collect::<Result<_, _>>()?,
        (hound::SampleFormat::Int, 32) => reader.samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 2_147_483_648.0)).collect::<Result<_, _>>()?,
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>()?,
        (fmt, bits) => anyhow::bail!("unsupported wav encoding: {fmt:?} {bits}-bit"),
    };
    let mut mono: Vec<f32> = if spec.channels > 1 {
        raw.chunks_exact(spec.channels as usize)
            .map(|f| f.iter().sum::<f32>() / f.len() as f32)
            .collect()
    } else {
        raw
    };
    let duration = mono.len() as f64 / spec.sample_rate as f64;

    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
    if rms > 0.001 && rms < target_rms {
        let gain = target_rms / rms;
        for s in &mut mono {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }

    let mut out = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut out, hound::WavSpec {
        channels: 1,
        sample_rate: spec.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    })?;
    for s in &mono {
        writer.write_sample((s * 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok((out.into_inner(), duration))
}

fn segment_json(id: usize, start: f64, end: f64, text: &str) -> Value {
    json!({
        "id": id, "seek": 0,
        "start": (start * 1000.0).round() / 1000.0,
        "end": (end * 1000.0).round() / 1000.0,
        "text": text, "tokens": [], "temperature": 0.0,
        "audio_start": (start * 1000.0).round() / 1000.0,
        "audio_end": (end * 1000.0).round() / 1000.0,
    })
}

/// Sentence-shaped segments with timestamps pro-rated by character share —
/// port of chirp.py's _segments. Cloud models return no timestamps, and
/// without per-segment ends LocalAgreement can't confirm text mid-turn.
fn split_segments(text: &str, duration: f64) -> Vec<Value> {
    if text.is_empty() {
        return vec![];
    }
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_terminator = false;
    for c in text.chars() {
        let is_term = matches!(c, '.' | '!' | '?' | '…');
        if in_terminator && !is_term {
            let p = current.trim().to_string();
            if !p.is_empty() {
                pieces.push(p);
            }
            current.clear();
        }
        in_terminator = is_term;
        current.push(c);
    }
    let p = current.trim().to_string();
    if !p.is_empty() {
        pieces.push(p);
    }
    // Cap unpunctuated runs at 12 words (~4s) so they stay confirmable.
    let split: Vec<String> = pieces.iter()
        .flat_map(|p| {
            let words: Vec<&str> = p.split_whitespace().collect();
            words.chunks(12).map(|c| c.join(" ")).collect::<Vec<_>>()
        })
        .collect();
    let total: usize = split.iter().map(|p| p.len()).sum::<usize>().max(1);
    let mut segs = Vec::with_capacity(split.len());
    let mut t = 0.0f64;
    for (i, p) in split.iter().enumerate() {
        let end = if i == split.len() - 1 {
            duration
        } else {
            t + duration * p.len() as f64 / total as f64
        };
        segs.push(segment_json(i, t, end, p));
        t = end;
    }
    segs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_matches_chirp_py() {
        let segs = split_segments("Hello there. How are you today?", 10.0);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0]["text"], "Hello there.");
        assert_eq!(segs[1]["text"], "How are you today?");
        assert_eq!(segs[0]["start"], 0.0);
        assert_eq!(segs[1]["end"], 10.0);

        // 26 unpunctuated words -> 12/12/2 chunks
        let long = (1..=26).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        assert_eq!(split_segments(&long, 8.0).len(), 3);

        assert!(split_segments("", 5.0).is_empty());
    }

    #[test]
    fn wav_roundtrip_gains_quiet_audio() {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Cursor::new(Vec::new());
        let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
        for i in 0..48_000 {
            // quiet stereo sine, rms ~0.007 (like Meet capture)
            let s = ((i as f32 / 48.0).sin() * 0.01 * 32767.0) as i16;
            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();

        let (wav, duration) = preprocess_wav(buf.get_ref(), 0.1).unwrap();
        assert!((duration - 1.0).abs() < 0.01);
        let mut r = hound::WavReader::new(Cursor::new(&wav[..])).unwrap();
        assert_eq!(r.spec().channels, 1);
        let out: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 0.05, "gain not applied: rms={rms}");
    }
}
