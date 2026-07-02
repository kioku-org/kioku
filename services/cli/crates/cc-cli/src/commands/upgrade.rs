use crate::config::{REPO, VERSION};
use anyhow::Result;

pub async fn run() -> Result<()> {
    let info = cc_upgrade::check_for_update(REPO, VERSION).await?;
    if info.latest_version == VERSION {
        println!("Already up to date (v{VERSION}).");
    } else {
        println!("Upgrading v{VERSION} → v{}…", info.latest_version);
        let msg = cc_upgrade::perform_upgrade(REPO, VERSION).await?;
        println!("{msg}");
    }
    Ok(())
}
