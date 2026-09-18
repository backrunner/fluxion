use crate::{
    backend::{self, Action, Command, Message},
    form::{build_input, parse_limit},
    model::*,
};
use fluxion_core::*;
use std::{collections::BTreeMap, time::Duration};

fn input(source: &str, directory: &str) -> CreateTaskInput {
    let fields = BTreeMap::from([
        ("source", source.into()),
        ("directory", directory.into()),
        ("connections", "16".into()),
    ]);
    build_input(&fields, Default::default(), vec![], false, true, vec![]).unwrap()
}

#[test]
fn native_form_isolates_headers_urls_and_ftp_credentials() {
    let fields = BTreeMap::from([
        ("source", "https://example.com/file?token=secret".into()),
        ("directory", "/tmp".into()),
        ("connections", "16".into()),
        (
            "headers",
            "Accept: */*\nAuthorization: Bearer private".into(),
        ),
        ("cookie", "sid=private".into()),
    ]);
    let http = build_input(&fields, Default::default(), vec![], false, true, vec![]).unwrap();
    let metadata = serde_json::to_string(&http.kind).unwrap();
    assert!(!metadata.contains("secret"));
    assert!(!metadata.contains("private"));
    assert_eq!(http.credentials.headers.len(), 2);
    assert!(http.credentials.extra.contains_key(SECRET_HTTP_SOURCE_URL));
    let ftp = input("sftp://jane:p%40ss@example.com/archive.zip", "/tmp");
    assert_eq!(ftp.credentials.password.as_deref(), Some("p@ss"));
    assert_eq!(ftp.credentials.username.as_deref(), Some("jane"));
    assert!(!serde_json::to_string(&ftp.kind).unwrap().contains("p%40ss"));
}

#[test]
fn form_rejects_invalid_limits_headers_and_protocols() {
    for invalid in ["NaN", "inf", "-1", "0", "hello", "1e300"] {
        assert!(parse_limit(invalid).is_err(), "{invalid}");
    }
    assert_eq!(parse_limit("1.5").unwrap(), Some(1536));
    assert_eq!(parse_limit("").unwrap(), None);
    for source in [
        "file:///etc/passwd",
        "ftps://example.com/file",
        "https:///",
        "ftp://example.com/",
    ] {
        let fields = BTreeMap::from([
            ("source", source.into()),
            ("directory", "/tmp".into()),
            ("connections", "16".into()),
        ]);
        assert!(build_input(&fields, Default::default(), vec![], false, true, vec![]).is_err());
    }
    let fields = BTreeMap::from([
        ("source", "https://example.com/file".into()),
        ("directory", "../relative".into()),
        ("connections", "16".into()),
    ]);
    assert!(build_input(&fields, Default::default(), vec![], false, true, vec![]).is_err());
}

