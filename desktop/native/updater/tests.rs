use super::*;

fn release(version: &str) -> Release {
    let channel = if version.contains('-') {
        Channel::Beta
    } else {
        Channel::Stable
    };
    Release {
        version: version.into(),
        channel: Some(channel),
        notes: String::new(),
        platforms: [(
            target().into(),
            Artifact {
                url: format!(
                    "{BASE}/releases/{}/{version}/Fluxion.app.tar.gz",
                    channel.as_str()
                ),
                signature: "test signature".into(),
            },
        )]
        .into_iter()
        .collect(),
    }
}
#[test]
fn stable_never_accepts_a_beta_or_mismatched_manifest() {
    assert!(newer(&release("1.1.0-beta.1"), "1.0.0", Channel::Stable).is_err());
    let mut wrong = release("1.1.0");
    wrong.channel = Some(Channel::Beta);
    assert!(newer(&wrong, "1.0.0", Channel::Beta).is_err());
    assert!(newer(&release("1.1.0-rc.1"), "1.0.0", Channel::Beta).is_err());
}
#[test]
fn beta_tracks_preview_then_final_stable_without_downgrades() {
    let chosen = select_release(
        "1.1.0-beta.2",
        Channel::Beta,
        [release("1.1.0-beta.3"), release("1.1.0")].into_iter(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(chosen.version, "1.1.0");
    assert!(
        select_release(
            "1.2.0-beta.1",
            Channel::Stable,
            [release("1.1.0")].into_iter()
        )
        .unwrap()
        .is_none()
    );
    assert!(!newer(&release("1.0.0"), "1.0.0", Channel::Stable).unwrap());
    assert!(!newer(&release("1.0.0"), "1.0.0+build.2", Channel::Stable).unwrap());
    assert!(newer(&release("1.0.0-beta.10"), "1.0.0-beta.9", Channel::Beta).unwrap());
}
#[test]
fn update_artifacts_are_bound_to_origin_channel_version_and_architecture() {
    for invalid in [
        "http://assets.fluxion.alkinum.io/releases/stable/1.1.0/Fluxion.app.tar.gz",
        "https://example.com/releases/stable/1.1.0/Fluxion.app.tar.gz",
        "https://assets.fluxion.alkinum.io:8443/releases/stable/1.1.0/Fluxion.app.tar.gz",
        "https://user@assets.fluxion.alkinum.io/releases/stable/1.1.0/Fluxion.app.tar.gz",
        "https://assets.fluxion.alkinum.io/releases/beta/1.1.0/Fluxion.app.tar.gz",
        "https://assets.fluxion.alkinum.io/releases/stable/0.9.0/Fluxion.app.tar.gz",
        "https://assets.fluxion.alkinum.io/releases/stable/1.1.0/Fluxion.app.tar.gz?token=bad",
    ] {
        let mut candidate = release("1.1.0");
        candidate.platforms.get_mut(target()).unwrap().url = invalid.into();
        assert!(artifact_url(&candidate).is_err(), "accepted {invalid}");
    }
    let mut candidate = release("1.1.0");
    candidate.platforms.clear();
    assert!(artifact_url(&candidate).is_err());
}
#[test]
fn old_preferences_get_safe_update_defaults_and_choices_persist() {
    let mut prefs: crate::model::Preferences =
        serde_json::from_str(r#"{"locale":"zh-CN"}"#).unwrap();
    assert!(prefs.auto_check_updates);
    assert_eq!(prefs.update_channel, Channel::default());
    prefs.update_channel = Channel::Beta;
    prefs.auto_check_updates = false;
    let stored = serde_json::to_string(&prefs).unwrap();
    let restored: crate::model::Preferences = serde_json::from_str(&stored).unwrap();
    assert_eq!(restored.update_channel, Channel::Beta);
    assert!(!restored.auto_check_updates);
}
fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zipped = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    {
        let mut tar = tar::Builder::new(&mut zipped);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, *data).unwrap();
        }
        tar.finish().unwrap();
    }
    zipped.finish().unwrap()
}
#[test]
fn extraction_accepts_one_bundle_and_rejects_other_roots() {
    let good = archive(&[
        ("Fluxion.app/Contents/MacOS/fluxion-app", b"binary"),
        ("Fluxion.app/Contents/Info.plist", b"plist"),
    ]);
    let temp = tempfile::tempdir().unwrap();
    assert!(extract(&good, temp.path()).is_ok());
    for path in ["Other.app/Contents/test", "install.sh"] {
        let temp = tempfile::tempdir().unwrap();
        assert!(extract(&archive(&[(path, b"bad")]), temp.path()).is_err());
    }
    assert!(extract(b"not an archive", temp.path()).is_err());
}
#[test]
fn staged_bundle_mutation_and_symlinks_are_detected() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("binary");
    std::fs::write(&file, b"original").unwrap();
    let original = bundle_digest(temp.path()).unwrap();
    std::fs::write(&file, b"tampered").unwrap();
    assert_ne!(original, bundle_digest(temp.path()).unwrap());
    std::os::unix::fs::symlink("binary", temp.path().join("link")).unwrap();
    assert!(bundle_digest(temp.path()).is_err());
}

#[test]
fn accepts_base64_wrapped_minisign_and_rejects_modified_payload() {
    // Public test vector from minisign-verify (ISC); no release/private key material.
    let key = "untrusted comment: test public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    let signature = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";
    let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
    assert!(verify_with_key(b"test", &encode(signature), &encode(key)).is_ok());
    assert!(verify_with_key(b"Test", &encode(signature), &encode(key)).is_err());
    assert!(
        verify(b"test", &encode(signature)).is_err(),
        "a different signing key must not be trusted"
    );
}
