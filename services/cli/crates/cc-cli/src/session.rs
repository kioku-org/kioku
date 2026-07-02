use anyhow::Result;
use cc_auth::AuthFile;
use cc_kioku::KiokuClient;

pub fn require_auth() -> Result<AuthFile> {
    AuthFile::load()?.ok_or_else(|| anyhow::anyhow!("not signed in — run `kioku signin`"))
}

pub fn make_client(auth: &AuthFile) -> KiokuClient {
    KiokuClient::with_token(&auth.server_url, &auth.token)
}