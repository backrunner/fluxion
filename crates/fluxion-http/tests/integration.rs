//! End-to-end HTTP download tests against a local, hand-rolled test server
//! (development plan §6.2): 302 redirects, Range/206 segmented downloads,
//! servers without Range support, and final file content verification.

use std::sync::Arc;

use fluxion_core::{
    CreateTaskInput, FluxionCore, HeaderPair, HttpTaskConfig, ProxyPolicy, TaskCredentials,
    TaskKind, TaskRateLimit, TaskState,
};
use fluxion_http::HttpEngine;
use fluxion_storage::SqliteTaskStore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Which behaviour the test server should exhibit.
#[derive(Clone, Copy, PartialEq)]
enum ServerMode {
    /// Full Range support: HEAD advertises ranges, GET honours them with 206.
    Ranges,
    /// No Range support at all: every GET returns 200 with the full body.
    NoRanges,
    /// HEAD claims Range support but GET ignores it (lying server).
    LyingHead,
    /// First response is a 302 redirect to `/file`; target supports ranges.
    Redirect,
    /// Empty resource: Range 0-0 correctly returns 416, full GET returns 200.
    Empty,
}

async fn spawn_server(body: Arc<Vec<u8>>, mode: ServerMode) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let _ = handle_conn(stream, body, mode).await;
            });
        }
    });
    format!("http://{addr}")
}

async fn handle_conn(
    mut stream: TcpStream,
    body: Arc<Vec<u8>>,
    mode: ServerMode,
) -> std::io::Result<()> {
    loop {
        // Read one request (headers end at CRLFCRLF; no request bodies).
        let mut buf = Vec::new();
        let mut byte = [0_u8; 1];
        loop {
            let n = stream.read(&mut byte).await?;
            if n == 0 {
                return Ok(());
            }
            buf.push(byte[0]);
            if buf.ends_with(b"\r\n\r\n") {
                break;
            }
            if buf.len() > 64 * 1024 {
                return Ok(());
            }
        }
        let request = String::from_utf8_lossy(&buf);
        let mut lines = request.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let range = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("range")
                    .then(|| value.trim().to_string())
            })
            .next();

        let honour_ranges =
            matches!(mode, ServerMode::Ranges) || (mode == ServerMode::Redirect && path == "/file");
        let advertise_ranges = honour_ranges || mode == ServerMode::LyingHead;

        if mode == ServerMode::Redirect && path == "/" {
            stream
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: /file\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n")
                .await?;
            continue;
        }

        let total = body.len();
        let accept_ranges = if advertise_ranges {
            "Accept-Ranges: bytes\r\n"
        } else {
            ""
        };
        if method == "HEAD" {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\n{accept_ranges}ETag: \"test-etag\"\r\nConnection: keep-alive\r\n\r\n"
            );
            stream.write_all(head.as_bytes()).await?;
            continue;
        }

        if total == 0 && range.is_some() {
            stream
                .write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */0\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n")
                .await?;
            continue;
        }

        match range.filter(|_| honour_ranges) {
            Some(range) => {
                let spec = range.trim_start_matches("bytes=");
                let (start, end) = spec.split_once('-').unwrap_or(("0", ""));
                let start: usize = start.parse().unwrap_or(0);
                let end: usize = if end.is_empty() {
                    total - 1
                } else {
                    end.parse().unwrap_or(total - 1).min(total - 1)
                };
                let slice = &body[start..=end];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{total}\r\nETag: \"test-etag\"\r\nConnection: keep-alive\r\n\r\n",
                    slice.len()
                );
                stream.write_all(head.as_bytes()).await?;
                stream.write_all(slice).await?;
            }
            None => {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nETag: \"test-etag\"\r\nConnection: keep-alive\r\n\r\n"
                );
                stream.write_all(head.as_bytes()).await?;
                stream.write_all(&body).await?;
            }
        }
    }
}

fn test_body(len: usize) -> Vec<u8> {
    // Deterministic pseudo-random content so offset mistakes shift bytes.
    let mut out = Vec::with_capacity(len);
    let mut state = 0x12345678_u32;
    for _ in 0..len {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        out.push((state >> 24) as u8);
    }
    out
}

async fn build_core(dir: &std::path::Path) -> Arc<FluxionCore> {
    let store = Arc::new(
        SqliteTaskStore::connect(dir.join("test.sqlite"))
            .await
            .expect("store"),
    );
    let core = Arc::new(FluxionCore::new(
        store,
        vec![Arc::new(HttpEngine::new().expect("engine"))],
    ));
    core.initialize().await.expect("init");
    core
}

