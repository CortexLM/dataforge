//! Self-update functionality for the swe-forge binary.
//!
//! Checks GitHub Releases for the latest version and replaces the current
//! binary in-place when a newer version is available.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{info, warn};

const GITHUB_REPO: &str = "CortexLM/swe-forge";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Detect the platform-specific asset name suffix for the current binary.
fn platform_asset_suffix() -> Result<&'static str> {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    match (os, arch) {
        ("linux", "x86_64") => Ok("linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Ok("linux-aarch64.tar.gz"),
        _ => bail!("Unsupported platform: {os}-{arch}. Self-update is only available for Linux x86_64 and aarch64."),
    }
}

/// Get the path of the currently running binary.
fn current_exe_path() -> Result<PathBuf> {
    std::env::current_exe().context("Failed to determine current executable path")
}

/// Run the self-update process.
///
/// Checks GitHub for the latest release, compares versions, and replaces
/// the binary if a newer version is available.
pub async fn run_self_update(force: bool) -> Result<()> {
    info!("Checking for updates...");
    info!("Current version: {CURRENT_VERSION}");

    let suffix = platform_asset_suffix()?;

    let client = reqwest::Client::builder()
        .user_agent(format!("swe-forge/{CURRENT_VERSION}"))
        .build()
        .context("Failed to create HTTP client")?;

    let api_url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let response = client
        .get(&api_url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("Failed to check for updates. Check your internet connection.")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("GitHub API returned {status}: {body}");
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse GitHub release response")?;

    let latest_tag = release.tag_name.trim_start_matches('v');
    let latest_version =
        semver::Version::parse(latest_tag).context("Failed to parse latest version")?;
    let current_version =
        semver::Version::parse(CURRENT_VERSION).context("Failed to parse current version")?;

    if latest_version <= current_version && !force {
        info!("Already up to date (v{CURRENT_VERSION})");
        return Ok(());
    }

    if latest_version == current_version && force {
        warn!("Forcing re-install of current version v{CURRENT_VERSION}");
    } else {
        info!("New version available: v{latest_tag} (current: v{CURRENT_VERSION})");
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(suffix))
        .with_context(|| {
            let available: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            format!(
                "No binary found for this platform (looking for *{suffix}). Available assets: {available:?}"
            )
        })?;

    info!("Downloading {}...", asset.name);

    let archive_bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("Failed to download update")?
        .bytes()
        .await
        .context("Failed to read update archive")?;

    info!("Extracting...");

    let tmp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let archive_path = tmp_dir.path().join("archive.tar.gz");
    std::fs::write(&archive_path, &archive_bytes).context("Failed to write archive to disk")?;

    let tar_gz = std::fs::File::open(&archive_path).context("Failed to open downloaded archive")?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    archive
        .unpack(tmp_dir.path())
        .context("Failed to extract archive")?;

    let new_binary = tmp_dir.path().join("swe-forge");
    if !new_binary.exists() {
        bail!("Binary not found in downloaded archive");
    }

    let current_path = current_exe_path()?;
    info!("Replacing binary at {}", current_path.display());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&new_binary, perms).context("Failed to set binary permissions")?;
    }

    let backup_path = current_path.with_extension("old");
    if current_path.exists() {
        std::fs::rename(&current_path, &backup_path)
            .context("Failed to create backup of current binary")?;
    }

    match std::fs::rename(&new_binary, &current_path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup_path);
        }
        Err(rename_err) => match std::fs::copy(&new_binary, &current_path) {
            Ok(_) => {
                let _ = std::fs::remove_file(&backup_path);
            }
            Err(copy_err) => {
                if backup_path.exists() {
                    let _ = std::fs::rename(&backup_path, &current_path);
                }
                bail!(
                        "Failed to install new binary (rename: {rename_err}, copy: {copy_err}). Original binary restored."
                    );
            }
        },
    }

    info!("Successfully updated to v{latest_tag}");
    info!("Restart swe-forge to use the new version.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_asset_suffix() {
        let result = platform_asset_suffix();
        if cfg!(target_os = "linux") {
            assert!(result.is_ok());
            let suffix = result.unwrap();
            assert!(suffix.ends_with(".tar.gz"));
        }
    }

    #[test]
    fn test_current_version_is_valid_semver() {
        let version = semver::Version::parse(CURRENT_VERSION);
        assert!(
            version.is_ok(),
            "CARGO_PKG_VERSION is not valid semver: {CURRENT_VERSION}"
        );
    }
}
