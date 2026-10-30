mod files;
mod glob;
use files::FilePolicy;
use glob::{glob, wildcard};
use varta_common::{CapabilityGrant, CapabilityValue};

pub(crate) struct Policy {
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
    files: FilePolicy,
    /// Requested grants the v38 permission broker withholds (state `denied`
    /// or `prompt`): every check of their operation fails, as if the package
    /// had not requested them, until the broker grants them.
    withheld: std::collections::BTreeSet<String>,
}

impl Policy {
    pub(crate) fn new(grants: &[CapabilityGrant]) -> Self {
        Self::with_ceiling(grants, None)
    }

    pub(crate) fn with_ceiling(
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
    ) -> Self {
        Self {
            requested: requested.to_vec(),
            ceiling: ceiling.map(ToOwned::to_owned),
            files: FilePolicy::new(requested, ceiling.unwrap_or(requested)),
            withheld: std::collections::BTreeSet::new(),
        }
    }

    /// Withhold or release a requested grant (v38 broker states). Releasing
    /// never adds authority: the request and the ceiling still decide.
    pub(crate) fn withhold(&mut self, id: &str, withheld: bool) {
        if withheld {
            self.withheld.insert(id.to_owned());
        } else {
            self.withheld.remove(id);
        }
    }

    /// The request and the ceiling, ignoring broker states.
    fn permits(&self, permits: impl Fn(&[CapabilityGrant]) -> bool) -> bool {
        permits(&self.requested) && self.ceiling.as_deref().is_none_or(permits)
    }

    /// Whether the broker leaves operation `id` open.
    fn open(&self, id: &str) -> bool {
        !self.withheld.contains(id)
    }

    /// The package requests `id` with a non-denying filter, and so does the
    /// ceiling when there is one (the grant-level view of a broker state).
    pub(crate) fn requested_open(&self, id: &str) -> bool {
        self.permits(|grants| grants.iter().any(|g| g.id == id && grant_open(&g.value)))
    }

    /// The names that pattern grants of `id` list exactly (no `*` or `?`),
    /// in the request and, when there is one, also in the ceiling; empty
    /// while the broker withholds `id`. Wildcard patterns name nothing, so
    /// no name is ever enumerated from the resource behind them.
    pub(crate) fn exact_names(&self, id: &str) -> Vec<String> {
        fn exact(grants: &[CapabilityGrant], id: &str) -> std::collections::BTreeSet<String> {
            grants
                .iter()
                .filter(|grant| grant.id == id)
                .flat_map(|grant| match &grant.value {
                    CapabilityValue::PathGlobs(patterns) => patterns.clone(),
                    _ => Vec::new(),
                })
                .filter(|pattern| !pattern.contains(['*', '?']))
                .collect()
        }
        if !self.open(id) {
            return Vec::new();
        }
        let mut names = exact(&self.requested, id);
        if let Some(ceiling) = &self.ceiling {
            let allowed = exact(ceiling, id);
            names.retain(|name| allowed.contains(name));
        }
        names.into_iter().collect()
    }

    /// The exact requested patterns of `id` (what a prompt shows).
    pub(crate) fn requested_scope(&self, id: &str) -> Vec<String> {
        self.requested
            .iter()
            .filter(|g| g.id == id)
            .flat_map(|g| match &g.value {
                CapabilityValue::PathGlobs(p)
                | CapabilityValue::UrlGlobs(p)
                | CapabilityValue::ListenEndpoints(p) => p.clone(),
                CapabilityValue::Requests(scope) => scope.urls.clone(),
                CapabilityValue::Boolean(_) => Vec::new(),
            })
            .collect()
    }

    /// Whether the request and the ceiling cover `resource` of `id`, ignoring
    /// broker states. `None` when the descriptor is malformed: an unknown
    /// id, or a resource on a boolean operation.
    pub(crate) fn covers(&self, id: &str, resource: Option<&str>) -> Option<bool> {
        let kind = varta_common::capability_kind(id)?;
        let Some(resource) = resource else {
            return Some(self.requested_open(id));
        };
        Some(match kind {
            varta_common::GrantKind::Boolean => return None,
            varta_common::GrantKind::Patterns if id.starts_with("fs.") => {
                self.permits(|grants| grants.iter().any(|g| g.id == id))
                    && self.files.covers(id, resource)
            }
            varta_common::GrantKind::Patterns => self.env_scope(id, resource),
            varta_common::GrantKind::Urls => self.network_scope(id, resource).is_some(),
            varta_common::GrantKind::Requests => self.request_scope(resource, None).is_some(),
            varta_common::GrantKind::Listeners => self.listen_scope(resource),
            // V38 `ui.open_url`: the link matches a pattern of the request
            // and of the ceiling (the any-HTTPS form included).
            varta_common::GrantKind::OpenUrls => parse_link(resource)
                .is_some_and(|url| self.permits(|grants| link_match(grants, &url).is_some())),
        })
    }

