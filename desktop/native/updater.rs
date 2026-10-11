//! Native, channel-aware updates. Only verified releases can reach the installer.
use crate::backend::Message;
#[cfg(not(feature = "app-store"))]
use base64::Engine;
use serde::{Deserialize, Serialize};
#[cfg(not(feature = "app-store"))]
use std::collections::HashMap;
#[cfg(not(feature = "app-store"))]
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(not(feature = "app-store"))]
const BASE: &str = "https://assets.fluxion.alkinum.io";
#[cfg(not(feature = "app-store"))]
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc1QUU2RjM4MUU0MjQ5N0EKUldSNlNVSWVPRyt1ZFRCMlhMOGVNUTV0THYzaDVpT2hnb0ZJYllSb3E4SE91RC9ZSm9Ma3IrOUMK";
pub const VERSION: &str = match option_env!("FLUXION_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}
impl Default for Channel {
    fn default() -> Self {
        if option_env!("FLUXION_CHANNEL") == Some("beta") {
            Self::Beta
        } else {
            Self::Stable
        }
    }
}
impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
    #[cfg(not(feature = "app-store"))]
    fn matches_version(self, version: &semver::Version) -> bool {
        if self == Self::Stable {
            version.pre.is_empty()
        } else {
            version
                .pre
                .as_str()
                .strip_prefix("beta.")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    #[cfg(not(feature = "app-store"))]
    pub channel: Option<Channel>,
    #[serde(default)]
    pub notes: String,
    #[cfg(not(feature = "app-store"))]
    pub platforms: HashMap<String, Artifact>,
}
#[cfg(not(feature = "app-store"))]
#[derive(Clone, Debug, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub signature: String,
}

pub enum Status {
    Idle,
    Checking,
    Current,
    Available,
    Downloading(u64, Option<u64>),
    #[cfg_attr(feature = "app-store", allow(dead_code))]
    Verifying,
    Ready,
    Installing,
    Error(String),
}
impl Status {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading(..) | Self::Verifying | Self::Installing
        )
    }
    pub fn installing(&self) -> bool {
        matches!(
            self,
            Self::Downloading(..) | Self::Verifying | Self::Installing
        )
    }
}

