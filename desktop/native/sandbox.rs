//! Public macOS APIs for file access. Security scopes stay on the main thread
//! until Core has flushed and stopped; the granted access is process-wide.
use objc2_foundation::{NSArray, NSString, NSURL};
use std::path::{Path, PathBuf};

pub fn open(path: &Path, reveal: bool) -> anyhow::Result<()> {
    let url = file_url(path)?;
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    if reveal {
        workspace.activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
    } else {
        anyhow::ensure!(workspace.openURL(&url), "Finder could not open this file");
    }
    Ok(())
}

fn file_url(path: &Path) -> anyhow::Result<objc2::rc::Retained<NSURL>> {
    anyhow::ensure!(path.is_absolute(), "Choose an absolute file path");
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?;
    Ok(NSURL::fileURLWithPath(&NSString::from_str(path)))
}

pub fn downloads_dir() -> PathBuf {
    use objc2_foundation::{
        NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
    };
    NSSearchPathForDirectoriesInDomains(
        NSSearchPathDirectory::DownloadsDirectory,
        NSSearchPathDomainMask::UserDomainMask,
        true,
    )
    .firstObject()
    .map(|s| {
        let path = PathBuf::from(s.to_string());
        path.canonicalize().unwrap_or(path)
    })
    .unwrap_or_default()
}

#[cfg(feature = "app-store")]
pub fn data_dir() -> PathBuf {
    // NSHomeDirectory honours App Sandbox; environment overrides must not redirect
    // the store build into a different application's data.
    PathBuf::from(objc2_foundation::NSHomeDirectory().to_string())
        .join("Library/Application Support/top.backrunner.fluxion/core")
}

#[cfg(feature = "app-store")]
mod bookmarks {
    use super::*;
    use objc2::{rc::Retained, runtime::Bool};
    use objc2_foundation::{
        NSData, NSURLBookmarkCreationOptions as Creation,
        NSURLBookmarkResolutionOptions as Resolution,
    };
    use std::{cell::RefCell, collections::BTreeMap, io::Write, os::unix::fs::OpenOptionsExt};

    struct Scope(Retained<NSURL>);
    impl Drop for Scope {
        fn drop(&mut self) {
            // Balanced with exactly one successful start in resolve().
            unsafe { self.0.stopAccessingSecurityScopedResource() };
        }
    }
    thread_local! {
        static SCOPES: RefCell<BTreeMap<PathBuf, Scope>> = RefCell::new(BTreeMap::new());
    }
    type Bookmarks = BTreeMap<PathBuf, Vec<u8>>;
    fn read() -> anyhow::Result<Bookmarks> {
        let path = data_dir().join("file-access.json");
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn save(bookmarks: &Bookmarks) -> anyhow::Result<()> {
        let directory = data_dir();
        std::fs::create_dir_all(&directory)?;
        let temporary = directory.join("file-access.json.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(bookmarks)?)?;
        file.sync_all()?;
        std::fs::rename(temporary, directory.join("file-access.json"))?;
        Ok(())
    }
    fn bookmark(url: &NSURL) -> anyhow::Result<Vec<u8>> {
        url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            Creation::WithSecurityScope,
            None,
            None,
        )
        .map(|data| data.to_vec())
        .map_err(|_| anyhow::anyhow!("Choose this folder or file again to grant access"))
    }
    fn resolve(path: &Path, bytes: &[u8]) -> anyhow::Result<(Scope, bool)> {
        let mut stale = Bool::NO;
        // The stale out-pointer is valid for the duration of the call.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &NSData::with_bytes(bytes),
                Resolution::WithSecurityScope | Resolution::WithoutUI,
                None,
                &mut stale,
            )
        }
        .map_err(|_| anyhow::anyhow!("File access expired; choose the folder or file again"))?;
        anyhow::ensure!(
            url.to_file_path().as_deref() == Some(path),
            "The folder or file moved; choose its new location"
        );
        // Starting access on the resolved URL grants access to Core's worker threads.
        anyhow::ensure!(
            unsafe { url.startAccessingSecurityScopedResource() },
            "File access expired; choose the folder or file again"
        );
        Ok((Scope(url), stale.as_bool()))
    }
    pub fn remember(path: &Path) -> anyhow::Result<()> {
        let url = file_url(path)?;
        let bytes = bookmark(&url)?;
        let (scope, _) = resolve(path, &bytes)?;
        let mut bookmarks = read()?;
        bookmarks.insert(path.into(), bytes);
        save(&bookmarks)?;
        SCOPES.with(|scopes| {
            scopes.borrow_mut().insert(path.into(), scope);
        });
        Ok(())
    }
    pub fn restore() -> anyhow::Result<()> {
        let mut bookmarks = read()?;
        let mut failed = false;
        let mut changed = false;
        for (path, bytes) in &mut bookmarks {
            match resolve(path, bytes) {
                Ok((scope, stale)) => {
                    if stale {
                        if let Ok(fresh) = bookmark(&scope.0) {
                            *bytes = fresh;
                            changed = true;
                        }
                    }
                    SCOPES.with(|scopes| {
                        scopes.borrow_mut().insert(path.clone(), scope);
                    });
                }
                Err(_) => failed = true,
            }
        }
        if changed {
            save(&bookmarks)?;
        }
        anyhow::ensure!(
            !failed,
            "Some file permissions expired. Choose those folders or files again before resuming."
        );
        Ok(())
    }
    pub fn clear() {
        SCOPES.with(|scopes| scopes.borrow_mut().clear());
    }
}
#[cfg(feature = "app-store")]
pub use bookmarks::{clear, remember, restore};