    pub(crate) fn enabled(&self, id: &str) -> bool {
        self.open(id)
            && self.permits(|grants| {
                grants
                    .iter()
                    .any(|g| g.id == id && g.value == CapabilityValue::Boolean(true))
            })
    }

    pub(crate) fn env(&self, id: &str, name: &str) -> bool {
        self.open(id) && self.env_scope(id, name)
    }

    fn env_scope(&self, id: &str, name: &str) -> bool {
        !name.is_empty() && name.len() <= 256 && !name.contains(['=', '\0']) && self.permits(|grants| {
            grants.iter().any(|g| g.id == id && matches!(&g.value, CapabilityValue::PathGlobs(p) if p.iter().any(|p| glob(p, name, false))))
        })
    }

    pub(crate) fn network(&self, id: &str, input: &str) -> Option<url::Url> {
        if !self.open(id) {
            return None;
        }
        self.network_scope(id, input)
    }

    fn network_scope(&self, id: &str, input: &str) -> Option<url::Url> {
        let url = parse_network_url(input, &[HTTP])?;
        self.permits(|grants| {
            grants.iter().filter(|g| g.id == id).any(|g| {
                let CapabilityValue::UrlGlobs(patterns) = &g.value else {
                    return false;
                };
                patterns.iter().any(|pattern| url_matches(pattern, &url))
            })
        })
        .then_some(url)
    }

    /// `net.request` (v37): the URL must parse under the network rules (HTTP
    /// or HTTPS, no credentials, fragment or encoded path) and match a URL
    /// pattern and the method list of one grant in the package's requests and
    /// in the host ceiling. Nothing is resolved or connected here.
    pub(crate) fn request(&self, input: &str, method: &str) -> Option<url::Url> {
        if !self.open(varta_common::net::REQUEST_CAPABILITY) {
            return None;
        }
        self.request_scope(input, Some(method))
    }

    /// The URL scope of `net.request`; without a method, any granted method
    /// (a nonempty method list or none) qualifies.
    fn request_scope(&self, input: &str, method: Option<&str>) -> Option<url::Url> {
        let url = parse_network_url(input, &[HTTP, HTTPS])?;
        self.permits(|grants| {
            grants
                .iter()
                .filter(|g| g.id == varta_common::net::REQUEST_CAPABILITY)
                .any(|g| {
                    let CapabilityValue::Requests(scope) = &g.value else {
                        return false;
                    };
                    scope.methods.as_ref().is_none_or(|methods| match method {
                        Some(method) => methods.iter().any(|m| m == method),
                        None => !methods.is_empty(),
                    }) && scope.urls.iter().any(|pattern| url_matches(pattern, &url))
                })
        })
        .then_some(url)
    }

    /// `ui.open_url` (v38), patch time: an external `href` must match a
    /// pattern of the package request (the any-HTTPS form included).
    pub(crate) fn link_requested(&self, input: &str) -> bool {
        parse_link(input).is_some_and(|url| link_match(&self.requested, &url).is_some())
    }

    /// `ui.open_url` (v38), activation time: granted when an explicit
    /// pattern of the request and of the ceiling matches; a prompt on every
    /// activation when either side matches only through the any-HTTPS form;
    /// denied otherwise.
    pub(crate) fn open_url(&self, input: &str) -> varta_view::Access {
        // A withheld grant (a v38 broker state) denies like a missing one.
        if !self.open(varta_common::view::OPEN_URL_CAPABILITY) {
            return varta_view::Access::Denied;
        }
        let Some(url) = parse_link(input) else {
            return varta_view::Access::Denied;
        };
        let requested = link_match(&self.requested, &url);
        let ceiling = match &self.ceiling {
            Some(ceiling) => link_match(ceiling, &url),
            None => requested,
        };
        match (requested, ceiling) {
            (Some(LinkMatch::Explicit), Some(LinkMatch::Explicit)) => varta_view::Access::Granted,
            (Some(_), Some(_)) => varta_view::Access::Prompt,
            _ => varta_view::Access::Denied,
        }
    }