// The temporary directory is owned until the user requests a restart.
// Changing channels or quitting before installation discards the staged update.
pub struct PreparedUpdate {
    #[cfg(not(feature = "app-store"))]
    pub release: Release,
    #[cfg(not(feature = "app-store"))]
    staging: tempfile::TempDir,
    #[cfg(not(feature = "app-store"))]
    bundle: PathBuf,
    #[cfg(not(feature = "app-store"))]
    unpacked: PathBuf,
    #[cfg(not(feature = "app-store"))]
    digest: [u8; 32],
}
#[cfg(not(feature = "app-store"))]
fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("Fluxion/{VERSION}"))
        .build()?)
}
#[cfg(not(feature = "app-store"))]
fn target() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "darwin-aarch64"
    } else {
        "darwin-x86_64"
    }
}
#[cfg(not(feature = "app-store"))]
fn release_channel(release: &Release) -> anyhow::Result<Channel> {
    let version = semver::Version::parse(&release.version)?;
    let channel = if version.pre.is_empty() {
        Channel::Stable
    } else {
        Channel::Beta
    };
    anyhow::ensure!(
        channel.matches_version(&version),
        "Unsupported release version"
    );
    anyhow::ensure!(
        release.channel.is_none_or(|c| c == channel),
        "Release channel does not match its version"
    );
    Ok(channel)
}
#[cfg(not(feature = "app-store"))]
fn artifact_url(release: &Release) -> anyhow::Result<url::Url> {
    let channel = release_channel(release)?;
    let artifact = release
        .platforms
        .get(target())
        .ok_or_else(|| anyhow::anyhow!("No update is available for this Mac"))?;
    let url = url::Url::parse(&artifact.url)?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("assets.fluxion.alkinum.io")
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path().starts_with(&format!(
                "/releases/{}/{}/",
                channel.as_str(),
                release.version
            ))
            && url.path().ends_with(".app.tar.gz"),
        "Invalid update artifact URL"
    );
    anyhow::ensure!(
        !artifact.signature.trim().is_empty(),
        "Update signature is missing"
    );
    Ok(url)
}
#[cfg(not(feature = "app-store"))]
fn newer(release: &Release, current: &str, selected: Channel) -> anyhow::Result<bool> {
    let channel = release_channel(release)?;
    anyhow::ensure!(
        selected == Channel::Beta || channel == Channel::Stable,
        "Stable cannot install a beta release"
    );
    artifact_url(release)?;
    Ok(semver::Version::parse(&release.version)?
        .cmp_precedence(&semver::Version::parse(current)?)
        .is_gt())
}
#[cfg(not(feature = "app-store"))]
async fn fetch_feed(channel: Channel) -> anyhow::Result<Option<Release>> {
    let mut response = client()?
        .get(format!("{BASE}/updates/{}/latest.json", channel.as_str()))
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    response = response.error_for_status()?;
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            data.len() + chunk.len() <= 1024 * 1024,
            "Update manifest is too large"
        );
        data.extend_from_slice(&chunk);
    }
    let release: Release = serde_json::from_slice(&data)?;
    anyhow::ensure!(
        release_channel(&release)? == channel,
        "Unexpected release channel"
    );
    artifact_url(&release)?;
    Ok(Some(release))
}
#[cfg(not(feature = "app-store"))]
pub async fn check(channel: Channel) -> anyhow::Result<Option<Release>> {
    let releases = if channel == Channel::Stable {
        vec![fetch_feed(Channel::Stable).await?]
    } else {
        // Beta users also receive the final stable build when their beta graduates.
        let (beta, stable) = tokio::join!(fetch_feed(Channel::Beta), fetch_feed(Channel::Stable));
        match (beta, stable) {
            (Ok(a), Ok(b)) => vec![a, b],
            (Ok(Some(a)), Err(_)) | (Err(_), Ok(Some(a))) if newer(&a, VERSION, channel)? => {
                vec![Some(a)]
            }
            (Err(e), _) | (_, Err(e)) => return Err(e),
        }
    };
    select_release(VERSION, channel, releases.into_iter().flatten())
}
#[cfg(not(feature = "app-store"))]
fn select_release(
    current: &str,
    channel: Channel,
    releases: impl Iterator<Item = Release>,
) -> anyhow::Result<Option<Release>> {
    let mut selected: Option<Release> = None;
    for release in releases {
        if newer(&release, current, channel)?
            && selected.as_ref().is_none_or(|old| {
                semver::Version::parse(&release.version)
                    .unwrap()
                    .cmp_precedence(&semver::Version::parse(&old.version).unwrap())
                    .is_gt()
            })
        {
            selected = Some(release);
        }
    }
    Ok(selected)
}
#[cfg(not(feature = "app-store"))]
pub fn verify(data: &[u8], signature: &str) -> anyhow::Result<()> {
    verify_with_key(data, signature, PUBLIC_KEY)
}
#[cfg(not(feature = "app-store"))]
fn verify_with_key(data: &[u8], signature: &str, public_key: &str) -> anyhow::Result<()> {
    let decode = |s: &str| -> anyhow::Result<String> {
        Ok(String::from_utf8(
            base64::engine::general_purpose::STANDARD.decode(s)?,
        )?)
    };
    let key = minisign_verify::PublicKey::decode(&decode(public_key)?)?;
    let signature = minisign_verify::Signature::decode(&decode(signature)?)?;
    key.verify(data, &signature, true)?;
    Ok(())
}
#[cfg(not(feature = "app-store"))]
fn installed_bundle() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe()?.canonicalize()?;
    executable
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Run Fluxion from its installed .app to install updates"))
}
#[cfg(not(feature = "app-store"))]
async fn validate_bundle(app: &Path, version: &str) -> anyhow::Result<()> {
    let status = tokio::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .await?;
    anyhow::ensure!(
        status.status.success(),
        "The downloaded application has an invalid code signature"
    );
    for (key, expected) in [
        ("CFBundleIdentifier", "top.backrunner.fluxion"),
        ("CFBundleShortVersionString", version),
        ("CFBundleExecutable", "fluxion-app"),
    ] {
        let value = tokio::process::Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}")])
            .arg(app.join("Contents/Info.plist"))
            .output()
            .await?;
        anyhow::ensure!(
            value.status.success() && String::from_utf8_lossy(&value.stdout).trim() == expected,
            "The downloaded application identity or version is incorrect"
        );
    }
    Ok(())
}
#[cfg(not(feature = "app-store"))]
pub async fn prepare(
    release: Release,
    channel: Channel,
    tx: async_channel::Sender<Message>,
) -> anyhow::Result<PreparedUpdate> {
    anyhow::ensure!(
        newer(&release, VERSION, channel)?,
        "The update is not newer than this installation"
    );
    let url = artifact_url(&release)?;
    let bundle = installed_bundle()?;
    let parent = bundle
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid application location"))?;
    let staging = tempfile::Builder::new().prefix(".fluxion-update-").tempdir_in(parent)
        .map_err(|_| anyhow::anyhow!("Fluxion cannot update here. Move the app to a writable Applications folder and try again."))?;
    let mut response = client()?.get(url).send().await?.error_for_status()?;
    let total = response.content_length();
    anyhow::ensure!(
        total.is_none_or(|n| n <= 512 * 1024 * 1024),
        "Update archive exceeds 512 MB"
    );
    let mut data = Vec::new();
    let mut last = std::time::Instant::now();
    let _ = tx.send(Message::UpdateProgress(0, total)).await;
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            data.len() + chunk.len() <= 512 * 1024 * 1024,
            "Update archive exceeds 512 MB"
        );
        data.extend_from_slice(&chunk);
        if last.elapsed() > Duration::from_millis(250) {
            let _ = tx
                .send(Message::UpdateProgress(data.len() as u64, total))
                .await;
            last = std::time::Instant::now();
        }
    }
    let _ = tx.send(Message::UpdateVerifying).await;
    let signature = release.platforms[target()].signature.clone();
    let archive_path = staging.path().to_owned();
    let unpacked = tokio::task::spawn_blocking(move || {
        verify(&data, &signature)?;
        extract(&data, &archive_path)
    })
    .await??;
    validate_bundle(&unpacked, &release.version).await?;
    let path = unpacked.clone();
    let digest = tokio::task::spawn_blocking(move || bundle_digest(&path)).await??;
    Ok(PreparedUpdate {
        release,
        staging,
        bundle,
        unpacked,
        digest,
    })
}
#[cfg(not(feature = "app-store"))]
pub async fn install(prepared: PreparedUpdate) -> anyhow::Result<()> {
    // Verify again after the user has chosen to restart; staged files may have been modified.
    let path = prepared.unpacked.clone();
    anyhow::ensure!(
        tokio::task::spawn_blocking(move || bundle_digest(&path)).await?? == prepared.digest,
        "The staged update was modified. Download it again."
    );
    validate_bundle(&prepared.unpacked, &prepared.release.version).await?;
    anyhow::ensure!(
        prepared.bundle == installed_bundle()?,
        "The installed application moved; download the update again"
    );
    let helper = prepared.staging.path().join("install.sh");
    tokio::fs::write(&helper, include_str!("install-update.sh")).await?;
    tokio::process::Command::new("/bin/sh")
        .arg(helper)
        .arg(&prepared.bundle)
        .arg(&prepared.unpacked)
        .arg(prepared.staging.path().join("previous.app"))
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let _ = prepared.staging.keep();
    Ok(())
}
#[cfg(not(feature = "app-store"))]
fn bundle_digest(root: &Path) -> anyhow::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::{io::Read, os::unix::fs::PermissionsExt};
    fn visit(root: &Path, path: &Path, hash: &mut Sha256) -> anyhow::Result<()> {
        let meta = path.symlink_metadata()?;
        anyhow::ensure!(
            meta.is_file() || meta.is_dir(),
            "Unexpected staged update entry"
        );
        let name = path.strip_prefix(root)?.as_os_str().as_encoded_bytes();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name);
        hash.update(meta.permissions().mode().to_le_bytes());
        if meta.is_dir() {
            let mut children = std::fs::read_dir(path)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            children.sort();
            for child in children {
                visit(root, &child, hash)?;
            }
        } else {
            hash.update(meta.len().to_le_bytes());
            let mut file = std::fs::File::open(path)?;
            let mut buffer = [0; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    visit(root, root, &mut hash)?;
    Ok(hash.finalize().into())
}
#[cfg(not(feature = "app-store"))]
pub fn extract(data: &[u8], destination: &Path) -> anyhow::Result<PathBuf> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(data));
    let mut expanded = 0u64;
    let mut entries = 0usize;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        anyhow::ensure!(
            !path.is_absolute()
                && !path.components().any(|c| matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::Prefix(_)
                ))
                && path.components().next()
                    == Some(std::path::Component::Normal(std::ffi::OsStr::new(
                        "Fluxion.app"
                    ))),
            "Unsafe update archive path"
        );
        let kind = entry.header().entry_type();
        anyhow::ensure!(
            kind.is_file() || kind.is_dir(),
            "Update archive contains a link or special file"
        );
        entries += 1;
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("Update size overflow"))?;
        anyhow::ensure!(
            expanded <= 1024 * 1024 * 1024 && entries <= 50_000,
            "Expanded update is too large"
        );
        anyhow::ensure!(entry.unpack_in(destination)?, "Unsafe update archive entry");
    }
    let app = destination.join("Fluxion.app");
    anyhow::ensure!(
        app.join("Contents/MacOS/fluxion-app").is_file()
            && app.join("Contents/Info.plist").is_file(),
        "Update is missing the Fluxion app bundle"
    );
    Ok(app)
}

#[cfg(all(test, not(feature = "app-store")))]
mod tests;

// Store builds contain no feed client, archive extraction or installer script.
#[cfg(feature = "app-store")]
pub async fn check(_: Channel) -> anyhow::Result<Option<Release>> {
    anyhow::bail!("Updates are managed by the Mac App Store")
}
#[cfg(feature = "app-store")]
pub async fn prepare(
    _: Release,
    _: Channel,
    _: async_channel::Sender<Message>,
) -> anyhow::Result<PreparedUpdate> {
    anyhow::bail!("Updates are managed by the Mac App Store")
}
#[cfg(feature = "app-store")]
pub async fn install(_: PreparedUpdate) -> anyhow::Result<()> {
    anyhow::bail!("Updates are managed by the Mac App Store")
}