fn task(state: TaskState) -> TaskSummary {
    TaskSummary {
        id: uuid::Uuid::new_v4(),
        kind: DownloadKind::Http,
        file_name: Some("sample.zip".into()),
        state,
        total_bytes: Some(1000),
        downloaded_bytes: 100,
        uploaded_bytes: 0,
        error: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn event_reducer_deduplicates_and_clears_stale_speeds() {
    let t = task(TaskState::Downloading);
    let id = t.id;
    let mut store = crate::model::TaskStore::default();
    store.apply(&CoreEvent::TaskCreated(t.clone()));
    store.apply(&CoreEvent::TaskCreated(t));
    assert_eq!(store.tasks.len(), 1);
    store.apply(&CoreEvent::TaskSpeed(TaskSpeed {
        task_id: id,
        download_bytes_per_second: 25,
        upload_bytes_per_second: 0,
    }));
    assert_eq!(store.speeds[&id].0, 25);
    store.apply(&CoreEvent::TaskStateChanged {
        task_id: id,
        state: TaskState::Paused,
    });
    assert!(!store.speeds.contains_key(&id));
    store.apply(&CoreEvent::TaskSpeed(TaskSpeed {
        task_id: id,
        download_bytes_per_second: 25,
        upload_bytes_per_second: 0,
    }));
    assert!(!store.speeds.contains_key(&id));
    store.apply(&CoreEvent::TaskCompleted {
        task_id: id,
        file_path: "/tmp/sample.zip".into(),
    });
    assert_eq!(store.tasks[0].downloaded_bytes, 1000);
}

#[test]
fn trash_never_leaks_into_normal_filters_and_progress_is_bounded() {
    let t = task(TaskState::Failed);
    let trash = [t.id].into_iter().collect();
    for filter in [
        Filter::All,
        Filter::Failed,
        Filter::Active,
        Filter::Completed,
        Filter::Paused,
    ] {
        assert!(!filter.matches(&t, &trash));
    }
    assert!(Filter::Trash.matches(&t, &trash));
    assert_eq!(percent(200, Some(100)), 1.);
    assert_eq!(percent(1, None), 0.);
    assert_eq!(eta(150, Some(100), 1), "0s");
    assert_eq!(eta(0, Some(100), 0), "—");
}

#[test]
fn signed_updater_rejects_bad_signatures_and_archive_links() {
    assert!(crate::updater::verify(b"payload", "invalid").is_err());
    let mut compressed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    {
        let mut tar = tar::Builder::new(&mut compressed);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        tar.append_link(&mut header, "Fluxion.app/escape", "/tmp")
            .unwrap();
        tar.finish().unwrap();
    }
    let data = compressed.finish().unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(crate::updater::extract(&data, dir.path()).is_err());
}

#[test]
fn native_backend_downloads_and_persists_real_http_file() {
    use std::io::{Read, Write};
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let payload = vec![42u8; 8192];
    let expected = payload.clone();
    let server = std::thread::spawn(move || {
        for stream in listener.incoming().take(3) {
            let mut stream = stream.unwrap();
            let mut buffer = [0u8; 8192];
            let size = stream.read(&mut buffer).unwrap();
            let head = buffer[..size].starts_with(b"HEAD");
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Disposition: attachment; filename=fixture.bin\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let _ = stream.write_all(header.as_bytes());
            if !head {
                let _ = stream.write_all(&payload);
            }
        }
    });
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let backend = backend::Backend::start(&runtime, data.path().into());
    runtime.block_on(async {
        async fn next(backend: &backend::Backend) -> Message {
            let message = tokio::time::timeout(Duration::from_secs(15), backend.messages.recv())
                .await
                .expect("backend response timeout")
                .unwrap();
            if let Message::Error(error) = &message {
                panic!("backend failed: {error}");
            }
            message
        }
        while !matches!(next(&backend).await, Message::Snapshot(..)) {}
        backend
            .commands
            .send(Command::Create(input(
                &format!("http://127.0.0.1:{port}/fixture.bin"),
                files.path().to_str().unwrap(),
            )))
            .await
            .unwrap();
        let id = loop {
            if let Message::Created(id) = next(&backend).await {
                break id;
            }
        };
        backend
            .commands
            .send(Command::Act(vec![id], Action::Start))
            .await
            .unwrap();
        loop {
            if let Message::Event(CoreEvent::TaskCompleted { task_id, .. }) = next(&backend).await {
                assert_eq!(task_id, id);
                break;
            }
        }
        assert_eq!(
            std::fs::read(files.path().join("fixture.bin")).unwrap(),
            expected
        );
        backend.commands.send(Command::Detail(id)).await.unwrap();
        loop {
            if let Message::Detail(_, Some(detail)) = next(&backend).await {
                assert_eq!(detail.task.state, TaskState::Completed);
                assert!(
                    backend::output_path(&detail)
                        .unwrap()
                        .starts_with(files.path().canonicalize().unwrap())
                );
                break;
            }
        }
        backend.commands.close();
        tokio::time::timeout(Duration::from_secs(5), backend.stopped.recv())
            .await
            .unwrap()
            .unwrap();
    });
    // The HTTP probe may use two or three connections. Dropping the listener thread is safe after transfer.
    drop(server);
    let restored = runtime
        .block_on(fluxion_runtime::build_core(data.path().into()))
        .unwrap();
    let tasks = runtime
        .block_on(restored.list_tasks(Default::default()))
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].state, TaskState::Completed);
}