    pub(crate) fn listen(&self, endpoint: &str) -> bool {
        self.open("net.listen") && self.listen_scope(endpoint)
    }

    fn listen_scope(&self, endpoint: &str) -> bool {
        self.permits(|grants| grants.iter().any(|g| g.id == "net.listen"
            && matches!(&g.value, CapabilityValue::ListenEndpoints(p) if p.iter().any(|p| p == endpoint))))
    }

    pub(crate) fn read(&self, id: &str, input: &str) -> Result<Vec<u8>, crate::HostFailure> {
        if !self.open(id) {
            return Err(crate::HostFailure::denied(id, input));
        }
        self.files.read(id, input)
    }
}

/// Whether a grant value permits anything at all.
fn grant_open(value: &CapabilityValue) -> bool {
    match value {
        CapabilityValue::Boolean(enabled) => *enabled,
        CapabilityValue::PathGlobs(patterns)
        | CapabilityValue::UrlGlobs(patterns)
        | CapabilityValue::ListenEndpoints(patterns) => !patterns.is_empty(),
        CapabilityValue::Requests(scope) => {
            !scope.urls.is_empty() && scope.methods.as_ref().is_none_or(|m| !m.is_empty())
        }
    }
}

const HTTP: &str = "http";
const HTTPS: &str = "https";

/// Parse a guest URL for authorization: bounded, no control characters or
/// whitespace, one of `schemes`, no credentials, fragment or encoded path.
/// The same parsed URL is what the transport uses.
fn parse_network_url(input: &str, schemes: &[&str]) -> Option<url::Url> {
    if input.len() > varta_common::net::MAX_URL_BYTES
        || input
            .chars()
            .any(|c| c.is_ascii_control() || c.is_whitespace())
    {
        return None;
    }
    let url = url::Url::parse(input).ok()?;
    if url.path().contains('%')
        || !schemes.contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    Some(url)
}

/// How a `ui.open_url` grant matched a link.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkMatch {
    Explicit,
    AnyHttps,
}

/// The best `ui.open_url` match among `grants`: an explicit pattern wins over
/// the any-HTTPS form.
fn link_match(grants: &[CapabilityGrant], url: &url::Url) -> Option<LinkMatch> {
    let patterns = grants
        .iter()
        .filter(|g| g.id == varta_common::view::OPEN_URL_CAPABILITY)
        .filter_map(|g| match &g.value {
            CapabilityValue::UrlGlobs(patterns) => Some(patterns),
            _ => None,
        })
        .flatten();
    let mut found = None;
    for pattern in patterns {
        if pattern == varta_common::view::ANY_HTTPS_PATTERN {
            if url.scheme() == HTTPS {
                found = found.or(Some(LinkMatch::AnyHttps));
            }
        } else if url_matches(pattern, url) {
            return Some(LinkMatch::Explicit);
        }
    }
    found
}

