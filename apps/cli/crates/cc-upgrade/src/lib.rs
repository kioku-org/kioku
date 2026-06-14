use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub struct UpgradeInfo {
    pub current_version: String,
    pub latest_version: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

fn target_triple() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown"
    };
    format!("{}-{}", arch, os)
}

pub async fn check_for_update(repo: &str, current_version: &str) -> Result<UpgradeInfo> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "companion-cli")
        .send()
        .await
        .context("checking for updates")?;

    if !resp.status().is_success() {
        anyhow::bail!("failed to check for updates: HTTP {}", resp.status());
    }

    let release: GithubRelease = resp
        .json()
        .await
        .context("parsing release info")?;

    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    let target = target_triple();

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.contains(&target))
        .or_else(|| release.assets.first());

    Ok(UpgradeInfo {
        current_version: current_version.to_string(),
        latest_version: latest_version.clone(),
        download_url: asset.map(|a| a.browser_download_url.clone()),
        asset_name: asset.map(|a| a.name.clone()),
        asset_size: asset.map(|a| a.size),
    })
}

pub async fn perform_upgrade(repo: &str, current_version: &str) -> Result<String> {
    let info = check_for_update(repo, current_version).await?;

    if info.latest_version == current_version {
        return Ok(format!("Already up to date (v{})", current_version));
    }

    let download_url = info
        .download_url
        .ok_or_else(|| anyhow::anyhow!("no download URL found for your platform"))?;

    println!(
        "Upgrading companion v{} → v{}",
        current_version, info.latest_version
    );
    println!("Downloading {}...", info.asset_name.unwrap_or_default());

    let client = reqwest::Client::new();
    let resp = client
        .get(&download_url)
        .header("User-Agent", "companion-cli")
        .send()
        .await
        .context("downloading update")?;

    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await.context("reading download")?;
    let current_exe = std::env::current_exe().context("finding current executable")?;
    let backup_path = current_exe.with_extension("old");

    std::fs::rename(&current_exe, &backup_path)
        .with_context(|| "renaming current binary")?;
    std::fs::write(&current_exe, &bytes)
        .with_context(|| "writing new binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))
            .context("setting executable permissions")?;
    }

    let _ = std::fs::remove_file(&backup_path);

    Ok(format!(
        "Upgraded to v{} ({} → {})",
        info.latest_version,
        current_version,
        info.latest_version
    ))
}