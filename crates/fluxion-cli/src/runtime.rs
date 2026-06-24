use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use fluxion_core::FluxionCore;

pub async fn build_core(data_dir: PathBuf) -> Result<Arc<FluxionCore>> {
    fluxion_runtime::build_core(data_dir).await
}

pub fn default_data_dir() -> PathBuf {
    crate::ipc::default_data_dir()
}
