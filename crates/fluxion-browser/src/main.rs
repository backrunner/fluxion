use fluxion_browser::*;
use std::{path::PathBuf, time::Duration};
use tokio::net::UnixStream;

#[tokio::main]
async fn main() {
    let argument = std::env::args().nth(1).unwrap_or_default();
    if argument == "--install" {
        let result = std::env::current_exe()
            .ok()
            .zip(std::env::var_os("HOME"))
            .ok_or_else(|| anyhow::anyhow!("Unavailable installation directory"))
            .and_then(|(host, home)| register_hosts(&host, &PathBuf::from(home)));
        if result.is_ok() {
            eprintln!("Fluxion browser host installed.");
        } else {
            eprintln!("Unable to install Fluxion browser host.");
            std::process::exit(1);
        }
        return;
    }
    if !allowed_origin(&argument) {
        // Never process stdin from an unapproved browser extension.
        std::process::exit(1);
    }
    let response = match run().await {
        Ok(response) => response,
        Err(_) => serde_json::to_vec(&Response::error("app_unavailable")).unwrap(),
    };
    let _ = write_frame(&mut tokio::io::stdout(), &response).await;
}

async fn run() -> anyhow::Result<Vec<u8>> {
    let body = tokio::time::timeout(Duration::from_secs(10), read_frame(&mut tokio::io::stdin()))
        .await??;
    // Validate before trying to launch the application. Never echo parse errors.
    let _: Request = serde_json::from_slice(&body)?;
    let mut stream = match UnixStream::connect(socket_path()).await {
        Ok(stream) => stream,
        Err(_) => {
            let exe = std::env::current_exe()?;
            let app = exe
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .filter(|p| p.extension().is_some_and(|e| e == "app"))
                .ok_or_else(|| anyhow::anyhow!("Start Fluxion first"))?;
            anyhow::ensure!(
                tokio::process::Command::new("/usr/bin/open")
                    .arg("-g")
                    .arg(app)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await?
                    .success(),
                "Launch failed"
            );
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            loop {
                if let Ok(stream) = UnixStream::connect(socket_path()).await {
                    break stream;
                }
                anyhow::ensure!(tokio::time::Instant::now() < deadline, "Launch timed out");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    };
    write_frame(&mut stream, &body).await?;
    // The user is choosing options in Add Task. Keep the IPC response pending.
    read_frame(&mut stream).await
}
