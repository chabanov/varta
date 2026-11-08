use super::*;
#[test]
fn protected_session_origin_and_channel_checks_reject_credential_authority_confusion() {
    for value in [
        "https://api.example.test",
        "https://api.example.test:8443",
        "https://[::1]:8443",
    ] {
        assert!(origin(value, false), "{value}");
    }
    for value in [
        "http://api.example.test",
        "https://api.example.test/",
        "https://API.example.test",
        "https://api.example.test:443",
        "https://api.example.test:",
        "https://api.example.test:0",
        "https://user@api.example.test",
        "https://api.example.test.evil/",
        "https://*.example.test",
        "https://api.example.test#x",
        "https://[::1]junk]",
        "https://api..test",
        "https://api.example.test\n",
    ] {
        assert!(!origin(value, false), "{value}");
    }
    assert!(origin("http://127.0.0.1:8787", true));
    assert!(!origin("http://127.0.0.1:8787", false));
    assert!(!origin("http://evil.test:8787", true));
    let api = "https://api.example.test";
    let client = "https://studio.example.test";
    assert!(channel(
        api,
        client,
        client,
        "https://api.example.test/api/me",
        "GET",
        false
    ));
    for (page, url, method) in [
        (
            "https://preview.example.test",
            "https://api.example.test/api/me",
            "GET",
        ),
        (client, "https://api.example.test.evil/api/me", "GET"),
        (client, "https://api.example.test:444/api/me", "GET"),
        (client, "https://api.example.test@evil.test/api/me", "GET"),
        (client, "https://api.example.test/api/me#x", "GET"),
        (client, "https://api.example.test/api/me", "TRACE"),
    ] {
        assert!(!channel(api, client, page, url, method, false));
    }
    for name in [
        "Authorization",
        "Cookie",
        "Origin",
        "X-Varta-Session",
        "Sec-Fetch-Site",
        "Proxy-Authorization",
        "Host",
    ] {
        assert!(!request_header(name));
    }
    assert!(request_header("Idempotency-Key"));
}
#[test]
fn protected_session_policy_requires_https_and_bounded_explicit_lifetimes() {
    let mut policy = ServerPolicy {
        origin: "https://api.test".into(),
        client_origin: "https://studio.test".into(),
        max_age_seconds: 3600,
        idle_seconds: 900,
        max_sessions: 2,
        same_site: CookieSite::Strict,
        allow_insecure_loopback: false,
    };
    policy.validate().unwrap();
    policy.max_sessions = 0;
    assert!(policy.validate().is_err());
    policy.max_sessions = 2;
    policy.idle_seconds = 3601;
    assert!(policy.validate().is_err());
    policy.idle_seconds = 900;
    policy.origin = "http://127.0.0.1:8787".into();
    assert!(policy.validate().is_err());
    policy.allow_insecure_loopback = true;
    policy.validate().unwrap();
    policy.same_site = CookieSite::None;
    assert!(policy.validate().is_err());
    let cookie = cookie("opaque", 3600, true, "Strict");
    assert!(cookie.contains("HttpOnly; Secure; SameSite=Strict"));
    assert!(!cookie.contains("Domain="));
}
#[test]
fn protected_session_grants_are_separate_and_negotiate_the_new_abi() {
    use crate::{CapabilityGrant, CapabilityValue, Manifest, RequestScope};
    let grants = [
        CapabilityGrant {
            id: READ_CAPABILITY.into(),
            value: CapabilityValue::Boolean(true),
        },
        CapabilityGrant {
            id: CLIENT_CAPABILITY.into(),
            value: CapabilityValue::Requests(RequestScope {
                urls: vec!["https://api.test/api/**".into()],
                methods: Some(vec!["GET".into()]),
                response_headers: None,
            }),
        },
    ];
    let mut manifest = Manifest::new(&grants);
    assert!(manifest.validate_package().is_err());
    manifest.abi_version = crate::abi::PROTECTED_SESSION_VERSION;
    manifest.validate_package().unwrap();
    let mut unknown = Manifest::new(&[]);
    unknown.capabilities.push(crate::manifest::WireGrant {
        id: "session.read.extra".into(),
        enabled: Some(true),
        patterns: None,
        methods: None,
        response_headers: None,
        required: None,
        purpose: None,
    });
    assert!(
        matches!(unknown.grants(),Err(crate::VartaError::InvalidManifest(message))
        if message=="invalid grant shape for session.read.extra")
    );
    assert_eq!(
        crate::HostOperation::HttpSessionRead.capability(),
        Some(READ_CAPABILITY)
    );
    assert_eq!(
        crate::HostOperation::HttpSessionWrite.capability(),
        Some(WRITE_CAPABILITY)
    );
    assert_eq!(
        crate::HostOperation::HttpSessionClear.capability(),
        Some(CLEAR_CAPABILITY)
    );
}
