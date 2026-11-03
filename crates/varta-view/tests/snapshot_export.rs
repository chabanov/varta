use varta_common::view::{
    Attribute as A, DRAFT_MAX_OPENED_URL_BYTES as BYTES, DRAFT_MAX_OPENED_URLS as COUNT, Tag,
};
use varta_view::{
    Access, BatchWriter as B, Broker, Environment, Input, Platform, SnapshotError, SnapshotFormat,
    Target, View,
};
#[derive(Default)]
struct Granted {
    calls: usize,
}
impl Broker for Granted {
    fn link_requested(&self, _: &str) -> bool {
        true
    }
    fn open_url(&mut self, _: &str) -> Access {
        self.calls += 1;
        Access::Granted
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}
fn commit(view: &mut View, b: B, broker: &mut Granted) {
    view.commit(&b.finish(), false, broker).unwrap();
}
fn open(view: &mut View, broker: &mut Granted, url: &str) {
    commit(
        view,
        B::new().set_attribute(2, A::Href, url).clone(),
        broker,
    );
    view.input(
        &Input::Click {
            target: Target::Node(2),
        },
        broker,
    )
    .unwrap();
    let deliveries = view.drain();
    assert!(deliveries.iter().any(|d| d.record.key == "open_url"));
}
fn link(platform: Platform) -> (View, Granted) {
    let mut view = View::new(Environment::default(), platform);
    let mut broker = Granted::default();
    commit(
        &mut view,
        B::new()
            .create_element(2, Tag::A)
            .set_attribute(2, A::Href, "https://example.test/")
            .insert(1, 2, 0)
            .clone(),
        &mut broker,
    );
    (view, broker)
}
#[test]
fn opened_url_retention_does_not_limit_actions_and_keeps_the_newest_complete_urls() {
    let (mut view, mut broker) = link(Platform::default());
    assert!(view.snapshot().get("openedDropped").is_none());
    for i in 0..COUNT + 7 {
        open(&mut view, &mut broker, &format!("https://example.test/{i}"));
    }
    assert_eq!(broker.calls, COUNT + 7);
    let snapshot = view.snapshot();
    let urls = snapshot["opened"].as_array().unwrap();
    assert_eq!(urls.len(), COUNT);
    assert_eq!(snapshot["openedDropped"], 7);
    assert_eq!(urls.first().unwrap(), "https://example.test/7");
    assert_eq!(
        urls.last().unwrap(),
        &format!("https://example.test/{}", COUNT + 6)
    );
    let before = view.snapshot();
    assert!(
        view.commit(
            &B::new()
                .set_attribute(2, A::Href, "https://example.test/rollback")
                .set_text(999, "invalid")
                .finish(),
            false,
            &mut broker
        )
        .is_err()
    );
    assert_eq!(view.snapshot(), before);
    open(&mut view, &mut broker, "https://example.test/next");
    assert_eq!(view.snapshot()["openedDropped"], 8);
}
#[test]
fn opened_url_bytes_count_utf8_and_platform_refusals_do_not_enter_the_history() {
    let (mut view, mut broker) = link(Platform::default());
    let url = |i| format!("https://example.test/{i:03}/{}", "界".repeat(16000));
    for i in 0..40 {
        open(&mut view, &mut broker, &url(i));
    }
    let snapshot = view.snapshot();
    let urls = snapshot["opened"].as_array().unwrap();
    let count = BYTES / url(0).len();
    assert!(count < COUNT);
    assert_eq!(urls.len(), count);
    assert_eq!(snapshot["openedDropped"], 40 - count);
    assert_eq!(urls.first().unwrap(), &url(40 - count));
    assert_eq!(urls.last().unwrap(), &url(39));
    assert!(
        urls.iter()
            .map(|u| u.as_str().unwrap().len())
            .sum::<usize>()
            <= BYTES
    );
    let (mut refused, mut broker) = link(Platform {
        browser_fails: true,
        ..Platform::default()
    });
    for _ in 0..COUNT + 3 {
        open(&mut refused, &mut broker, "https://example.test/refused");
    }
    assert!(refused.snapshot()["opened"].as_array().unwrap().is_empty());
    assert!(refused.snapshot().get("openedDropped").is_none());
}
#[test]
fn pretty_expansion_is_refused_without_changing_the_view_and_compact_retry_succeeds() {
    let mut view = View::default();
    let mut broker = Granted::default();
    let mut b = B::new();
    for id in 2..502 {
        b.create_element(id, Tag::Span).insert(id - 1, id, 0);
    }
    commit(&mut view, b, &mut broker);
    let mut b = B::new();
    for id in 1000..4000 {
        b.create_text(id, "x").insert(501, id, 0);
        if (id - 999) % 64 == 0 {
            commit(&mut view, b, &mut broker);
            b = B::new();
        }
    }
    commit(&mut view, b, &mut broker);
    let commits = view.commits();
    assert!(matches!(
        view.snapshot_json(SnapshotFormat::Pretty),
        Err(SnapshotError::Limit { .. })
    ));
    let compact = view.snapshot_json(SnapshotFormat::Compact).unwrap();
    assert_eq!(compact, serde_json::to_vec(&view.snapshot()).unwrap());
    assert_eq!(view.commits(), commits);
    commit(
        &mut view,
        B::new().remove(2).set_title("recovered").clone(),
        &mut broker,
    );
    let bytes = view.snapshot_json(SnapshotFormat::Pretty).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["title"],
        "recovered"
    );
}
