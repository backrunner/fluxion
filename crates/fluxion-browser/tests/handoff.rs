use fluxion_browser::*;
use std::time::Duration;
use tokio::net::UnixStream;

fn download(url: &str) -> Download {
    Download {
        url: url.into(),
        filename: Some("../../secret.bin".into()),
        headers: vec![
            Header {
                name: "Cookie".into(),
                value: "session=private-cookie".into(),
            },
            Header {
                name: "Authorization".into(),
                value: "Bearer private-auth".into(),
            },
            Header {
                name: "Referer".into(),
                value: "https://example.test/?opaque=private-referrer".into(),
            },
            Header {
                name: "X-Api-Key".into(),
                value: "private-api-key".into(),
            },
        ],
    }
}

fn request(command: Command) -> Request {
    Request {
        version: 1,
        command,
    }
}

#[test]
fn schema_origin_and_input_validation() {
    assert!(allowed_origin(&format!(
        "chrome-extension://{EXTENSION_ID}/"
    )));
    for origin in [
        "https://example.test",
        "chrome-extension://other/",
        "",
        &format!("chrome-extension://{EXTENSION_ID}/evil"),
    ] {
        assert!(!allowed_origin(origin));
    }
    assert!(serde_json::from_str::<Request>(r#"{"version":1,"action":"ping"}"#).is_ok());
    for raw in [
        r#"{"version":1,"action":"ping","url":"secret"}"#,
        r#"{"version":1,"action":"start","request_id":"bad"}"#,
        r#"{"version":1,"action":"delete_all"}"#,
        r#"{"action":"ping"}"#,
    ] {
        assert!(serde_json::from_str::<Request>(raw).is_err(), "{raw}");
    }
    for url in [
        "file:///etc/passwd",
        "ftp://host/file",
        "https://user:secret@host/file",
    ] {
        assert!(download(url).validate().is_err());
    }
    for name in [
        "Host",
        "Range",
        "Proxy-Authorization",
        "X-Forwarded-For",
        "bad\nname",
    ] {
        let mut input = download("https://example.test/file");
        input.headers.push(Header {
            name: name.into(),
            value: "x".into(),
        });
        assert!(input.validate().is_err());
    }
    let mut input = download("https://example.test/file");
    input.headers[0].value = "bad\r\nInjected: true".into();
    assert!(input.validate().is_err());
    let input = download("https://example.test/file?opaque=private-url")
        .validate()
        .unwrap();
    assert_eq!(input.filename.as_deref(), Some("secret.bin"));
    assert!(input.url.contains("private-url"));
    assert_eq!(input.headers[0].value, "session=private-cookie");
}

#[tokio::test]
async fn framing_rejects_oversized_and_truncated_messages() {
    let mut oversized = ((MAX_MESSAGE + 1) as u32).to_ne_bytes().to_vec();
    assert!(read_frame(&mut oversized.as_slice()).await.is_err());
    oversized = 16_u32.to_ne_bytes().to_vec();
    oversized.extend_from_slice(b"short");
    assert!(read_frame(&mut oversized.as_slice()).await.is_err());
    let mut buffer = Vec::new();
    write_frame(&mut buffer, b"hello").await.unwrap();
    assert_eq!(read_frame(&mut buffer.as_slice()).await.unwrap(), b"hello");
}

#[test]
fn registration_limits_origins_and_preserves_absolute_paths() {
    let temp = tempfile::tempdir().unwrap();
    let helper = temp.path().join("App With Spaces/host");
    std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
    std::fs::write(&helper, "test").unwrap();
    register_hosts(&helper, temp.path()).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(temp.path().join(
        "Library/Application Support/Google/Chrome/NativeMessagingHosts/top.backrunner.fluxion.json"
    )).unwrap()).unwrap();
    assert_eq!(
        manifest["path"],
        helper.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        manifest["allowed_origins"],
        serde_json::json!([format!("chrome-extension://{EXTENSION_ID}/")])
    );
}

async fn exchange(directory: &std::path::Path, command: Command) -> Response {
    let mut stream = UnixStream::connect(directory.join("bridge.sock"))
        .await
        .unwrap();
    write_frame(&mut stream, &serde_json::to_vec(&request(command)).unwrap())
        .await
        .unwrap();
    serde_json::from_slice(&read_frame(&mut stream).await.unwrap()).unwrap()
}

#[tokio::test]
async fn native_host_transfers_credentials_and_waits_for_the_add_task_decision() {
    let directory = tempfile::Builder::new()
        .prefix("fx-")
        .tempdir_in("/tmp")
        .unwrap();
    let listener = bind(directory.path()).await.unwrap();
    assert!(bind(directory.path()).await.is_err());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(directory.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let (drafts, mut incoming) = tokio::sync::mpsc::channel(1);
    let server = tokio::spawn(serve(listener, drafts));
    let mut helper = tokio::process::Command::new(env!("CARGO_BIN_EXE_fluxion-browser-host"))
        .arg(format!("chrome-extension://{EXTENSION_ID}/"))
        .env("FLUXION_BROWSER_DIR", directory.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    write_frame(
        &mut helper.stdin.take().unwrap(),
        &serde_json::to_vec(&request(Command::AddDownload {
            download: download("https://example.test/file?opaque=private-url"),
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let draft = tokio::time::timeout(Duration::from_secs(3), incoming.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(draft.download.filename.as_deref(), Some("secret.bin"));
    assert!(draft.download.url.contains("private-url"));
    assert_eq!(draft.download.headers[0].value, "session=private-cookie");
    assert!(
        helper.try_wait().unwrap().is_none(),
        "Host must wait for the form decision"
    );
    // Ping is independent of the open form; no Core or SecretStore was needed.
    assert_eq!(
        exchange(directory.path(), Command::Ping).await.status,
        "connected"
    );
    let id = fluxion_core::TaskId::new_v4();
    draft.reply.send(Response::task("created", id)).unwrap();
    let response: Response = serde_json::from_slice(
        &read_frame(&mut helper.stdout.take().unwrap())
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(response.ok);
    assert_eq!(response.task_id, Some(id));
    assert_eq!(response.status, "created");
    assert!(helper.wait().await.unwrap().success());
    server.abort();
}

#[tokio::test]
async fn cancelling_the_form_replies_cancelled_and_disconnect_invalidates_the_draft() {
    let directory = tempfile::Builder::new()
        .prefix("fx-")
        .tempdir_in("/tmp")
        .unwrap();
    let (drafts, mut incoming) = tokio::sync::mpsc::channel(1);
    let server = tokio::spawn(serve(bind(directory.path()).await.unwrap(), drafts));
    let mut stream = UnixStream::connect(directory.path().join("bridge.sock"))
        .await
        .unwrap();
    let body = serde_json::to_vec(&request(Command::AddDownload {
        download: download("https://example.test/file"),
    }))
    .unwrap();
    write_frame(&mut stream, &body).await.unwrap();
    let draft = incoming.recv().await.unwrap();
    drop(draft);
    let response: Response =
        serde_json::from_slice(&read_frame(&mut stream).await.unwrap()).unwrap();
    assert!(!response.ok);
    assert_eq!(response.status, "cancelled");
    let mut stream = UnixStream::connect(directory.path().join("bridge.sock"))
        .await
        .unwrap();
    write_frame(&mut stream, &body).await.unwrap();
    let mut draft = incoming.recv().await.unwrap();
    drop(stream);
    tokio::time::timeout(Duration::from_secs(2), draft.reply.closed())
        .await
        .unwrap();
    assert!(draft.reply.is_closed());
    server.abort();
}

#[tokio::test]
async fn native_host_rejects_unapproved_origin_without_reading_stdin() {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_fluxion-browser-host"))
        .arg("chrome-extension://untrusted/")
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
