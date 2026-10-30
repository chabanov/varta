use crate::{Artifact, HostPolicy, Result};
use varta_common::ToolInvocationOutput;
use varta_runtime::VartaRuntime;

pub fn invoke_tool(
    artifact: &Artifact,
    name: &str,
    arguments: &[u8],
) -> Result<ToolInvocationOutput> {
    Ok(VartaRuntime::new().invoke_tool(&artifact.wasm, name, arguments, &artifact.grants()?)?)
}
pub fn invoke_tool_with_policy(
    artifact: &Artifact,
    name: &str,
    arguments: &[u8],
    host: &HostPolicy,
) -> Result<ToolInvocationOutput> {
    // The package's declared file roots that the host policy maps.
    let roots =
        varta_runtime::FileRoots::from_manifests(&artifact.manifest, Some(host.manifest()))?;
    Ok(VartaRuntime::new().invoke_tool_with_roots(
        &artifact.wasm,
        name,
        arguments,
        &artifact.grants()?,
        Some(host.grants()),
        &roots,
    )?)
}
