use std::path::{Path, PathBuf};

use fluxion_core::{FluxionError, FluxionErrorKind, ProxyConfig, Result};

pub async fn ensure_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

pub fn sanitize_file_name(input: &str) -> String {
    let cleaned = input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ => ch,
        })
        .collect::<String>();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed
    }
}

pub fn safe_join(save_dir: &Path, file_name: &str) -> Result<PathBuf> {
    let sanitized = sanitize_file_name(file_name);
    let path = save_dir.join(sanitized);
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "unsafe output path",
        ));
    }
    Ok(path)
}

pub fn part_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".fluxionpart");
    PathBuf::from(os)
}

pub async fn preallocate(path: &Path, len: u64) -> Result<()> {
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .await?;
    file.set_len(len).await?;
    Ok(())
}

pub async fn system_proxy() -> Result<Option<ProxyConfig>> {
    Ok(None)
}
