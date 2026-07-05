//! Storage client abstraction for recording media files. Faithful port of storage.py —
//! MinIO/S3 (boto3 -> aws-sdk-s3) and local-filesystem backends behind one trait.

use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait StorageClient: Send + Sync {
    async fn upload_file(&self, path: &str, data: Vec<u8>, content_type: &str) -> anyhow::Result<String>;
    async fn download_file(&self, path: &str) -> anyhow::Result<Vec<u8>>;
    async fn get_presigned_url(&self, path: &str, expires_secs: u64) -> anyhow::Result<String>;
    async fn delete_file(&self, path: &str) -> anyhow::Result<()>;
    async fn file_exists(&self, path: &str) -> anyhow::Result<bool>;
    /// Sorted ascending — callers rely on zero-padded chunk_seq lexicographic ordering.
    async fn list_objects_bounded(&self, prefix: &str, max_keys: usize) -> anyhow::Result<Vec<String>>;

    /// Stream-download to a local path. Default falls back to download_file + write (bytes
    /// pass through memory); MinIO overrides with a true streamed copy for bounded memory
    /// regardless of file size — used by recording_finalizer for chunk assembly.
    async fn download_file_to_path(&self, key: &str, dest_path: &Path) -> anyhow::Result<()> {
        let data = self.download_file(key).await?;
        tokio::fs::write(dest_path, data).await?;
        Ok(())
    }

    /// Stream-upload from a local path. Default falls back to read + upload_file; MinIO
    /// overrides with ByteStream::from_path so large masters don't round-trip through memory.
    async fn upload_file_path(&self, key: &str, src_path: &Path, content_type: &str) -> anyhow::Result<String> {
        let data = tokio::fs::read(src_path).await?;
        self.upload_file(key, data, content_type).await
    }
}

pub struct MinioStorageClient {
    client: aws_sdk_s3::Client,
    signing_client: aws_sdk_s3::Client,
    bucket: String,
}

impl MinioStorageClient {
    pub async fn new(endpoint: &str, public_endpoint: &str, access_key: &str, secret_key: &str, bucket: &str, secure: bool) -> Self {
        let protocol = if secure { "https" } else { "http" };
        let endpoint_url = if endpoint.contains("://") { endpoint.to_string() } else { format!("{protocol}://{endpoint}") };
        let public_endpoint_url = {
            let raw = if public_endpoint.is_empty() { endpoint } else { public_endpoint };
            if raw.contains("://") { raw.to_string() } else { format!("{protocol}://{raw}") }
        };

        let creds = aws_sdk_s3::config::Credentials::new(access_key, secret_key, None, None, "static");
        let build = |url: &str| {
            aws_sdk_s3::Client::from_conf(
                aws_sdk_s3::Config::builder()
                    .endpoint_url(url)
                    .credentials_provider(creds.clone())
                    .region(aws_sdk_s3::config::Region::new("us-east-1"))
                    .force_path_style(true)
                    .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
                    .build(),
            )
        };
        let client = build(&endpoint_url);
        let signing_client = if public_endpoint_url == endpoint_url { build(&endpoint_url) } else { build(&public_endpoint_url) };

        tracing::info!(endpoint = %endpoint_url, public_endpoint = %public_endpoint_url, bucket, "MinIO storage client initialized");
        Self { client, signing_client, bucket: bucket.to_string() }
    }
}

#[async_trait]
impl StorageClient for MinioStorageClient {
    async fn upload_file(&self, path: &str, data: Vec<u8>, content_type: &str) -> anyhow::Result<String> {
        let len = data.len();
        self.client.put_object().bucket(&self.bucket).key(path).body(data.into()).content_type(content_type).send().await?;
        tracing::info!(bytes = len, bucket = %self.bucket, path, "uploaded");
        Ok(path.to_string())
    }

    async fn download_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self.client.get_object().bucket(&self.bucket).key(path).send().await?;
        let bytes = resp.body.collect().await?.into_bytes().to_vec();
        Ok(bytes)
    }

    async fn get_presigned_url(&self, path: &str, expires_secs: u64) -> anyhow::Result<String> {
        let presign_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(std::time::Duration::from_secs(expires_secs))?;
        let presigned = self.signing_client.get_object().bucket(&self.bucket).key(path).presigned(presign_config).await?;
        Ok(presigned.uri().to_string())
    }

    async fn delete_file(&self, path: &str) -> anyhow::Result<()> {
        self.client.delete_object().bucket(&self.bucket).key(path).send().await?;
        Ok(())
    }

    async fn file_exists(&self, path: &str) -> anyhow::Result<bool> {
        match self.client.head_object().bucket(&self.bucket).key(path).send().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn list_objects_bounded(&self, prefix: &str, max_keys: usize) -> anyhow::Result<Vec<String>> {
        let mut keys = vec![];
        let mut continuation: Option<String> = None;
        loop {
            let mut req = self.client.list_objects_v2().bucket(&self.bucket).prefix(prefix).max_keys((max_keys.min(1000)) as i32);
            if let Some(token) = &continuation {
                req = req.continuation_token(token);
            }
            let resp = req.send().await?;
            for obj in resp.contents() {
                if keys.len() >= max_keys {
                    tracing::warn!(max_keys, prefix, "storage.list_objects_bounded truncated");
                    keys.sort();
                    return Ok(keys);
                }
                if let Some(k) = obj.key() {
                    keys.push(k.to_string());
                }
            }
            match resp.next_continuation_token() {
                Some(t) if resp.is_truncated().unwrap_or(false) => continuation = Some(t.to_string()),
                _ => break,
            }
        }
        keys.sort();
        Ok(keys)
    }

    async fn download_file_to_path(&self, key: &str, dest_path: &Path) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let resp = self.client.get_object().bucket(&self.bucket).key(key).send().await?;
        let mut body = resp.body;
        let mut file = tokio::fs::File::create(dest_path).await?;
        while let Some(chunk) = body.try_next().await? {
            file.write_all(&chunk).await?;
        }
        Ok(())
    }

    async fn upload_file_path(&self, key: &str, src_path: &Path, content_type: &str) -> anyhow::Result<String> {
        let body = aws_sdk_s3::primitives::ByteStream::from_path(src_path).await?;
        self.client.put_object().bucket(&self.bucket).key(key).body(body).content_type(content_type).send().await?;
        let size = tokio::fs::metadata(src_path).await.map(|m| m.len()).unwrap_or(0);
        tracing::info!(bytes = size, bucket = %self.bucket, key, "uploaded (streamed from path)");
        Ok(key.to_string())
    }
}

