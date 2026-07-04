use std::io::Write;

/// Below this many characters, a PDF's extracted text layer is treated as too thin to be
/// real content (i.e. the document is scanned/image-based) and OCR is run instead.
/// ponytail: fixed global threshold, not per-page — good enough for "scanned PDF" detection;
/// a mixed text+embedded-diagram PDF's images are not OCR'd. Upgrade path: per-page hybrid
/// (only OCR pages whose own text yield is thin) if that gap matters in practice.
const OCR_FALLBACK_THRESHOLD_CHARS: usize = 200;

pub fn should_ocr(extracted_text: &str) -> bool {
    extracted_text.trim().len() < OCR_FALLBACK_THRESHOLD_CHARS
}

/// Rasterize every page of a PDF to a PNG (via `pdftoppm`, poppler-utils) and OCR each one
/// (via the `tesseract` CLI), concatenating the results. Both are subprocess calls, matching
/// the pattern transcription-service already uses for its `ffmpeg` fallback — no native
/// Rust PDF-rendering or OCR dependency needed.
pub fn ocr_pdf_bytes(data: &[u8]) -> anyhow::Result<String> {
    let dir = tempfile::tempdir()?;
    let pdf_path = dir.path().join("input.pdf");
    std::fs::File::create(&pdf_path)?.write_all(data)?;

    let page_prefix = dir.path().join("page");
    let render = std::process::Command::new("pdftoppm")
        .args(["-png", "-r", "200"])
        .arg(&pdf_path)
        .arg(&page_prefix)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to spawn pdftoppm: {}", e))?;
    if !render.status.success() {
        anyhow::bail!(
            "pdftoppm failed: {}",
            String::from_utf8_lossy(&render.stderr).trim()
        );
    }

    let mut pages: Vec<std::path::PathBuf> = std::fs::read_dir(dir.path())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .collect();
    pages.sort();

    let mut text = String::new();
    for page in pages {
        match ocr_image_file(&page) {
            Ok(page_text) => {
                text.push_str(&page_text);
                text.push('\n');
            }
            Err(e) => tracing::warn!("OCR failed for {}: {}", page.display(), e),
        }
    }
    Ok(text)
}

fn ocr_image_file(path: &std::path::Path) -> anyhow::Result<String> {
    let output = std::process::Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to spawn tesseract: {}", e))?;
    if !output.status.success() {
        anyhow::bail!(
            "tesseract failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_triggers_ocr_fallback() {
        assert!(should_ocr(""));
        assert!(should_ocr("a few words only"));
    }

    #[test]
    fn substantial_text_skips_ocr() {
        let text = "This is a real sentence with real content. ".repeat(20);
        assert!(!should_ocr(&text));
    }
}