#[gpui::test]
fn native_forms_render_and_accept_unicode(cx: &mut gpui::TestAppContext) {
    use gpui::*;
    use gpui_component::Root;
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::apply_theme("dark", None, cx);
    });
    for kind in [
        crate::form::FormKind::Create,
        crate::form::FormKind::Settings(Default::default()),
        crate::form::FormKind::Limits(uuid::Uuid::new_v4()),
    ] {
        let mut form = None;
        let window = cx.add_window(|window, cx| {
            let view = cx.new(|cx| crate::form::Form::new(kind, "zh-CN".into(), window, cx));
            form = Some(view.clone());
            Root::new(view, window, cx)
        });
        cx.run_until_parked();
        let form = form.unwrap();
        window
            .update(cx, |_, window, cx| {
                form.update(cx, |form, cx| form.focus(window, cx))
            })
            .unwrap();
        let creating = form.read_with(cx, |form, _| {
            matches!(form.kind, crate::form::FormKind::Create)
        });
        cx.simulate_input(
            *window,
            if creating {
                "https://example.com/中文资料.zip"
            } else {
                "1024"
            },
        );
        cx.run_until_parked();
        form.update(cx, |form, cx| {
            assert!(if creating {
                form.inputs["source"].read(cx).value().contains("中文资料")
            } else {
                form.inputs["down"].read(cx).value().contains("1024")
            })
        });
    }
}

#[test]
fn opening_a_completed_file_rejects_symlink_escape() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("outside.bin"), b"sample").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("outside.bin"),
        directory.path().join("link.bin"),
    )
    .unwrap();
    let task = DownloadTask {
        id: uuid::Uuid::new_v4(),
        kind: input("https://example.com/file", "/tmp").kind,
        save_dir: directory.path().into(),
        file_name: Some("link.bin".into()),
        state: TaskState::Completed,
        limits: Default::default(),
        proxy: Default::default(),
        total_bytes: Some(6),
        downloaded_bytes: 6,
        uploaded_bytes: 0,
        error: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
    };
    assert!(
        backend::output_path(&TaskDetail {
            task,
            credentials: Default::default()
        })
        .is_err()
    );
}

#[gpui::test]
fn editing_browser_source_clears_inherited_credentials(cx: &mut gpui::TestAppContext) {
    use gpui::*;
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::apply_theme("light", None, cx);
    });
    let mut form = None;
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|cx| {
            crate::form::Form::new(crate::form::FormKind::Create, "zh-CN".into(), window, cx)
        });
        form = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    let form = form.unwrap();
    window
        .update(cx, |_, window, cx| {
            form.update(cx, |form, cx| {
                form.inherit_browser(
                    fluxion_browser::Download {
                        url: "https://example.test/file?opaque=private".into(),
                        filename: Some("file.bin".into()),
                        headers: vec![fluxion_browser::Header {
                            name: "cookie".into(),
                            value: "sid=private".into(),
                        }],
                    },
                    window,
                    cx,
                );
            })
        })
        .unwrap();
    cx.run_until_parked();
    form.update(cx, |form, cx| {
        assert_eq!(
            form.inputs["cookie"].read(cx).value().as_str(),
            "sid=private"
        )
    });
    window
        .update(cx, |_, window, cx| {
            form.update(cx, |form, cx| {
                form.set("source", "https://other.test/file", window, cx)
            })
        })
        .unwrap();
    cx.run_until_parked();
    form.update(cx, |form, cx| {
        assert!(form.inputs["cookie"].read(cx).value().is_empty());
        let Command::Create(input) = form.command(cx).unwrap() else {
            panic!("expected Create");
        };
        assert!(input.credentials.headers.is_empty());
        assert!(!input.credentials.extra.contains_key(SECRET_HTTP_SOURCE_URL));
    });
}
