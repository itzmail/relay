use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const RELAY_DIR: &str = ".relay";
const CACHE_FILE: &str = ".relay/version_check.json";
const CACHE_TTL_SECS: u64 = 86400; // 24 hours
const GITHUB_REPO: &str = "itzmail/relay";
const CHECK_TIMEOUT_SECS: u64 = 3;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub release_url: String,
    pub asset_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct VersionCache {
    checked_at: u64,
    latest_version: String,
    release_url: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_cache() -> Option<VersionCache> {
    let content = fs::read_to_string(CACHE_FILE).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_cache(entry: &VersionCache) {
    let _ = fs::create_dir_all(RELAY_DIR);
    if let Ok(json) = serde_json::to_string(entry) {
        let _ = fs::write(CACHE_FILE, json);
    }
}

fn detect_asset_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => other,
    };
    format!("relay-{}-{}", os, arch)
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

async fn fetch_latest_release() -> Result<(String, String, Option<String>)> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(CHECK_TIMEOUT_SECS))
        .user_agent(format!("relay/{}", CURRENT_VERSION))
        .build()?;

    let release: GithubRelease = client.get(&url).send().await?.json().await?;

    let tag = release.tag_name.trim_start_matches('v').to_string();
    let asset_name = detect_asset_name();
    let asset_url = release
        .assets
        .iter()
        .find(|a| a.name.starts_with(&asset_name))
        .map(|a| a.browser_download_url.clone());

    Ok((tag, release.html_url, asset_url))
}

pub async fn check_latest_version() -> Option<UpdateInfo> {
    let current = semver::Version::parse(CURRENT_VERSION).ok()?;

    if let Some(cache) = read_cache() {
        if now_secs() < cache.checked_at + CACHE_TTL_SECS {
            let latest = semver::Version::parse(&cache.latest_version).ok()?;
            if latest > current {
                return Some(UpdateInfo {
                    current: CURRENT_VERSION.to_string(),
                    latest: cache.latest_version,
                    release_url: cache.release_url,
                    asset_url: None,
                });
            }
            return None;
        }
    }

    let (latest_tag, release_url, asset_url) = fetch_latest_release().await.ok()?;
    let latest = semver::Version::parse(&latest_tag).ok()?;

    write_cache(&VersionCache {
        checked_at: now_secs(),
        latest_version: latest_tag.clone(),
        release_url: release_url.clone(),
    });

    if latest > current {
        Some(UpdateInfo {
            current: CURRENT_VERSION.to_string(),
            latest: latest_tag,
            release_url,
            asset_url,
        })
    } else {
        None
    }
}

/// Force check, bypass cache. Returns None if already up to date.
pub async fn force_check_latest_version() -> Result<Option<UpdateInfo>> {
    let current = semver::Version::parse(CURRENT_VERSION)
        .map_err(|e| anyhow::anyhow!("Invalid current version: {}", e))?;

    let (latest_tag, release_url, asset_url) = fetch_latest_release().await?;
    let latest = semver::Version::parse(&latest_tag)
        .map_err(|e| anyhow::anyhow!("Invalid latest version tag '{}': {}", latest_tag, e))?;

    write_cache(&VersionCache {
        checked_at: now_secs(),
        latest_version: latest_tag.clone(),
        release_url: release_url.clone(),
    });

    if latest > current {
        Ok(Some(UpdateInfo {
            current: CURRENT_VERSION.to_string(),
            latest: latest_tag,
            release_url,
            asset_url,
        }))
    } else {
        Ok(None)
    }
}

pub async fn download_and_install(asset_url: &str) -> Result<()> {
    let exe_path = std::env::current_exe()?;
    let tmp_path = exe_path.with_extension("new");

    let client = reqwest::Client::builder()
        .user_agent(format!("relay/{}", CURRENT_VERSION))
        .build()?;

    let bytes = client.get(asset_url).send().await?.bytes().await?;

    if bytes.is_empty() {
        anyhow::bail!("Downloaded binary is empty");
    }

    fs::write(&tmp_path, &bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))?;
    }

    // Atomic replace — Unix: rename overwrites running binary safely
    // TODO: Windows needs rename-old-then-rename-new strategy
    fs::rename(&tmp_path, &exe_path)?;

    Ok(())
}

/// Return path for version cache file, used for testing
pub fn cache_file_path() -> PathBuf {
    PathBuf::from(CACHE_FILE)
}
