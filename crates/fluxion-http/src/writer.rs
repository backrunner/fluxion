use std::{io::SeekFrom, path::Path};

use fluxion_core::Result;
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt},
    sync::Mutex,
};

pub struct PositionedWriter {
    file: Mutex<File>,
}

impl PositionedWriter {
    pub async fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .await?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<()> {
        let mut file = self.file.lock().await;
        file.seek(SeekFrom::Start(offset)).await?;
        file.write_all(data).await?;
        Ok(())
    }

    pub async fn flush(&self) -> Result<()> {
        self.file.lock().await.flush().await?;
        Ok(())
    }
}