fn http_input(url: &str, save_dir: &std::path::Path, min_split: u64) -> CreateTaskInput {
    CreateTaskInput {
        kind: fluxion_core::TaskKind::Http(HttpTaskConfig {
            url: url.parse().expect("url"),
            method: fluxion_core::HttpMethod::Get,
            headers: Vec::<HeaderPair>::new(),
            max_connections: Some(4),
            min_split_size: Some(min_split),
            redirect_limit: HttpTaskConfig::DEFAULT_REDIRECT_LIMIT,
        }),
        save_dir: save_dir.to_path_buf(),
        file_name: Some("out.bin".to_string()),
        limits: TaskRateLimit::default(),
        proxy: ProxyPolicy::Direct,
        credentials: TaskCredentials::default(),
    }
}

/// Run one download to terminal state; panics on failure/timeout.
async fn run_to_completion(core: &FluxionCore, input: CreateTaskInput) -> fluxion_core::TaskId {
    let task_id = core.create_task(input).await.expect("create");
    core.start_task(task_id).await.expect("start");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        assert!(tokio::time::Instant::now() < deadline, "download timed out");
        let detail = core
            .get_task(task_id)
            .await
            .expect("get")
            .expect("task exists");
        match detail.task.state {
            TaskState::Completed => return task_id,
            TaskState::Failed => panic!("download failed: {:?}", detail.task.error),
            _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
}

#[tokio::test]
async fn segmented_download_via_ranges_produces_exact_content() {
    let body = Arc::new(test_body(512 * 1024));
    let url = spawn_server(body.clone(), ServerMode::Ranges).await;
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    // min_split 64 KiB → 4 segments over 512 KiB.
    run_to_completion(&core, http_input(&url, dir.path(), 64 * 1024)).await;
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert_eq!(written.len(), body.len());
    assert_eq!(&written, body.as_ref(), "content must match byte-for-byte");
}

#[tokio::test]
async fn server_without_ranges_falls_back_to_single_connection() {
    let body = Arc::new(test_body(300 * 1024));
    let url = spawn_server(body.clone(), ServerMode::NoRanges).await;
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    run_to_completion(&core, http_input(&url, dir.path(), 64 * 1024)).await;
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert_eq!(&written, body.as_ref());
}

#[tokio::test]
async fn lying_head_is_caught_by_range_probe() {
    // HEAD advertises Accept-Ranges but GET ignores Range: the 0-0 probe
    // must detect this and the download completes single-threaded.
    let body = Arc::new(test_body(200 * 1024));
    let url = spawn_server(body.clone(), ServerMode::LyingHead).await;
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    run_to_completion(&core, http_input(&url, dir.path(), 32 * 1024)).await;
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert_eq!(&written, body.as_ref());
}

#[tokio::test]
async fn redirect_is_followed_to_final_resource() {
    let body = Arc::new(test_body(256 * 1024));
    let url = spawn_server(body.clone(), ServerMode::Redirect).await;
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    run_to_completion(&core, http_input(&url, dir.path(), 64 * 1024)).await;
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert_eq!(&written, body.as_ref());
}

#[tokio::test]
async fn completed_task_cannot_be_restarted() {
    let body = Arc::new(test_body(64 * 1024));
    let url = spawn_server(body.clone(), ServerMode::Ranges).await;
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    let task_id = run_to_completion(&core, http_input(&url, dir.path(), 64 * 1024)).await;
    // Restarting a completed task would clobber the finished file.
    let result = core.start_task(task_id).await;
    assert!(result.is_err(), "start on completed task must be rejected");
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert_eq!(&written, body.as_ref(), "file must remain intact");
}

#[tokio::test]
async fn zero_length_resource_downloads_as_an_empty_file() {
    let body = Arc::new(Vec::new());
    let url = spawn_server(body, ServerMode::Empty).await;
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    run_to_completion(&core, http_input(&url, dir.path(), 64 * 1024)).await;
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert!(written.is_empty());
}

#[tokio::test]
async fn signed_source_url_is_restored_from_isolated_credentials() {
    let body = Arc::new(test_body(32 * 1024));
    let base = spawn_server(body.clone(), ServerMode::NoRanges).await;
    let url = format!("{base}?X-Amz-Signature=top-secret");
    let dir = tempfile::tempdir().expect("tmp");
    let core = build_core(dir.path()).await;
    let task_id = run_to_completion(&core, http_input(&url, dir.path(), 64 * 1024)).await;
    let detail = core.get_task(task_id).await.unwrap().unwrap();
    let TaskKind::Http(config) = detail.task.kind else {
        panic!("expected HTTP task");
    };
    assert!(!config.url.as_str().contains("top-secret"));
    let written = std::fs::read(dir.path().join("out.bin")).expect("output file");
    assert_eq!(&written, body.as_ref());
}
