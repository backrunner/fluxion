use std::path::{Path, PathBuf};

use fluxion_core::{FluxionError, FluxionErrorKind, ProxyConfig, Result, SecretStore};

/// macOS Keychain-backed secret store (design §7.3): sensitive credential
/// payloads live in the user's login keychain under one service name, keyed
/// by an opaque reference (the task id). The database only ever sees the
/// reference. Keychain calls are blocking, so they run on the blocking pool.
pub struct KeychainSecretStore {
    service: String,
}

impl KeychainSecretStore {
    pub fn new() -> Self {
        Self {
            service: "com.fluxion.credentials".to_string(),
        }
    }

    fn entry(&self, secret_ref: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, secret_ref).map_err(keychain_error)
    }
}

impl Default for KeychainSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SecretStore for KeychainSecretStore {
    async fn put(&self, secret_ref: &str, value: &str) -> Result<()> {
        let entry = self.entry(secret_ref)?;
        let value = value.to_string();
        tokio::task::spawn_blocking(move || entry.set_password(&value).map_err(keychain_error))
            .await
            .map_err(|error| FluxionError::new(FluxionErrorKind::Unknown, error.to_string()))?
    }

    async fn get(&self, secret_ref: &str) -> Result<Option<String>> {
        let entry = self.entry(secret_ref)?;
        tokio::task::spawn_blocking(move || match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        })
        .await
        .map_err(|error| FluxionError::new(FluxionErrorKind::Unknown, error.to_string()))?
    }

    async fn delete(&self, secret_ref: &str) -> Result<()> {
        let entry = self.entry(secret_ref)?;
        tokio::task::spawn_blocking(move || match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        })
        .await
        .map_err(|error| FluxionError::new(FluxionErrorKind::Unknown, error.to_string()))?
    }
}

fn keychain_error(error: keyring::Error) -> FluxionError {
    FluxionError::new(
        FluxionErrorKind::Storage,
        format!("keychain error: {error}"),
    )
}

pub async fn ensure_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

pub fn sanitize_file_name(input: &str) -> String {
    // Delegate to the canonical sanitizer so engines, storage and the app
    // shell always agree on the on-disk name.
    fluxion_core::sanitize_file_name(input)
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

/// Read the macOS system HTTP(S) proxy through SystemConfiguration. Returns `None`
/// when no proxy is enabled (or on non-macOS platforms). rustls-based reqwest
/// only honours env-var proxies by itself, so this bridges the gap between
/// "Use system proxy" in settings and the actual macOS network configuration.
pub async fn system_proxy() -> Result<Option<ProxyConfig>> {
    #[cfg(target_os = "macos")]
    {
        Ok(native_system_proxy())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Fall back to conventional environment variables elsewhere.
        for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            if let Some(value) = std::env::var_os(key)
                && let Ok(url) = url::Url::parse(&value.to_string_lossy())
            {
                return Ok(Some(ProxyConfig {
                    url,
                    username: None,
                }));
            }
        }
        Ok(None)
    }
}

#[cfg(target_os = "macos")]
fn native_system_proxy() -> Option<ProxyConfig> {
    use core_foundation::{
        base::{CFType, TCFType},
        dictionary::{CFDictionary, CFDictionaryRef},
        number::CFNumber,
        string::CFString,
    };
    #[link(name = "SystemConfiguration", kind = "framework")]
    unsafe extern "C" {
        fn SCDynamicStoreCopyProxies(store: *const std::ffi::c_void) -> CFDictionaryRef;
    }
    // Copy returns an owned dictionary. A null store asks for the current system configuration.
    let reference = unsafe { SCDynamicStoreCopyProxies(std::ptr::null()) };
    if reference.is_null() {
        return None;
    }
    let dictionary: CFDictionary<CFString, CFType> =
        unsafe { TCFType::wrap_under_create_rule(reference) };
    let mut text = String::new();
    for key in [
        "HTTPEnable",
        "HTTPProxy",
        "HTTPPort",
        "HTTPSEnable",
        "HTTPSProxy",
        "HTTPSPort",
        "SOCKSEnable",
        "SOCKSProxy",
        "SOCKSPort",
    ] {
        if let Some(value) = dictionary.find(CFString::new(key)) {
            let value = value
                .downcast::<CFString>()
                .map(|s| s.to_string())
                .or_else(|| {
                    value
                        .downcast::<CFNumber>()
                        .and_then(|n| n.to_i64())
                        .map(|n| n.to_string())
                });
            if let Some(value) = value {
                text.push_str(&format!("{key} : {value}\n"));
            }
        }
    }
    parse_scutil_proxy(&text)
}

#[cfg(target_os = "macos")]
fn parse_scutil_proxy(text: &str) -> Option<ProxyConfig> {
    let field = |name: &str| -> Option<String> {
        text.lines().find_map(|line| {
            let line = line.trim();
            line.strip_prefix(name)
                .and_then(|rest| rest.trim().strip_prefix(':'))
                .map(|value| value.trim().to_string())
        })
    };
    let enabled = |name: &str| field(name).is_some_and(|value| value == "1");
    // Prefer HTTPS proxy (downloads are mostly HTTPS), fall back to HTTP,
    // then SOCKS.
    let candidates = [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort", "http"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort", "http"),
        ("SOCKSEnable", "SOCKSProxy", "SOCKSPort", "socks5"),
    ];
    for (enable_key, host_key, port_key, scheme) in candidates {
        if enabled(enable_key)
            && let Some(host) = field(host_key)
            && let Some(port) = field(port_key)
            && let Ok(url) = url::Url::parse(&format!("{scheme}://{host}:{port}"))
        {
            return Some(ProxyConfig {
                url,
                username: None,
            });
        }
    }
    None
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn parses_scutil_output() {
        let sample = "<dictionary> {\n  HTTPEnable : 1\n  HTTPPort : 7890\n  HTTPProxy : 127.0.0.1\n  HTTPSEnable : 1\n  HTTPSPort : 7890\n  HTTPSProxy : 127.0.0.1\n  SOCKSEnable : 0\n}\n";
        let proxy = parse_scutil_proxy(sample).expect("proxy parsed");
        assert_eq!(proxy.url.as_str(), "http://127.0.0.1:7890/");
    }

    #[test]
    fn disabled_proxy_is_none() {
        let sample = "<dictionary> {\n  HTTPEnable : 0\n  HTTPSEnable : 0\n  SOCKSEnable : 0\n}\n";
        assert!(parse_scutil_proxy(sample).is_none());
    }
}
