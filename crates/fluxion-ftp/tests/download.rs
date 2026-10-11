//! FTP wire tests with an owned loopback-only passive server.
use fluxion_core::{
    CreateTaskInput, FluxionCore, FtpTaskConfig, ProxyPolicy, TaskCredentials, TaskKind,
    TaskRateLimit, TaskState,
};
use fluxion_ftp::FtpEngine;
use fluxion_storage::SqliteTaskStore;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

struct Server {
    address: std::net::SocketAddr,
    task: tokio::task::JoinHandle<()>,
    offsets: Arc<Mutex<Vec<usize>>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn new(body: Arc<Vec<u8>>, stall: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let offsets = Arc::new(Mutex::new(vec![]));
        let record = offsets.clone();
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        let body = body.clone(); let record = record.clone();
                        connections.spawn(async move { let _ = serve(stream, body, record, stall).await; });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Self {
            address,
            task,
            offsets,
        }
    }
}
async fn serve(
    stream: TcpStream,
    body: Arc<Vec<u8>>,
    offsets: Arc<Mutex<Vec<usize>>>,
    stall: bool,
) -> std::io::Result<()> {
    let mut control = BufReader::new(stream);
    control
        .get_mut()
        .write_all(b"220 Fixture ready\r\n")
        .await?;
    let mut passive = None;
    let mut offset = 0usize;
    let mut authenticated = false;
    loop {
        let mut line = String::new();
        if control.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let (command, value) = line
            .trim_end()
            .split_once(' ')
            .unwrap_or((line.trim_end(), ""));
        let reply = match command {
            "USER" => "331 Password required\r\n",
            "PASS" => {
                authenticated = value == "fixture-password";
                if authenticated {
                    "230 Logged in\r\n"
                } else {
                    "530 Login rejected\r\n"
                }
            }
            "TYPE" => "200 Binary\r\n",
            "SIZE" if authenticated => {
                control
                    .get_mut()
                    .write_all(format!("213 {}\r\n", body.len()).as_bytes())
                    .await?;
                continue;
            }
            "EPSV" | "PASV" => {
                let listener = TcpListener::bind("127.0.0.1:0").await?;
                let port = listener.local_addr()?.port();
                let response = if command == "EPSV" {
                    format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                } else {
                    format!(
                        "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                        port / 256,
                        port % 256
                    )
                };
                passive = Some(listener);
                control.get_mut().write_all(response.as_bytes()).await?;
                continue;
            }
            "REST" => {
                offset = value.parse().unwrap();
                offsets.lock().unwrap().push(offset);
                "350 Restart accepted\r\n"
            }
            "RETR" if authenticated => {
                control.get_mut().write_all(b"150 Opening data\r\n").await?;
                let listener = passive.take().unwrap();
                let (mut data, _) = listener.accept().await?;
                if stall {
                    std::future::pending::<()>().await;
                }
                for chunk in body[offset..].chunks(8192) {
                    data.write_all(chunk).await?;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                data.shutdown().await?;
                drop(data);
                "226 Transfer complete\r\n"
            }
            "ABOR" => "226 Aborted\r\n",
            "QUIT" => {
                control.get_mut().write_all(b"221 Goodbye\r\n").await?;
                return Ok(());
            }
            _ => "502 Unsupported\r\n",
        };
        control.get_mut().write_all(reply.as_bytes()).await?;
    }
}
async fn core(dir: &std::path::Path) -> Arc<FluxionCore> {
    let storage = Arc::new(
        SqliteTaskStore::connect(dir.join("tasks.sqlite"))
            .await
            .unwrap(),
    );
    let core = Arc::new(FluxionCore::new(storage, vec![Arc::new(FtpEngine::new())]));
    core.initialize().await.unwrap();
    core
}
fn input(server: &Server, path: &std::path::Path, password: &str) -> CreateTaskInput {
    CreateTaskInput {
        kind: TaskKind::Ftp(FtpTaskConfig {
            url: format!("ftp://{}/fixture.bin", server.address)
                .parse()
                .unwrap(),
            username: None,
            passive: true,
            ftps: false,
        }),
        save_dir: path.into(),
        file_name: Some("fixture.bin".into()),
        limits: TaskRateLimit::default(),
        proxy: ProxyPolicy::Direct,
        credentials: TaskCredentials {
            username: Some("fixture-user".into()),
            password: Some(password.into()),
            ..Default::default()
        },
    }
}
async fn terminal(core: &FluxionCore, id: fluxion_core::TaskId) -> fluxion_core::TaskDetail {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let detail = core.get_task(id).await.unwrap().unwrap();
            if matches!(detail.task.state, TaskState::Completed | TaskState::Failed) {
                break detail;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn passive_download_and_rest_resume_produce_exact_content() {
    let bytes = Arc::new(
        (0..256 * 1024)
            .map(|i| (i * 17 % 251) as u8)
            .collect::<Vec<_>>(),
    );
    let server = Server::new(bytes.clone(), false).await;
    for resume in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        if resume {
            tokio::fs::write(dir.path().join("fixture.bin.fluxionpart"), &bytes[..32768])
                .await
                .unwrap();
        }
        let id = core
            .create_task(input(&server, dir.path(), "fixture-password"))
            .await
            .unwrap();
        core.start_task(id).await.unwrap();
        assert_eq!(terminal(&core, id).await.task.state, TaskState::Completed);
        assert_eq!(
            tokio::fs::read(dir.path().join("fixture.bin"))
                .await
                .unwrap(),
            *bytes
        );
        assert!(!dir.path().join("fixture.bin.fluxionpart").exists());
    }
    assert_eq!(*server.offsets.lock().unwrap(), vec![32768]);
}

#[tokio::test]
async fn rejected_password_does_not_publish_a_file() {
    let server = Server::new(Arc::new(vec![1; 100]), false).await;
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path()).await;
    let id = core
        .create_task(input(&server, dir.path(), "incorrect"))
        .await
        .unwrap();
    core.start_task(id).await.unwrap();
    let detail = terminal(&core, id).await;
    assert_eq!(detail.task.state, TaskState::Failed);
    assert!(!dir.path().join("fixture.bin").exists());
}

#[tokio::test]
async fn pause_interrupts_a_stalled_data_connection() {
    let server = Server::new(Arc::new(vec![1; 100]), true).await;
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path()).await;
    let id = core
        .create_task(input(&server, dir.path(), "fixture-password"))
        .await
        .unwrap();
    core.start_task(id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !dir.path().join("fixture.bin.fluxionpart").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), core.pause_task(id))
        .await
        .expect("pause hung on FTP read")
        .unwrap();
    assert_eq!(
        core.get_task(id).await.unwrap().unwrap().task.state,
        TaskState::Paused
    );
    assert!(!dir.path().join("fixture.bin").exists());
}
