//! HuggingFace Hub REST API client for dataset upload.
//!
//! Uses the HF Hub commit API to create repos and push files
//! (including parquet) to a HuggingFace dataset repository.

use reqwest::Client;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

const HF_API_BASE: &str = "https://huggingface.co/api";

#[derive(Debug, Clone)]
pub struct HfUploadConfig {
    pub repo_id: String,
    pub token: String,
    pub private: bool,
}

#[derive(Debug, Serialize)]
struct CommitAction {
    action: String,
    path: String,
    content: String,
    encoding: String,
}

#[derive(Debug, Serialize)]
struct CommitRequest {
    summary: String,
    actions: Vec<CommitAction>,
}

pub struct HfUploader {
    client: Client,
    config: HfUploadConfig,
    uploaded_files: Arc<Mutex<Vec<String>>>,
}

impl HfUploader {
    pub fn new(config: HfUploadConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            client,
            config,
            uploaded_files: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn ensure_repo_exists(&self) -> anyhow::Result<()> {
        let url = format!("{}/repos/create", HF_API_BASE);

        let (organization, name) = if let Some((org, n)) = self.config.repo_id.split_once('/') {
            (Some(org.to_string()), n.to_string())
        } else {
            (None, self.config.repo_id.clone())
        };

        let mut body = serde_json::json!({
            "type": "dataset",
            "name": name,
            "private": self.config.private,
        });
        if let Some(org) = organization {
            body["organization"] = serde_json::Value::String(org);
        }

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.token)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() || status.as_u16() == 409 {
            tracing::info!(repo = %self.config.repo_id, "HF dataset repo ready");
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            if text.contains("already created") || text.contains("already exist") {
                tracing::info!(repo = %self.config.repo_id, "HF dataset repo already exists");
                Ok(())
            } else {
                anyhow::bail!("Failed to create HF repo ({}): {}", status, text);
            }
        }
    }

    /// Upload a file to the dataset repository via the commit API.
    /// `path_in_repo` is the path inside the repo (e.g. "data/train.parquet").
    /// `content` is the raw bytes of the file.
    pub async fn upload_file(
        &self,
        path_in_repo: &str,
        content: &[u8],
        commit_message: &str,
    ) -> anyhow::Result<()> {
        let url = format!(
            "{}/datasets/{}/commit/main",
            HF_API_BASE, self.config.repo_id
        );

        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content);

        let body = CommitRequest {
            summary: commit_message.to_string(),
            actions: vec![CommitAction {
                action: "file".to_string(),
                path: path_in_repo.to_string(),
                content: encoded,
                encoding: "base64".to_string(),
            }],
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.token)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            tracing::info!(
                path = path_in_repo,
                repo = %self.config.repo_id,
                "Uploaded file to HF"
            );
            self.uploaded_files
                .lock()
                .await
                .push(path_in_repo.to_string());
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HF upload failed ({}): {}", status, text);
        }
    }

    /// Upload a local file from disk.
    pub async fn upload_file_from_path(
        &self,
        local_path: &Path,
        path_in_repo: &str,
        commit_message: &str,
    ) -> anyhow::Result<()> {
        let content = std::fs::read(local_path)?;
        self.upload_file(path_in_repo, &content, commit_message)
            .await
    }

    /// Upload multiple files in a single commit (more efficient).
    pub async fn upload_files(
        &self,
        files: &[(&str, &[u8])], // (path_in_repo, content)
        commit_message: &str,
    ) -> anyhow::Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        let url = format!(
            "{}/datasets/{}/commit/main",
            HF_API_BASE, self.config.repo_id
        );

        let actions: Vec<CommitAction> = files
            .iter()
            .map(|(path, content)| {
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content);
                CommitAction {
                    action: "file".to_string(),
                    path: path.to_string(),
                    content: encoded,
                    encoding: "base64".to_string(),
                }
            })
            .collect();

        let body = CommitRequest {
            summary: commit_message.to_string(),
            actions,
        };

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.token)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            let mut uploaded = self.uploaded_files.lock().await;
            for (path, _) in files {
                uploaded.push(path.to_string());
            }
            tracing::info!(
                count = files.len(),
                repo = %self.config.repo_id,
                "Uploaded batch to HF"
            );
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HF batch upload failed ({}): {}", status, text);
        }
    }

    /// Upload the README.md dataset card.
    pub async fn upload_dataset_card(&self, card_content: &str) -> anyhow::Result<()> {
        self.upload_file("README.md", card_content.as_bytes(), "Update dataset card")
            .await
    }

    /// Upload an entire directory tree to the HF repo.
    /// `local_dir` is the directory on disk.
    /// `repo_prefix` is the prefix path inside the repo (e.g. "tasks/my-task-id").
    pub async fn upload_directory(
        &self,
        local_dir: &Path,
        repo_prefix: &str,
        commit_message: &str,
    ) -> anyhow::Result<usize> {
        let mut file_pairs: Vec<(String, Vec<u8>)> = Vec::new();

        fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> anyhow::Result<()> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let repo_path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", prefix, name)
                };
                if path.is_dir() {
                    walk(&path, &repo_path, out)?;
                } else if path.is_file() {
                    if let Ok(bytes) = std::fs::read(&path) {
                        out.push((repo_path, bytes));
                    }
                }
            }
            Ok(())
        }

        walk(local_dir, repo_prefix, &mut file_pairs)?;

        if file_pairs.is_empty() {
            return Ok(0);
        }

        let total = file_pairs.len();

        for chunk in file_pairs.chunks(20) {
            let refs: Vec<(&str, &[u8])> = chunk
                .iter()
                .map(|(p, b)| (p.as_str(), b.as_slice()))
                .collect();
            self.upload_files(&refs, commit_message).await?;
        }

        Ok(total)
    }

    pub fn repo_url(&self) -> String {
        format!("https://huggingface.co/datasets/{}", self.config.repo_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = HfUploadConfig {
            repo_id: "org/my-dataset".to_string(),
            token: "hf_test_token".to_string(),
            private: false,
        };
        assert_eq!(config.repo_id, "org/my-dataset");
    }

    #[test]
    fn test_uploader_creation() {
        let config = HfUploadConfig {
            repo_id: "org/my-dataset".to_string(),
            token: "hf_test".to_string(),
            private: false,
        };
        let uploader = HfUploader::new(config);
        assert_eq!(
            uploader.repo_url(),
            "https://huggingface.co/datasets/org/my-dataset"
        );
    }
}
