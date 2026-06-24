use chrono::Utc;
use fluxion_core::{
    DownloadTask, HeaderPair, HttpMethod, HttpTaskConfig, IpFilterConfig, IpFilterDecision, IpRule,
    ProxyConfig, ProxyPolicy, TaskCredentials, TaskDetail, TaskKind, TaskRateLimit, TaskState,
    apply_ip_filter,
};
use url::Url;
use uuid::Uuid;

#[test]
fn ip_filter_allow_rules_take_precedence() {
    let config = IpFilterConfig {
        allow: vec![IpRule {
            cidr: "10.1.2.3".to_string(),
        }],
        deny: vec![IpRule {
            cidr: "10.0.0.0/8".to_string(),
        }],
    };
    assert_eq!(
        apply_ip_filter(&config, "10.1.2.3".parse().unwrap()),
        IpFilterDecision::Allow
    );
}

#[test]
fn task_detail_redaction_removes_secrets_from_public_output() {
    let detail = TaskDetail {
        task: DownloadTask {
            id: Uuid::new_v4(),
            kind: TaskKind::Http(HttpTaskConfig {
                url: Url::parse("https://example.com/file.bin?token=secret&name=public").unwrap(),
                method: HttpMethod::Get,
                headers: vec![
                    HeaderPair {
                        name: "Authorization".to_string(),
                        value: "Bearer secret".to_string(),
                    },
                    HeaderPair {
                        name: "User-Agent".to_string(),
                        value: "Fluxion".to_string(),
                    },
                ],
                max_connections: Some(16),
                min_split_size: None,
                redirect_limit: 10,
            }),
            save_dir: "/tmp".into(),
            file_name: Some("file.bin".to_string()),
            state: TaskState::Queued,
            limits: TaskRateLimit::default(),
            proxy: ProxyPolicy::Custom(ProxyConfig {
                url: Url::parse("http://user:password@proxy.local:8080?api_key=secret").unwrap(),
                username: Some("user".to_string()),
            }),
            total_bytes: None,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        },
        credentials: TaskCredentials {
            password: Some("secret".to_string()),
            headers: vec![HeaderPair {
                name: "Cookie".to_string(),
                value: "sid=secret".to_string(),
            }],
            ..Default::default()
        },
    };

    let redacted = detail.redacted();
    let serialized = serde_json::to_string(&redacted).unwrap();

    assert!(!serialized.contains("Bearer secret"));
    assert!(!serialized.contains("sid=secret"));
    assert!(!serialized.contains("password@proxy"));
    assert!(!serialized.contains("token=secret"));
    assert!(!serialized.contains("api_key=secret"));
    assert!(serialized.contains("User-Agent"));
    assert!(serialized.contains("Fluxion"));
    assert!(serialized.contains("name=public"));
}

#[test]
fn http_segment_is_complete_includes_last_byte() {
    let segment = fluxion_core::HttpSegment {
        index: 0,
        start_byte: 10,
        end_byte: 10,
        downloaded_bytes: 1,
        state: fluxion_core::HttpSegmentState::Completed,
        retry_count: 0,
        last_error: None,
    };

    assert!(segment.is_complete());
}
