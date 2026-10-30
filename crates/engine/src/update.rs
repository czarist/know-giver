//! New versions, from the project's GitHub releases.
//!
//! When the app opens it asks GitHub for the latest release; only that
//! request leaves the computer, and Settings can turn it off. Updating
//! downloads this system's package from the release, checks it against the
//! release's `SHA256SUMS.txt` and installs it: the Windows installer runs
//! silently, macOS and Linux run the same install script as the README. The
//! app then closes and the new version opens.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{Engine, Event};

/// Where the code and the releases are.
pub const REPOSITORY: &str = "https://github.com/czarist/know-giver";
const LATEST: &str = "https://api.github.com/repos/czarist/know-giver/releases/latest";
const CHECKSUMS: &str = "SHA256SUMS.txt";
const USER_AGENT: &str = concat!(
    "KnowGiver/",
    env!("CARGO_PKG_VERSION"),
    " (update check; https://github.com/czarist/know-giver)"
);
/// The macOS and Linux install script, the one the README runs.
#[cfg(not(windows))]
const INSTALL_SH: &str = include_str!("../../../scripts/install.sh");

/// A release newer than this app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Version without the leading "v", e.g. "0.33.0".
    pub version: String,
    /// The release page, with its notes.
    pub page: String,
    package: Option<Asset>,
    checksums: Option<String>,
}

impl Release {
    /// Whether it has a package for this system, so it can install itself;
    /// otherwise the user downloads it from [`Release::page`].
    pub fn installable(&self) -> bool {
        self.package.is_some() && self.checksums.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Asset {
    name: String,
    url: String,
    size: u64,
}

/// What the update offer shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    Available(Release),
    Downloading {
        release: Release,
        done: u64,
        total: u64,
    },
    /// Installed or handed to the installer: the app is closing.
    Installing(Release),
    /// Could not install; the release page is the way out.
    Failed(Release),
}

impl UpdateStatus {
    pub fn release(&self) -> &Release {
        match self {
            Self::Available(r)
            | Self::Downloading { release: r, .. }
            | Self::Installing(r)
            | Self::Failed(r) => r,
        }
    }
}

#[derive(Default)]
pub(crate) struct Updates {
    status: Option<UpdateStatus>,
    progress: Arc<AtomicU64>,
    checking: bool,
    dismissed: bool,
    quit: bool,
}

impl Engine {
    /// The update on offer, unless there is none or it was put off.
    pub fn update(&self) -> Option<UpdateStatus> {
        let updates = &self.updates;
        if updates.dismissed {
            return None;
        }
        match updates.status.clone()? {
            UpdateStatus::Downloading { release, total, .. } => Some(UpdateStatus::Downloading {
                release,
                done: updates.progress.load(Ordering::Relaxed),
                total,
            }),
            other => Some(other),
        }
    }

    /// True once, when the app should close for the new version.
    pub fn take_quit_request(&mut self) -> bool {
        std::mem::take(&mut self.updates.quit)
    }

    /// Asks GitHub for the latest release, if checks are on.
    pub(crate) fn check_for_update(&mut self) {
        if !self.settings.check_updates || self.updates.checking || self.updates.status.is_some() {
            return;
        }
        self.updates.checking = true;
        self.spawn(|| Event::UpdateChecked(fetch_latest()));
    }

    pub(crate) fn on_update_checked(&mut self, result: Result<Option<Release>, String>) {
        self.updates.checking = false;
        match result {
            Ok(Some(release)) => {
                tracing::info!(version = %release.version, "update available");
                self.updates.status = Some(UpdateStatus::Available(release));
            }
            Ok(None) => tracing::debug!("app is up to date"),
            Err(err) => tracing::info!(%err, "update check failed"),
        }
    }

    /// Downloads and installs the release on offer.
    pub(crate) fn install_update(&mut self) {
        let Some(UpdateStatus::Available(release) | UpdateStatus::Failed(release)) =
            self.updates.status.clone()
        else {
            return;
        };
        if !release.installable() {
            return;
        }
        let total = release.package.as_ref().map_or(0, |p| p.size);
        let progress = Arc::new(AtomicU64::new(0));
        self.updates.progress = Arc::clone(&progress);
        self.updates.status = Some(UpdateStatus::Downloading {
            release: release.clone(),
            done: 0,
            total,
        });
        let dir = self.paths.data_dir.join("updates");
        self.spawn(move || Event::UpdateInstalled(install(&release, &dir, &progress)));
    }

    pub(crate) fn on_update_installed(&mut self, result: Result<(), String>) {
        let Some(release) = self.updates.status.as_ref().map(|s| s.release().clone()) else {
            return;
        };
        match result {
            Ok(()) => {
                self.updates.status = Some(UpdateStatus::Installing(release));
                self.updates.quit = true;
            }
            Err(err) => {
                tracing::warn!(%err, "update failed");
                self.updates.status = Some(UpdateStatus::Failed(release));
            }
        }
    }

    /// Hides the offer until the app opens again.
    pub(crate) fn dismiss_update(&mut self) {
        if !matches!(self.updates.status, Some(UpdateStatus::Downloading { .. })) {
            self.updates.dismissed = true;
        }
    }

