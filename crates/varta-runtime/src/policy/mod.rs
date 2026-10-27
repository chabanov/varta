mod files;
mod glob;
use files::FilePolicy;
use glob::{glob, wildcard};
use varta_common::{CapabilityGrant, CapabilityValue};

pub(crate) struct Policy {
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
    files: FilePolicy,
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
        }
    }

    fn both(&self, permits: impl Fn(&[CapabilityGrant]) -> bool) -> bool {
        permits(&self.requested) && self.ceiling.as_deref().is_none_or(permits)
    }

    pub(crate) fn enabled(&self, id: &str) -> bool {
        self.both(|grants| {
            grants
                .iter()
                .any(|g| g.id == id && g.value == CapabilityValue::Boolean(true))
        })
    }

    pub(crate) fn env(&self, id: &str, name: &str) -> bool {
        !name.is_empty() && name.len() <= 256 && !name.contains(['=', '\0']) && self.both(|grants| {
            grants.iter().any(|g| g.id == id && matches!(&g.value, CapabilityValue::PathGlobs(p) if p.iter().any(|p| glob(p, name, false))))
        })
    }

    pub(crate) fn network(&self, id: &str, input: &str) -> Option<url::Url> {
        if input.len() > 8192
            || input
                .chars()
                .any(|c| c.is_ascii_control() || c.is_whitespace())
        {
            return None;
        }
        let url = url::Url::parse(input).ok()?;
        if url.path().contains('%')
            || url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        self.both(|grants| {
            grants.iter().filter(|g| g.id == id).any(|g| {
                let CapabilityValue::UrlGlobs(patterns) = &g.value else {
                    return false;
                };
                patterns.iter().any(|pattern| {
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
                })
            })
        })
        .then_some(url)
    }

    pub(crate) fn listen(&self, endpoint: &str) -> bool {
        self.both(|grants| grants.iter().any(|g| g.id == "net.listen"
            && matches!(&g.value, CapabilityValue::ListenEndpoints(p) if p.iter().any(|p| p == endpoint))))
    }

    pub(crate) fn read(&self, id: &str, input: &str) -> Result<Vec<u8>, crate::HostFailure> {
        self.files.read(id, input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
