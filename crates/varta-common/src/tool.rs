//! Public, bounded tool metadata. Schemas describe values; they never grant authority.
use crate::abi;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const TOOL_SECTION: &str = "varta.tools";
pub const TOOL_EXPORT_PREFIX: &str = "varta.tool.";
pub const TOOL_ABI_EXPORT_PREFIX: &str = "varta.tool_abi.";
pub const TOOL_COPY_STRING: &str = "varta.tool_abi.copy_string";
pub const TOOL_RETAIN_STRING: &str = "varta.tool_abi.retain_string";
pub const TOOL_CONTRACT_VERSION: u32 = 1;
pub const TOOL_INVOCATION_ABI_VERSION: u32 = 1;
pub const MAX_TOOL_CATALOG_BYTES: usize = 256 * 1024;
pub const MAX_TOOLS: usize = 64;
pub const MAX_TOOL_PARAMETERS: usize = 16;
pub const MAX_TOOL_NAME_BYTES: usize = 64;
pub const MAX_TOOL_DESCRIPTION_BYTES: usize = 2048;

#[derive(Debug, thiserror::Error)]
#[error("invalid tool contract: {0}")]
pub struct ToolContractError(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolType {
    Integer,
    Boolean,
    String,
    Void,
}
impl ToolType {
    #[must_use]
    pub fn schema(self) -> serde_json::Value {
        match self {
            Self::Integer => {
                serde_json::json!({"type":"integer", "minimum":i32::MIN, "maximum":i32::MAX})
            }
            Self::Boolean => serde_json::json!({"type":"boolean"}),
            // JSON Schema maxLength counts characters. The separate byte limit is
            // authoritative for UTF-8 allocation and must also be checked by a caller.
            Self::String => serde_json::json!({"type":"string", "maxLength":abi::MAX_STRING_BYTES}),
            Self::Void => serde_json::json!({"type":"null"}),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolParameter {
    pub name: String,
    pub value_type: ToolType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolLimits {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub string_bytes: usize,
    pub memory_bytes: usize,
    pub fuel: u64,
    pub host_calls: u32,
    pub deadline_ms: u64,
}
impl Default for ToolLimits {
    fn default() -> Self {
        Self {
            input_bytes: abi::MAX_INPUT_BYTES,
            output_bytes: abi::MAX_OUTPUT_BYTES,
            string_bytes: abi::MAX_STRING_BYTES,
            memory_bytes: abi::MAX_MEMORY_BYTES,
            fuel: abi::FUEL,
            host_calls: abi::MAX_HOST_CALLS,
            deadline_ms: abi::DEADLINE_SECONDS * 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolContract {
    pub name: String,
    pub description: String,
    pub export: String,
    pub parameters: Vec<ToolParameter>,
    pub result: ToolType,
    pub required_effects: Vec<String>,
    pub limits: ToolLimits,
}
impl ToolContract {
    #[must_use]
    pub fn input_schema(&self) -> serde_json::Value {
        let properties: BTreeMap<_, _> = self
            .parameters
            .iter()
            .map(|p| (&p.name, p.value_type.schema()))
            .collect();
        serde_json::json!({"type":"object", "properties":properties,
            "required":self.parameters.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "additionalProperties":false})
    }
    #[must_use]
    pub fn output_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object", "properties":{"value":self.result.schema()},
            "required":["value"], "additionalProperties":false})
    }
    #[must_use]
    pub fn uses_strings(&self) -> bool {
        self.result == ToolType::String
            || self
                .parameters
                .iter()
                .any(|p| p.value_type == ToolType::String)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolCatalog {
    pub contract_version: u32,
    pub invocation_abi_version: u32,
    pub tools: Vec<ToolContract>,
}
impl ToolCatalog {
    #[must_use]
    pub fn new(tools: Vec<ToolContract>) -> Self {
        Self {
            contract_version: TOOL_CONTRACT_VERSION,
            invocation_abi_version: TOOL_INVOCATION_ABI_VERSION,
            tools,
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, ToolContractError> {
        if bytes.len() > MAX_TOOL_CATALOG_BYTES {
            return Err(ToolContractError("catalog exceeds byte limit".into()));
        }
        // serde_json's recursion limit stays enabled; there are no open-ended
        // JSON values in this wire schema. Unknown fields and duplicate keys fail.
        let catalog: Self =
            serde_json::from_slice(bytes).map_err(|e| ToolContractError(e.to_string()))?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, ToolContractError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| ToolContractError(e.to_string()))?;
        if bytes.len() > MAX_TOOL_CATALOG_BYTES {
            return Err(ToolContractError("catalog exceeds byte limit".into()));
        }
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), ToolContractError> {
        let invalid = || {
            ToolContractError("unsupported version, limits or noncanonical tool metadata".into())
        };
        if self.contract_version != TOOL_CONTRACT_VERSION
            || self.invocation_abi_version != TOOL_INVOCATION_ABI_VERSION
            || self.tools.is_empty()
            || self.tools.len() > MAX_TOOLS
        {
            return Err(invalid());
        }
        let mut previous: Option<&str> = None;
        for tool in &self.tools {
            if !valid_tool_name(&tool.name)
                || previous.is_some_and(|name| name >= tool.name.as_str())
                || tool.export != format!("{TOOL_EXPORT_PREFIX}{}", tool.name)
                || !valid_tool_description(&tool.description)
                || tool.parameters.len() > MAX_TOOL_PARAMETERS
                || tool.limits != ToolLimits::default()
            {
                return Err(invalid());
            }
            previous = Some(&tool.name);
            let mut parameters = BTreeSet::new();
            for parameter in &tool.parameters {
                if !valid_parameter_name(&parameter.name)
                    || parameter.value_type == ToolType::Void
                    || !parameters.insert(&parameter.name)
                {
                    return Err(invalid());
                }
            }
            let mut effects = BTreeSet::new();
            for effect in &tool.required_effects {
                if crate::capability_kind(effect).is_none() || !effects.insert(effect) {
                    return Err(invalid());
                }
            }
            if !tool.required_effects.windows(2).all(|p| p[0] < p[1]) {
                return Err(invalid());
            }
        }
        Ok(())
    }
    /// Public projection adds generated schemas, never duplicated schema authority.
    #[must_use]
    pub fn discovery(&self) -> serde_json::Value {
        let tools: Vec<_> = self
            .tools
            .iter()
            .map(|tool| {
                serde_json::json!({"name":tool.name, "description":tool.description,
                "export":tool.export, "parameters":tool.parameters, "result":tool.result,
                "inputSchema":tool.input_schema(), "outputSchema":tool.output_schema(),
                "requiredEffects":tool.required_effects, "limits":tool.limits,
                "domainErrors":[]})
            })
            .collect();
        serde_json::json!({"contractVersion":self.contract_version,
            "invocationAbiVersion":self.invocation_abi_version, "tools":tools})
    }
}

#[must_use]
pub fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_TOOL_NAME_BYTES
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
#[must_use]
pub fn valid_tool_description(description: &str) -> bool {
    !description.trim().is_empty()
        && description.len() <= MAX_TOOL_DESCRIPTION_BYTES
        && !description.contains('\0')
}
fn valid_parameter_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_TOOL_NAME_BYTES
        && (name.as_bytes()[0].is_ascii_alphabetic() || matches!(name.as_bytes()[0], b'_' | b'$'))
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$'))
}

#[cfg(test)]
mod tests;
