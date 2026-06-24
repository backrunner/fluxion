use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use fluxion_bt::BtEngine;
use fluxion_core::{DownloadEngine, FluxionCore};
use fluxion_ftp::FtpEngine;
use fluxion_http::HttpEngine;
use fluxion_sftp::SftpEngine;
use fluxion_storage::SqliteTaskStore;

pub async fn build_core(data_dir: PathBuf) -> Result<Arc<FluxionCore>> {
    tokio::fs::create_dir_all(&data_dir).await?;
    let store = Arc::new(SqliteTaskStore::connect(data_dir.join("fluxion.sqlite")).await?);
    let engines: Vec<Arc<dyn DownloadEngine>> = vec![
        Arc::new(HttpEngine::new()?),
        Arc::new(FtpEngine::new()),
        Arc::new(SftpEngine::new()),
        Arc::new(BtEngine::new()),
    ];
    let core = Arc::new(FluxionCore::new(store, engines));
    core.initialize().await?;
    Ok(core)
}

pub fn default_data_dir() -> PathBuf {
    std::env::var_os("FLUXION_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".fluxion")
        })
}
