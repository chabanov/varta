use super::*;

fn files(text: &[u8]) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        (
            INDEX.into(),
            b"<script data-varta-dev src=\"./varta-host.js\" integrity=\"sha256-pinned\"></script>"
                .to_vec(),
        ),
        (LOADER.into(), text.to_vec()),
        ("app.wasm".into(), b"wasm".to_vec()),
    ])
}
fn revision(text: &[u8]) -> Revision {
    Revision::new(files(text)).unwrap()
}

#[test]
fn dev_revisions_pin_page_loader_and_all_asset_routes() {
    let revision = revision(b"first");
    assert_eq!(revision.id.len(), 64);
    let prefix = revision.prefix();
    let delivery = Delivery::new(revision);
    let html = std::str::from_utf8(&delivery.file("/").unwrap().bytes).unwrap();
    assert!(html.contains(&format!("src=\"{prefix}{LOADER}\"")));
    assert!(html.contains("integrity=\"sha256-pinned\""));
    for name in [INDEX, LOADER, "app.wasm"] {
        assert!(delivery.file(&format!("{prefix}{name}")).is_some());
    }
    assert_eq!(
        delivery.file(&prefix).unwrap().bytes,
        delivery.file("/").unwrap().bytes
    );
    for path in [
        "/app.wasm",
        "/varta-host.js",
        "/main.ts",
        "/__varta/",
        "/__varta/nope/app.wasm",
        "//index.html",
    ] {
        assert!(delivery.file(path).is_none(), "{path}");
    }
    for suffix in [
        "../app.wasm",
        "%61pp.wasm",
        "./app.wasm",
        "/app.wasm",
        "main.ts",
    ] {
        assert!(
            delivery.file(&format!("{prefix}{suffix}")).is_none(),
            "{suffix}"
        );
    }
}

#[test]
fn dev_revisions_hash_every_input_with_framed_names_and_lengths() {
    let first = revision(b"first");
    assert_eq!(first.id, revision(b"first").id);
    for name in [INDEX, LOADER, "app.wasm"] {
        let mut changed = files(b"first");
        changed.get_mut(name).unwrap().extend_from_slice(b" ");
        assert_ne!(first.id, Revision::new(changed).unwrap().id);
    }
    let mut left = files(b"first");
    left.insert("a".into(), b"bc".to_vec());
    let mut right = files(b"first");
    right.insert("ab".into(), b"c".to_vec());
    assert_ne!(
        Revision::new(left).unwrap().id,
        Revision::new(right).unwrap().id
    );
}

#[test]
fn dev_revisions_publish_atomically_retain_two_and_never_substitute_expired_assets() {
    let first = revision(b"first");
    let old_path = format!("{}{LOADER}", first.prefix());
    let mut delivery = Delivery::new(first);
    let old_bytes = delivery.file(&old_path).unwrap().bytes.clone();
    assert!(!delivery.publish(revision(b"first")));
    assert_eq!(delivery.revisions.len(), 1);
    assert!(delivery.publish(revision(b"second")));
    assert_eq!(delivery.file(&old_path).unwrap().bytes, old_bytes);
    assert_eq!(
        delivery
            .file(&format!("{}{LOADER}", delivery.latest().prefix()))
            .unwrap()
            .bytes,
        b"second".as_slice()
    );
    assert!(delivery.publish(revision(b"third")));
    assert_eq!(delivery.revisions.len(), RETAINED_REVISIONS);
    assert!(delivery.file(&old_path).is_none());
    assert_eq!(
        old_bytes,
        b"first".as_slice(),
        "in-flight owned bytes remain immutable after eviction"
    );
}

#[test]
fn dev_revisions_revert_without_duplicate_retention_and_reject_format_drift() {
    let first = revision(b"first");
    let id = first.id.clone();
    let mut delivery = Delivery::new(first);
    delivery.publish(revision(b"second"));
    delivery.publish(revision(b"first"));
    assert_eq!(delivery.latest().id, id);
    assert_eq!(delivery.revisions.len(), 2);
    for html in [
        "missing",
        "<script data-varta-dev src=\"./varta-host.js\"></script><script data-varta-dev src=\"./varta-host.js\"></script>",
    ] {
        let mut malformed = files(b"first");
        malformed.insert(INDEX.into(), html.as_bytes().to_vec());
        assert!(Revision::new(malformed).is_err());
        assert_eq!(delivery.latest().id, id);
    }
}

#[test]
fn dev_rebuild_diagnostics_are_bounded_and_success_clears_them_without_reloading_identical_bytes() {
    let mut delivery = Delivery::new(revision(b"first"));
    let id = delivery.latest().id.clone();
    delivery.complete(Err(ProjectError::Usage("🦀".repeat(20000))));
    let status: serde_json::Value = serde_json::from_slice(&delivery.status().bytes).unwrap();
    assert_eq!(status["revision"], id);
    assert!(status["error"].as_str().unwrap().len() <= 16 * 1024);
    delivery.complete(Ok(revision(b"first")));
    let status: serde_json::Value = serde_json::from_slice(&delivery.status().bytes).unwrap();
    assert_eq!(status["revision"], id);
    assert!(status["error"].is_null());
    assert_eq!(delivery.revisions.len(), 1);
}