pub struct LocalStorageClient {
    base_dir: PathBuf,
}

impl LocalStorageClient {
    pub fn new(base_dir: &str) -> anyhow::Result<Self> {
        std::fs::create_dir_all(base_dir)?;
        Ok(Self { base_dir: PathBuf::from(base_dir) })
    }

    fn normalize(path: &str) -> anyhow::Result<PathBuf> {
        let cleaned = path.replace('\\', "/");
        let normalized = Path::new(cleaned.trim_start_matches('/'));
        if normalized.components().any(|c| matches!(c, std::path::Component::ParentDir)) || normalized.as_os_str().is_empty() {
            anyhow::bail!("Invalid storage path: {path}");
        }
        Ok(normalized.to_path_buf())
    }

    fn full_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        Ok(self.base_dir.join(Self::normalize(path)?))
    }
}

#[async_trait]
impl StorageClient for LocalStorageClient {
    async fn upload_file(&self, path: &str, data: Vec<u8>, _content_type: &str) -> anyhow::Result<String> {
        let full = self.full_path(path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, &data).await?;
        Ok(Self::normalize(path)?.to_string_lossy().to_string())
    }

    async fn download_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        Ok(tokio::fs::read(self.full_path(path)?).await?)
    }

    async fn get_presigned_url(&self, path: &str, _expires_secs: u64) -> anyhow::Result<String> {
        Ok(format!("file://{}", self.full_path(path)?.display()))
    }

    async fn delete_file(&self, path: &str) -> anyhow::Result<()> {
        let full = self.full_path(path)?;
        if full.exists() {
            tokio::fs::remove_file(full).await?;
        }
        Ok(())
    }

    async fn file_exists(&self, path: &str) -> anyhow::Result<bool> {
        Ok(self.full_path(path)?.exists())
    }

    async fn list_objects_bounded(&self, prefix: &str, max_keys: usize) -> anyhow::Result<Vec<String>> {
        let full_prefix = if prefix.is_empty() { self.base_dir.clone() } else { self.base_dir.join(Self::normalize(prefix)?) };
        let mut keys = vec![];
        if !full_prefix.is_dir() {
            return Ok(keys);
        }
        for entry in walkdir(&full_prefix) {
            if keys.len() >= max_keys {
                tracing::warn!(max_keys, prefix, "storage.list_objects_bounded truncated");
                break;
            }
            if let Ok(rel) = entry.strip_prefix(&self.base_dir) {
                keys.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        keys.sort();
        Ok(keys)
    }
}

fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walkdir(&path));
        } else {
            out.push(path);
        }
    }
    out
}

pub async fn create_storage_client(config: &crate::config::Config) -> anyhow::Result<Box<dyn StorageClient>> {
    match config.storage_backend.as_str() {
        "minio" | "s3" => Ok(Box::new(
            MinioStorageClient::new(&config.minio_endpoint, &config.minio_public_endpoint, &config.minio_access_key, &config.minio_secret_key, &config.minio_bucket, config.minio_secure).await,
        )),
        "local" => Ok(Box::new(LocalStorageClient::new(&config.local_storage_dir)?)),
        other => anyhow::bail!("Unknown storage backend: {other}. Supported: minio, s3, local"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_path_traversal() {
        assert!(LocalStorageClient::normalize("../etc/passwd").is_err());
        assert!(LocalStorageClient::normalize("a/../../b").is_err());
    }

    #[test]
    fn normalize_accepts_ordinary_relative_path() {
        assert!(LocalStorageClient::normalize("recordings/1/2/audio/chunk-000001.webm").is_ok());
    }

    #[tokio::test]
    async fn local_storage_round_trip() {
        let dir = std::env::temp_dir().join(format!("kioku-storage-test-{}", uuid::Uuid::new_v4()));
        let client = LocalStorageClient::new(dir.to_str().unwrap()).unwrap();
        client.upload_file("a/b/c.txt", b"hello".to_vec(), "text/plain").await.unwrap();
        assert!(client.file_exists("a/b/c.txt").await.unwrap());
        assert_eq!(client.download_file("a/b/c.txt").await.unwrap(), b"hello");
        let listed = client.list_objects_bounded("a", 10).await.unwrap();
        assert_eq!(listed, vec!["a/b/c.txt".to_string()]);
        client.delete_file("a/b/c.txt").await.unwrap();
        assert!(!client.file_exists("a/b/c.txt").await.unwrap());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
