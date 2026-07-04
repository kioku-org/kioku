use std::io::Write;

/// Extract plain text from a .pptx file's raw bytes by shelling out to
/// `scripts/extract_pptx.py` (python-pptx) — mirrors the ffmpeg-subprocess
/// pattern already used by transcription-service. No mature native-Rust PPTX
/// parser exists; python-pptx is the de facto standard for this format.
pub fn extract_text_from_bytes(data: &[u8]) -> anyhow::Result<String> {
    let mut tmp = tempfile::Builder::new().suffix(".pptx").tempfile()?;
    tmp.write_all(data)?;
    tmp.flush()?;

    let script = script_path();
    let output = std::process::Command::new("python3")
        .arg(&script)
        .arg(tmp.path())
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to spawn extract_pptx.py: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("PPTX extraction failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn script_path() -> std::path::PathBuf {
    std::env::var("PPTX_EXTRACT_SCRIPT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("scripts/extract_pptx.py"))
}
