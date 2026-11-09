//! Runtime bootstrap data belongs to this local host, never distribution output.
use super::{BrowserDevOptions, INDEX, LOADER};
use crate::{Artifact, DecisionFile, HostPolicy, ProjectError, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn runtime_inputs(
    artifact: &Artifact,
    policy: Option<&HostPolicy>,
    decisions: Option<&DecisionFile>,
    options: &BrowserDevOptions,
) -> serde_json::Value {
    use varta_common::{CapabilityValue, budget::BudgetKind};
    let mut inputs = serde_json::json!({"application":options.application,"reportBudgets":options.report_budgets});
    if options.entry_prefix.is_some() {
        inputs["entryPathInput"] = serde_json::json!("VARTA_ENTRY_PATH");
        inputs["entryPathPrefix"] = serde_json::json!(options.entry_prefix);
    }
    if let Some(policy) = policy {
        let mut ceiling = serde_json::Map::new();
        for grant in policy.grants() {
            let value = match &grant.value {
                CapabilityValue::Boolean(value) => Some(serde_json::json!(value)),
                CapabilityValue::Requests(scope)
                    if matches!(
                        grant.id.as_str(),
                        varta_common::net::REQUEST_CAPABILITY
                            | varta_common::session::CLIENT_CAPABILITY
                    ) =>
                {
                    let mut value = serde_json::json!({"patterns":scope.urls});
                    if let Some(methods) = &scope.methods {
                        value["methods"] = serde_json::json!(methods);
                    }
                    if let Some(headers) = &scope.response_headers {
                        value["responseHeaders"] = serde_json::json!(headers);
                    }
                    Some(value)
                }
                CapabilityValue::UrlGlobs(patterns)
                    if grant.id == varta_common::view::OPEN_URL_CAPABILITY =>
                {
                    Some(serde_json::json!({"patterns":patterns}))
                }
                CapabilityValue::PathGlobs(patterns)
                    if matches!(grant.id.as_str(), "env.get" | "env.has") =>
                {
                    Some(serde_json::json!({"patterns":patterns}))
                }
                _ => None,
            };
            if let Some(value) = value {
                ceiling.insert(grant.id.clone(), value);
            }
        }
        if let Some(binding) = &policy.manifest().browser_session {
            inputs["sessionBinding"] = serde_json::json!(binding);
        }
        inputs["permissionCeiling"] = serde_json::Value::Object(ceiling);
        if let Some(request) = policy.manifest().budgets {
            inputs["budgetCeiling"] = serde_json::Value::Object(
                BudgetKind::ALL
                    .into_iter()
                    .filter_map(|kind| {
                        request
                            .value(kind)
                            .map(|value| (kind.wire_name().into(), serde_json::json!(value)))
                    })
                    .collect(),
            );
        }
    }
    if let Some(file) = decisions {
        let artifact = varta_runtime::artifact_identity(&artifact.wasm);
        let selected = file
            .decisions_for(&artifact)
            .into_iter()
            .map(|(id, decision)| serde_json::json!({"id":id,"decision":decision}))
            .collect::<Vec<_>>();
        inputs["decisions"] = serde_json::json!({"schemaVersion":varta_common::permission::DECISION_SCHEMA_VERSION,"applications":[{"artifact":artifact,"decisions":selected}]});
    }
    inputs
}

pub(super) fn install_bootstrap(
    files: &mut BTreeMap<String, Vec<u8>>,
    inputs: &serde_json::Value,
) -> Result<()> {
    // The loader is an HTTP resource. A guest with network permission could
    // fetch it, bypassing per-variable env authorization if values were embedded.
    if inputs.get("environmentInputs").is_some() {
        return Err(ProjectError::Usage(
            "environment values cannot be embedded in a browser development bundle".into(),
        ));
    }
    let mut loader = files
        .remove(LOADER)
        .ok_or_else(|| ProjectError::Artifact("missing browser loader".into()))?;
    let integrity = |bytes: &[u8]| {
        format!(
            "sha256-{}",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
        )
    };
    let before = integrity(&loader);
    loader.extend_from_slice(include_bytes!("client.js"));
    loader.extend_from_slice(
        format!(
            "\nif(typeof document!==\"undefined\"&&[...document.querySelectorAll(\"script[data-varta-dev]\")].some(script=>script.src===import.meta.url))devBootstrap({});\n",
            serde_json::to_string(inputs).map_err(|e| ProjectError::Artifact(e.to_string()))?
        )
        .as_bytes(),
    );
    let after = integrity(&loader);
    let html = String::from_utf8(
        files
            .remove(INDEX)
            .ok_or_else(|| ProjectError::Artifact("missing browser page".into()))?,
    )
    .map_err(|e| ProjectError::Artifact(e.to_string()))?;
    // A generator format change must fail closed, rather than accidentally
    // retaining the distribution bootstrap without this host's runtime ceiling.
    let optin = " data-varta-bootstrap src=";
    if html.matches(optin).count() != 1 || html.matches(&before).count() != 2 {
        return Err(ProjectError::Artifact(
            "browser development bootstrap boundary changed".into(),
        ));
    }
    files.insert(
        INDEX.into(),
        html.replace(&before, &after)
            .replace(optin, " data-varta-dev src=")
            .into_bytes(),
    );
    files.insert(LOADER.into(), loader);
    Ok(())
}