/// Parse an external link: bounded, no control characters, whitespace or
/// `\`, `https`/`http` with a host and no credentials; a fragment is allowed.
fn parse_link(input: &str) -> Option<url::Url> {
    if input.len() > varta_common::view::DRAFT_MAX_VALUE_BYTES
        || input
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return None;
    }
    let url = url::Url::parse(input).ok()?;
    (matches!(url.scheme(), HTTP | HTTPS)
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

/// Exact scheme, host and effective port; wildcards only after the authority.
fn url_matches(pattern: &str, url: &url::Url) -> bool {
    let Ok(allow) = url::Url::parse(pattern) else {
        return false;
    };
    allow.scheme() == url.scheme()
        && allow.host_str() == url.host_str()
        && allow.port_or_known_default() == url.port_or_known_default()
        && wildcard(
            &allow[url::Position::BeforePath..],
            &url[url::Position::BeforePath..],
            false,
            false,
        )
}

#[cfg(test)]
mod request_tests {
    use super::*;
    use varta_common::RequestScope;

    fn grant(urls: &[&str], methods: Option<&[&str]>) -> CapabilityGrant {
        CapabilityGrant {
            id: varta_common::net::REQUEST_CAPABILITY.into(),
            value: CapabilityValue::Requests(RequestScope {
                urls: urls.iter().map(|u| (*u).into()).collect(),
                methods: methods.map(|m| m.iter().map(|m| (*m).into()).collect()),
            }),
        }
    }

    #[test]
    fn requests_match_scheme_host_port_path_and_method_exactly() {
        let policy = Policy::new(&[grant(
            &["https://api.test/v1/**", "http://127.0.0.1:8080/**"],
            Some(&["GET", "POST"]),
        )]);
        for (url, method) in [
            ("https://api.test/v1/items?x=1", "GET"),
            ("https://API.test:443/v1/items", "POST"),
            ("http://127.0.0.1:8080/", "GET"),
        ] {
            assert!(policy.request(url, method).is_some(), "{method} {url}");
        }
        for (url, method) in [
            ("https://api.test/v1/items", "DELETE"),
            ("https://api.test/v1/items", "get"),
            ("http://api.test/v1/items", "GET"),
            ("https://api.test:8443/v1/items", "GET"),
            ("https://api.test/v2/items", "GET"),
            ("https://api.test/v1/../admin", "GET"),
            ("https://api.test/v1/%2e%2e/admin", "GET"),
            ("https://user:pw@api.test/v1/x", "GET"),
            ("https://api.test/v1/x#frag", "GET"),
            ("https://api.test.evil/v1/x", "GET"),
            ("http://127.0.0.1:8081/", "GET"),
            ("ftp://api.test/v1/x", "GET"),
            ("/v1/relative", "GET"),
        ] {
            assert!(policy.request(url, method).is_none(), "{method} {url}");
        }
    }

    #[test]
    fn requests_are_distinct_from_earlier_network_operations() {
        let fetch = CapabilityGrant {
            id: "net.fetch".into(),
            value: CapabilityValue::UrlGlobs(vec!["http://127.0.0.1:8080/**".into()]),
        };
        let policy = Policy::new(&[fetch]);
        assert!(policy.request("http://127.0.0.1:8080/x", "GET").is_none());
        let request = Policy::new(&[grant(&["http://127.0.0.1:8080/**"], None)]);
        assert!(
            request
                .network("net.fetch", "http://127.0.0.1:8080/x")
                .is_none()
        );
        assert!(
            request
                .network("net.get", "http://127.0.0.1:8080/x")
                .is_none()
        );
        assert!(
            request
                .request("http://127.0.0.1:8080/x", "PATCH")
                .is_some()
        );
    }

    #[test]
    fn host_ceilings_narrow_urls_and_methods() {
        let requested = [grant(&["https://api.test/**"], None)];
        let ceiling = [grant(&["https://api.test/public/**"], Some(&["GET"]))];
        let policy = Policy::with_ceiling(&requested, Some(&ceiling));
        assert!(policy.request("https://api.test/public/a", "GET").is_some());
        assert!(
            policy
                .request("https://api.test/public/a", "POST")
                .is_none()
        );
        assert!(policy.request("https://api.test/private", "GET").is_none());
        assert!(
            Policy::with_ceiling(&requested, Some(&[]))
                .request("https://api.test/public/a", "GET")
                .is_none()
        );
        // A ceiling cannot add a request the package did not ask for, and
        // empty URL or method lists deny.
        assert!(
            Policy::with_ceiling(&[], Some(&ceiling))
                .request("https://api.test/public/a", "GET")
                .is_none()
        );
        assert!(
            Policy::new(&[grant(&["https://api.test/**"], Some(&[]))])
                .request("https://api.test/a", "GET")
                .is_none()
        );
        assert!(
            Policy::new(&[grant(&[], None)])
                .request("https://api.test/a", "GET")
                .is_none()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_names_never_come_from_wildcards() {
        let env = |patterns: &[&str]| CapabilityGrant {
            id: "env.get".into(),
            value: CapabilityValue::PathGlobs(patterns.iter().map(|p| (*p).into()).collect()),
        };
        let policy = Policy::new(&[env(&["APP_KEY", "APP_*", "B?", "BASE"])]);
        assert_eq!(policy.exact_names("env.get"), ["APP_KEY", "BASE"]);
        assert!(policy.exact_names("env.has").is_empty());
        // The ceiling must list a name exactly too; its wildcard adds none.
        let ceiling = [env(&["APP_*", "BASE"])];
        let policy = Policy::with_ceiling(&[env(&["APP_KEY", "BASE"])], Some(&ceiling));
        assert_eq!(policy.exact_names("env.get"), ["BASE"]);
        let mut policy = Policy::new(&[env(&["APP_KEY"])]);
        policy.withhold("env.get", true);
        assert!(policy.exact_names("env.get").is_empty());
    }
    #[test]
    fn glob_respects_directory_components() {
        assert!(glob("**", "sub/a.txt", true));
        assert!(!glob("*.txt", "sub/a.txt", true));
        assert!(glob("*.txt", "a.txt", true));
    }
    #[test]
    fn url_normalization_precedes_authorization() {
        let policy = Policy::new(&[CapabilityGrant {
            id: "net.fetch".into(),
            value: CapabilityValue::UrlGlobs(vec!["http://example.com/public/**".into()]),
        }]);
        for denied in [
            "http://example.com/public/../private",
            "http://example.com/public/%2e%2e/private",
            "http://example.com/public/x\r\nGET /private",
            "http://example.com.evil/public/x",
        ] {
            assert!(policy.network("net.fetch", denied).is_none(), "{denied}");
        }
        assert!(
            policy
                .network("net.fetch", "http://example.com/public/ok")
                .is_some()
        );
        assert!(
            policy
                .network("net.get", "http://example.com/public/ok")
                .is_none()
        );
    }
    #[test]
    fn a_url_query_separator_is_not_a_wildcard() {
        let policy = Policy::new(&[CapabilityGrant {
            id: "net.fetch".into(),
            value: CapabilityValue::UrlGlobs(vec!["http://example.com/public?x=1".into()]),
        }]);
        assert!(
            policy
                .network("net.fetch", "http://example.com/public?x=1")
                .is_some()
        );
        assert!(
            policy
                .network("net.fetch", "http://example.com/public/x=1")
                .is_none()
        );
    }
}

#[cfg(test)]
mod ceiling_tests {
    use super::*;

    #[test]
    fn environment_scopes_require_both_filters_and_exact_operations() {
        let requested = [CapabilityGrant {
            id: "env.get".into(),
            value: CapabilityValue::PathGlobs(vec!["APP_*".into()]),
        }];
        let ceiling = [CapabilityGrant {
            id: "env.get".into(),
            value: CapabilityValue::PathGlobs(vec!["*_NAME".into()]),
        }];
        let policy = Policy::with_ceiling(&requested, Some(&ceiling));
        assert!(policy.env("env.get", "APP_NAME"));
        for denied in ["APP_SECRET", "OTHER_NAME"] {
            assert!(!policy.env("env.get", denied));
        }
        assert!(!policy.env("env.has", "APP_NAME"));
        assert!(!Policy::with_ceiling(&requested, Some(&[])).env("env.get", "APP_NAME"));
        assert!(!Policy::with_ceiling(&[], Some(&ceiling)).env("env.get", "APP_NAME"));
    }

    #[test]
    fn network_authority_and_path_must_satisfy_both_scopes() {
        let requested = [CapabilityGrant {
            id: "net.fetch".into(),
            value: CapabilityValue::UrlGlobs(vec!["http://example.com/public/**".into()]),
        }];
        let ceiling = [CapabilityGrant {
            id: "net.fetch".into(),
            value: CapabilityValue::UrlGlobs(vec!["http://example.com/public/*/profile".into()]),
        }];
        let policy = Policy::with_ceiling(&requested, Some(&ceiling));
        assert!(
            policy
                .network("net.fetch", "http://example.com/public/user/profile")
                .is_some()
        );
        for denied in [
            "http://example.com/public/user/private",
            "http://example.com:81/public/user/profile",
            "http://example.com/public/../admin/profile",
        ] {
            assert!(policy.network("net.fetch", denied).is_none(), "{denied}");
        }
        assert!(
            policy
                .network("net.get", "http://example.com/public/user/profile")
                .is_none()
        );
        assert!(
            Policy::with_ceiling(&requested, Some(&[]))
                .network("net.fetch", "http://example.com/public/user/profile")
                .is_none()
        );
    }

    #[test]
    fn boolean_permissions_cannot_be_added_by_either_party() {
        let yes = [CapabilityGrant {
            id: "time.now".into(),
            value: CapabilityValue::Boolean(true),
        }];
        let no = [CapabilityGrant {
            id: "time.now".into(),
            value: CapabilityValue::Boolean(false),
        }];
        assert!(Policy::with_ceiling(&yes, Some(&yes)).enabled("time.now"));
        for (requested, ceiling) in [
            (&yes[..], &no[..]),
            (&no[..], &yes[..]),
            (&yes[..], &[][..]),
            (&[][..], &yes[..]),
        ] {
            assert!(!Policy::with_ceiling(requested, Some(ceiling)).enabled("time.now"));
        }
    }
}
