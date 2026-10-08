//! Update check against GitHub Releases.
//!
//! Every regular command prints a banner from a cached "latest version" file,
//! then refreshes that cache from a detached child process (at most once per
//! TTL) so the network call never slows the command down. `update` downloads
//! the release asset for this platform, verifies it against `SHA256SUMS` and
//! replaces the running binary.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::{Settings, home_dir};
use crate::{http, output};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
pub const REFRESH_ARG: &str = "__refresh-update-cache";
const DEFAULT_API: &str = "https://api.github.com/repos/Dxsk/repos-manager/releases/latest";
const DEFAULT_TTL: u64 = 86_400;
const MAX_ASSET_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn api_url() -> String {
    std::env::var("REPOS_MANAGER_UPDATE_URL").unwrap_or_else(|_| DEFAULT_API.to_string())
}

fn cache_path() -> PathBuf {
    if let Some(p) = std::env::var_os("REPOS_MANAGER_UPDATE_CACHE") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(dirs::cache_dir)
        .unwrap_or_else(|| home_dir().join(".cache"));
    base.join("repos-manager").join("latest-version")
}

fn ttl() -> Duration {
    let secs = std::env::var("REPOS_MANAGER_UPDATE_TTL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TTL);
    Duration::from_secs(secs)
}

fn parse_version(s: &str) -> Option<Version> {
    Version::parse(s.trim().trim_start_matches('v')).ok()
}

/// True when `latest` is strictly newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

fn cache_is_stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_none_or(|age| age >= ttl())
}

pub fn banner(settings: &Settings) {
    if !settings.check_updates {
        return;
    }
    let Ok(latest) = fs::read_to_string(cache_path()) else {
        return;
    };
    let latest = latest.trim();
    if is_newer(latest, CURRENT) {
        eprintln!(
            "{}",
            output::yellow(&format!(
                "⬆ repos-manager {latest} available (current {CURRENT}), run: repos-manager update"
            ))
        );
    }
}

/// Spawn a detached copy of ourselves to refresh the cache. Never fails the caller.
pub fn refresh_async(settings: &Settings) {
    if !settings.check_updates || !cache_is_stale(&cache_path()) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg(REFRESH_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group so Ctrl+C on the parent does not kill the refresh.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let _ = cmd.spawn();
}

fn fetch_release(timeout: Duration) -> Result<Release> {
    http::agent(timeout)
        .get(&api_url())
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("cannot reach the release API")?
        .body_mut()
        .read_json()
        .context("unexpected release API response")
}

/// Entry point of the detached child.
pub fn refresh_cache() -> Result<()> {
    let release = fetch_release(Duration::from_secs(5))?;
    let path = cache_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, release.tag_name.trim_start_matches('v'))?;
    Ok(())
}

/// Release asset name for the running platform, matching the release workflow.
pub fn asset_name() -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => bail!("no prebuilt binary for {other}"),
    };
    let arch = match std::env::consts::ARCH {
        a @ ("x86_64" | "aarch64") => a,
        other => bail!("no prebuilt binary for {other}"),
    };
    let ext = if os == "windows" { "zip" } else { "tar.gz" };
    Ok(format!("repos-manager-{os}-{arch}.{ext}"))
}

fn download(url: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    http::agent(Duration::from_secs(300))
        .get(url)
        .call()
        .with_context(|| format!("download failed: {url}"))?
        .body_mut()
        .as_reader()
        .take(MAX_ASSET_BYTES)
        .read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn expected_checksum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == asset).then(|| hash.to_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn extract_binary(archive: &[u8], asset: &str) -> Result<Vec<u8>> {
    let bin = if cfg!(windows) {
        "repos-manager.exe"
    } else {
        "repos-manager"
    };
    let is_bin = |p: &Path| p.file_name().is_some_and(|n| n == bin);
    let mut out = Vec::new();
    if asset.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(io::Cursor::new(archive))?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.enclosed_name().is_some_and(|p| is_bin(&p)) {
                f.read_to_end(&mut out)?;
                return Ok(out);
            }
        }
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in tar.entries()? {
            let mut e = entry?;
            if is_bin(&e.path()?) {
                e.read_to_end(&mut out)?;
                return Ok(out);
            }
        }
    }
    Err(anyhow!("{bin} not found in {asset}"))
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

pub fn self_update(assume_yes: bool) -> Result<()> {
    output::info("Checking for updates...");
    let release = fetch_release(Duration::from_secs(15))?;
    let latest = release.tag_name.trim_start_matches('v');
    if !is_newer(latest, CURRENT) {
        output::success(&format!("Already up to date ({CURRENT})"));
        return Ok(());
    }
    output::info(&format!(
        "New version available: {} (current: {CURRENT})",
        output::green(latest)
    ));
    if !assume_yes && !confirm("Update now?")? {
        return Ok(());
    }

    let name = asset_name()?;
    let find = |n: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == n)
            .map(|a| a.browser_download_url.clone())
            .ok_or_else(|| anyhow!("release {} has no asset {n}", release.tag_name))
    };
    let archive = download(&find(&name)?)?;
    let sums = String::from_utf8(download(&find("SHA256SUMS")?)?)?;
    let expected =
        expected_checksum(&sums, &name).ok_or_else(|| anyhow!("{name} missing from SHA256SUMS"))?;
    let actual = hex(&Sha256::digest(&archive));
    if actual != expected {
        bail!("checksum mismatch for {name}: expected {expected}, got {actual}");
    }

    let binary = extract_binary(&archive, &name)?;
    let tmp = std::env::temp_dir().join(format!("repos-manager-update-{}", std::process::id()));
    fs::write(&tmp, &binary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    let res = self_replace::self_replace(&tmp)
        .context("cannot replace the running binary (permissions?)");
    let _ = fs::remove_file(&tmp);
    res?;
    let _ = fs::write(cache_path(), latest);
    output::success(&format!("Updated to {latest}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.1.0", "1.0.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    #[test]
    fn checksum_lookup() {
        let sums =
            "abc123  repos-manager-linux-x86_64.tar.gz\nDEF456 *repos-manager-windows-x86_64.zip\n";
        assert_eq!(
            expected_checksum(sums, "repos-manager-linux-x86_64.tar.gz").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            expected_checksum(sums, "repos-manager-windows-x86_64.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(expected_checksum(sums, "nope"), None);
    }

    #[test]
    fn asset_name_for_this_platform() {
        let n = asset_name().unwrap();
        assert!(n.starts_with("repos-manager-"));
        assert!(n.ends_with(".tar.gz") || n.ends_with(".zip"));
    }

    #[test]
    fn extracts_from_tar_gz() {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let bin = if cfg!(windows) {
            "repos-manager.exe"
        } else {
            "repos-manager"
        };
        let data = b"binary";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, bin, &data[..]).unwrap();
        let gz = tar.into_inner().unwrap().finish().unwrap();
        assert_eq!(extract_binary(&gz, "x.tar.gz").unwrap(), data);
    }
}