    /// Shows an offer without asking GitHub, for screenshots and tests.
    #[doc(hidden)]
    pub fn simulate_update(&mut self, version: &str) {
        self.updates.status = Some(UpdateStatus::Available(Release {
            version: version.into(),
            page: format!("{REPOSITORY}/releases/tag/v{version}"),
            package: None,
            checksums: None,
        }));
    }
}

/// This system's package in a release, as the release workflow names it.
fn package_name() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("KnowGiver-setup.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("KnowGiver-linux-x86_64.tar.gz")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("KnowGiver-macos-arm64.tar.gz")
    } else {
        None
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(timeout))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn fetch_latest() -> Result<Option<Release>, String> {
    let body = agent(Duration::from_secs(20))
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    parse_release(&body, env!("CARGO_PKG_VERSION"), package_name())
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// The release in GitHub's answer, when it is newer than `current`.
fn parse_release(
    json: &str,
    current: &str,
    package: Option<&str>,
) -> Result<Option<Release>, String> {
    let api: ApiRelease = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let (Some(latest), Some(running)) = (version(&api.tag_name), version(current)) else {
        return Err(format!("unreadable version {:?}", api.tag_name));
    };
    if api.draft || api.prerelease || latest <= running {
        return Ok(None);
    }
    let asset = |name: &str| api.assets.iter().find(|a| a.name == name);
    Ok(Some(Release {
        version: api.tag_name.trim_start_matches('v').to_owned(),
        page: api.html_url.clone(),
        package: package.and_then(asset).map(|a| Asset {
            name: a.name.clone(),
            url: a.browser_download_url.clone(),
            size: a.size,
        }),
        checksums: asset(CHECKSUMS).map(|a| a.browser_download_url.clone()),
    }))
}

/// "v1.2.3" or "1.2.3-beta" as (1, 2, 3).
fn version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().unwrap_or(Some(0))?,
    ))
}

/// The SHA-256 listed for `name` in a `sha256sum` file.
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

fn install(release: &Release, dir: &Path, progress: &AtomicU64) -> Result<(), String> {
    let (Some(package), Some(sums_url)) = (&release.package, &release.checksums) else {
        return Err("no package for this system".into());
    };
    let agent = agent(Duration::from_secs(900));
    let sums = agent
        .get(sums_url)
        .call()
        .map_err(|e| e.to_string())?
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let expected = expected_hash(&sums, &package.name)
        .ok_or_else(|| format!("{} is not in {CHECKSUMS}", package.name))?;
    // Packages of earlier updates are not kept.
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(&package.name);
    let hash = download(&agent, &package.url, &file, progress)?;
    if hash != expected {
        let _ = fs::remove_file(&file);
        return Err(format!("checksum mismatch for {}", package.name));
    }
    run_installer(&file, dir)
}

/// Streams `url` into `dest`, returning its SHA-256 in hex.
fn download(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    progress: &AtomicU64,
) -> Result<String, String> {
    let response = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .map_err(|e| format!("{}: {e}", dest.display()))?;
        done += n as u64;
        progress.store(done, Ordering::Relaxed);
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Windows: the installer runs silently, closes this app and opens the new
/// version. macOS and Linux: the install script puts the package in place
/// and opens the new version.
#[cfg(windows)]
fn run_installer(package: &Path, _dir: &Path) -> Result<(), String> {
    std::process::Command::new(package)
        .args([
            "/SILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
        ])
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not start the installer: {e}"))
}

#[cfg(not(windows))]
fn run_installer(package: &Path, dir: &Path) -> Result<(), String> {
    let script = dir.join("install.sh");
    fs::write(&script, INSTALL_SH).map_err(|e| format!("{}: {e}", script.display()))?;
    let status = std::process::Command::new("bash")
        .arg(&script)
        .env("KG_ARCHIVE", package)
        .env("KG_LAUNCH", "1")
        .status()
        .map_err(|e| format!("could not run the install script: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("install script failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(tag: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/czarist/know-giver/releases/tag/{tag}",
               "draft":false,"prerelease":false,"assets":[
               {{"name":"KnowGiver-setup.exe","browser_download_url":"https://example.test/setup.exe","size":10}},
               {{"name":"SHA256SUMS.txt","browser_download_url":"https://example.test/sums","size":1}}]}}"#
        )
    }

    #[test]
    fn only_newer_releases_are_offered() {
        let found = parse_release(&answer("v0.33.0"), "0.32.1", Some("KnowGiver-setup.exe"))
            .unwrap()
            .unwrap();
        assert_eq!(found.version, "0.33.0");
        assert!(found.installable());
        assert!(found.page.ends_with("/v0.33.0"));

        for same_or_older in ["v0.32.1", "v0.32.0", "v0.9.9"] {
            assert_eq!(
                parse_release(&answer(same_or_older), "0.32.1", None).unwrap(),
                None
            );
        }
        // 0.40 is newer than 0.32, not older as text would say.
        assert!(
            parse_release(&answer("v0.40.0"), "0.32.1", None)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn a_system_without_a_package_gets_the_release_page() {
        let found = parse_release(
            &answer("v1.0.0"),
            "0.32.1",
            Some("KnowGiver-macos-arm64.tar.gz"),
        )
        .unwrap()
        .unwrap();
        assert!(!found.installable());
        let none = parse_release(&answer("v1.0.0"), "0.32.1", None)
            .unwrap()
            .unwrap();
        assert!(!none.installable());
    }

    #[test]
    fn versions_and_checksums_are_read() {
        assert_eq!(version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(version("1.2"), Some((1, 2, 0)));
        assert_eq!(version("latest"), None);
        let sums = "ABC123  KnowGiver-setup.exe\ndef456 *KnowGiver-linux-x86_64.tar.gz\n";
        assert_eq!(
            expected_hash(sums, "KnowGiver-setup.exe").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            expected_hash(sums, "KnowGiver-linux-x86_64.tar.gz").as_deref(),
            Some("def456")
        );
        assert_eq!(expected_hash(sums, "other"), None);
    }
}
