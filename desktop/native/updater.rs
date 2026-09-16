//! Signed native update support. The release format and verification key remain
//! compatible with installed versions, without linking a webview or Tauri.
use base64::Engine;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf, time::Duration};

const BASE: &str = "https://assets.fluxion.alkinum.io";
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc1QUU2RjM4MUU0MjQ5N0EKUldSNlNVSWVPRyt1ZFRCMlhMOGVNUTV0THYzaDVpT2hnb0ZJYllSb3E4SE91RC9ZSm9Ma3IrOUMK";
pub const VERSION: &str = match option_env!("FLUXION_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Clone, Deserialize)]
pub struct Release {
    pub version: String,
    pub platforms: HashMap<String, Artifact>,
}
#[derive(Clone, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub signature: String,
}
fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("Fluxion/{VERSION}"))
        .build()?)
}
pub async fn check() -> anyhow::Result<Option<Release>> {
    let channel = option_env!("FLUXION_CHANNEL").unwrap_or("stable");
    anyhow::ensure!(
        matches!(channel, "stable" | "beta"),
        "Invalid update channel"
    );
    let response = client()?
        .get(format!("{BASE}/updates/{channel}/latest.json"))
        .send()
        .await?
        .error_for_status()?;
    let mut response = response;
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            data.len() + chunk.len() <= 1024 * 1024,
            "Update manifest is too large"
        );
        data.extend_from_slice(&chunk);
    }
    let release: Release = serde_json::from_slice(&data)?;
    if semver::Version::parse(&release.version)? > semver::Version::parse(VERSION)? {
        Ok(Some(release))
    } else {
        Ok(None)
    }
}
pub fn verify(data: &[u8], signature: &str) -> anyhow::Result<()> {
    let decode = |s: &str| -> anyhow::Result<String> {
        Ok(String::from_utf8(
            base64::engine::general_purpose::STANDARD.decode(s)?,
        )?)
    };
    let key = minisign_verify::PublicKey::decode(&decode(PUBLIC_KEY)?)?;
    let signature = minisign_verify::Signature::decode(&decode(signature)?)?;
    key.verify(data, &signature, true)?;
    Ok(())
}
pub async fn install(
    release: Release,
    tx: async_channel::Sender<crate::backend::Message>,
) -> anyhow::Result<()> {
    let target = if cfg!(target_arch = "aarch64") {
        "darwin-aarch64"
    } else {
        "darwin-x86_64"
    };
    let artifact = release
        .platforms
        .get(target)
        .ok_or_else(|| anyhow::anyhow!("No update is available for this Mac"))?;
    let url = url::Url::parse(&artifact.url)?;
    anyhow::ensure!(
        url.scheme() == "https" && url.host_str() == Some("assets.fluxion.alkinum.io"),
        "Untrusted update host"
    );
    let executable = std::env::current_exe()?.canonicalize()?;
    let bundle = executable
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| anyhow::anyhow!("Run Fluxion from its installed .app to install updates"))?
        .to_owned();
    let parent = bundle
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid application location"))?;
    let staging = tempfile::Builder::new()
        .prefix(".fluxion-update-")
        .tempdir_in(parent)?;
    let mut response = client()?.get(url).send().await?.error_for_status()?;
    let total = response.content_length();
    let mut data = Vec::new();
    let mut last = std::time::Instant::now();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            data.len() + chunk.len() <= 512 * 1024 * 1024,
            "Update archive exceeds 512 MB"
        );
        data.extend_from_slice(&chunk);
        if last.elapsed() > Duration::from_millis(250) {
            let _ = tx
                .send(crate::backend::Message::UpdateProgress(
                    data.len() as u64,
                    total,
                ))
                .await;
            last = std::time::Instant::now();
        }
    }
    verify(&data, &artifact.signature)?;
    let archive_path = staging.path().to_owned();
    let unpacked = tokio::task::spawn_blocking(move || extract(&data, &archive_path)).await??;
    let status = tokio::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&unpacked)
        .output()
        .await?;
    anyhow::ensure!(
        status.status.success(),
        "The downloaded application has an invalid code signature"
    );
    let staged = staging.keep();
    let helper = staged.join("install.sh");
    // Positional arguments keep all paths out of shell source. Never run a shell assembled from a URL.
    tokio::fs::write(&helper, include_str!("install-update.sh")).await?;
    tokio::process::Command::new("/bin/sh")
        .arg(helper)
        .arg(&bundle)
        .arg(unpacked)
        .arg(staged.join("previous.app"))
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let _ = tx.send(crate::backend::Message::Restart).await;
    Ok(())
}
pub fn extract(data: &[u8], destination: &std::path::Path) -> anyhow::Result<PathBuf> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(data));
    let mut expanded = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        anyhow::ensure!(
            !path.is_absolute()
                && !path.components().any(|c| matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::Prefix(_)
                )),
            "Unsafe update archive path"
        );
        let kind = entry.header().entry_type();
        anyhow::ensure!(
            kind.is_file() || kind.is_dir(),
            "Update archive contains a link or special file"
        );
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("Update size overflow"))?;
        anyhow::ensure!(
            expanded <= 1024 * 1024 * 1024,
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
