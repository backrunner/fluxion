//! Real SSH/SFTP wire tests with ephemeral keys and no external endpoints.
use fluxion_core::{
    CreateTaskInput, FluxionCore, ProxyPolicy, SftpTaskConfig, TaskCredentials, TaskKind,
    TaskRateLimit, TaskState,
};
use fluxion_sftp::SftpEngine;
use fluxion_storage::SqliteTaskStore;
use russh::keys::{Algorithm, PrivateKey};
use russh::{
    Channel, ChannelId,
    server::{Auth, Msg, Server as _, Session},
};
use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, OpenFlags, Status, StatusCode, Version,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
struct Handler {
    body: Arc<Vec<u8>>,
    offsets: Arc<Mutex<Vec<u64>>>,
    key: russh::keys::PublicKey,
    stall: bool,
}
struct Ssh {
    handler: Handler,
    channels: HashMap<ChannelId, Channel<Msg>>,
}
impl russh::server::Server for Handler {
    type Handler = Ssh;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Ssh {
        Ssh {
            handler: self.clone(),
            channels: HashMap::new(),
        }
    }
}
impl russh::server::Handler for Ssh {
    type Error = russh::Error;
    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(
            if user == "fixture-user" && password == "fixture-password" {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(if user == "fixture-user" && *key == self.handler.key {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channels.insert(channel.id(), channel);
        Ok(true)
    }
    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            session.channel_success(id)?;
            let channel = self.channels.remove(&id).unwrap();
            let handler = self.handler.clone();
            tokio::spawn(async move {
                russh_sftp::server::run(channel.into_stream(), handler).await;
            });
        } else {
            session.channel_failure(id)?;
        }
        Ok(())
    }
    async fn channel_eof(
        &mut self,
        id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.close(id)?;
        Ok(())
    }
}
impl russh_sftp::server::Handler for Handler {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn init(&mut self, _: u32, _: HashMap<String, String>) -> Result<Version, StatusCode> {
        Ok(Version::new())
    }
    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        if path != "/fixture.bin" {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(self.body.len() as u64),
                ..Default::default()
            },
        })
    }
    async fn fstat(&mut self, id: u32, _: String) -> Result<Attrs, StatusCode> {
        self.stat(id, "/fixture.bin".into()).await
    }
    async fn open(
        &mut self,
        id: u32,
        path: String,
        flags: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        if path != "/fixture.bin" || !flags.contains(OpenFlags::READ) {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Handle {
            id,
            handle: "fixture".into(),
        })
    }
    async fn read(
        &mut self,
        id: u32,
        _: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        self.offsets.lock().unwrap().push(offset);
        if self.stall {
            std::future::pending::<()>().await;
        }
        let offset = offset as usize;
        if offset >= self.body.len() {
            return Err(StatusCode::Eof);
        }
        Ok(Data {
            id,
            data: self.body[offset..(offset + len as usize).min(self.body.len())].to_vec(),
        })
    }
    async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
}
struct Server {
    address: std::net::SocketAddr,
    offsets: Arc<Mutex<Vec<u64>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn new(body: Arc<Vec<u8>>, client_key: &PrivateKey, stall: bool) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let offsets = Arc::new(Mutex::new(vec![]));
        let mut server = Handler {
            body,
            offsets: offsets.clone(),
            key: client_key.public_key().clone(),
            stall,
        };
        let config = Arc::new(russh::server::Config {
            keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap()],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let task = tokio::spawn(async move {
            server.run_on_socket(config, &listener).await.unwrap();
        });
        Self {
            address,
            offsets,
            task,
        }
    }
}
async fn core(dir: &std::path::Path) -> Arc<FluxionCore> {
    let storage = Arc::new(
        SqliteTaskStore::connect(dir.join("tasks.sqlite"))
            .await
            .unwrap(),
    );
    let core = Arc::new(FluxionCore::new(
        storage,
        vec![Arc::new(SftpEngine::with_known_hosts(
            dir.join("known_hosts"),
        ))],
    ));
    core.initialize().await.unwrap();
    core
}
fn input(
    server: &Server,
    path: &std::path::Path,
    key: Option<std::path::PathBuf>,
    password: Option<&str>,
) -> CreateTaskInput {
    CreateTaskInput {
        kind: TaskKind::Sftp(SftpTaskConfig {
            url: format!("sftp://{}/fixture.bin", server.address)
                .parse()
                .unwrap(),
            username: None,
            private_key_path: key,
        }),
        save_dir: path.into(),
        file_name: Some("fixture.bin".into()),
        limits: TaskRateLimit::default(),
        proxy: ProxyPolicy::Direct,
        credentials: TaskCredentials {
            username: Some("fixture-user".into()),
            password: password.map(str::to_owned),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn password_and_private_key_resume_produce_exact_bytes_and_persist_host_pin() {
    let bytes = Arc::new(
        (0..128 * 1024)
            .map(|i| (i * 19 % 251) as u8)
            .collect::<Vec<_>>(),
    );
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let server = Server::new(bytes.clone(), &key, false).await;
    for private_key in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        tokio::fs::write(dir.path().join("fixture.bin.fluxionpart"), &bytes[..32768])
            .await
            .unwrap();
        let key_path = if private_key {
            let path = dir.path().join("fixture-key");
            key.write_openssh_file(&path, Default::default()).unwrap();
            Some(path)
        } else {
            None
        };
        let before = server.offsets.lock().unwrap().len();
        let id = core
            .create_task(input(
                &server,
                dir.path(),
                key_path,
                if private_key {
                    None
                } else {
                    Some("fixture-password")
                },
            ))
            .await
            .unwrap();
        core.start_task(id).await.unwrap();
        let detail = terminal(&core, id).await;
        assert_eq!(
            detail.task.state,
            TaskState::Completed,
            "{:?}",
            detail.task.error
        );
        assert_eq!(
            tokio::fs::read(dir.path().join("fixture.bin"))
                .await
                .unwrap(),
            *bytes
        );
        assert_eq!(server.offsets.lock().unwrap()[before], 32768);
        assert!(dir.path().join("known_hosts").exists());
        // A fresh Core uses the same saved pin rather than trusting a new key.
        drop(core);
        let restarted = core_for_restart(dir.path()).await;
        let id = restarted
            .create_task(input(&server, dir.path(), None, Some("fixture-password")))
            .await
            .unwrap();
        restarted.start_task(id).await.unwrap();
        assert_eq!(
            terminal(&restarted, id).await.task.state,
            TaskState::Completed
        );
    }
}
async fn core_for_restart(path: &std::path::Path) -> Arc<FluxionCore> {
    core(path).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_host_key_and_bad_password_are_rejected() {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let server = Server::new(Arc::new(vec![1; 100]), &key, false).await;
    for wrong_pin in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        if wrong_pin {
            tokio::fs::write(
                dir.path().join("known_hosts"),
                format!("127.0.0.1 {} SHA256:wrong-key\n", server.address.port()),
            )
            .await
            .unwrap();
        }
        let core = core(dir.path()).await;
        let id = core
            .create_task(input(&server, dir.path(), None, Some("incorrect")))
            .await
            .unwrap();
        core.start_task(id).await.unwrap();
        assert_eq!(terminal(&core, id).await.task.state, TaskState::Failed);
        assert!(!dir.path().join("fixture.bin").exists());
    }
    assert!(server.offsets.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_interrupts_a_stalled_sftp_read() {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let server = Server::new(Arc::new(vec![1; 100]), &key, true).await;
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path()).await;
    let id = core
        .create_task(input(&server, dir.path(), None, Some("fixture-password")))
        .await
        .unwrap();
    core.start_task(id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while server.offsets.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), core.pause_task(id))
        .await
        .expect("pause hung on SFTP read")
        .unwrap();
    assert_eq!(
        core.get_task(id).await.unwrap().unwrap().task.state,
        TaskState::Paused
    );
    assert!(!dir.path().join("fixture.bin").exists());
}
