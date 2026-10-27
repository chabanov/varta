//! Source semantics are versioned separately from the host and value layouts.
use serde::{Deserialize, Serialize};

pub const SECTION: &str = "varta.language";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_METADATA_BYTES: usize = 1024;
pub const SUPPORTED_PROFILES: &[LanguageProfile] = &[
    LanguageProfile::CheckedI32Utf8V1,
    LanguageProfile::CheckedF64Utf8V1,
    LanguageProfile::CheckedF64Utf8V2,
    LanguageProfile::CheckedF64Utf16V1,
    LanguageProfile::CheckedF64Utf16V2,
    LanguageProfile::CheckedF64Utf16V3,
    LanguageProfile::CheckedF64Utf16V4,
    LanguageProfile::CheckedF64Utf16V5,
    LanguageProfile::CheckedF64Utf16V6,
    LanguageProfile::CheckedF64Utf16V7,
    LanguageProfile::CheckedF64Utf16V8,
    LanguageProfile::CheckedF64Utf16V9,
    LanguageProfile::CheckedF64Utf16V10,
    LanguageProfile::CheckedF64Utf16V11,
    LanguageProfile::CheckedF64Utf16V12,
    LanguageProfile::CheckedF64Utf16V13,
    LanguageProfile::CheckedF64Utf16V14,
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanguageProfile {
    #[default]
    #[serde(rename = "checked-i32-utf8-v1")]
    CheckedI32Utf8V1,
    #[serde(rename = "checked-f64-utf8-v1")]
    CheckedF64Utf8V1,
    #[serde(rename = "checked-f64-utf8-v2")]
    CheckedF64Utf8V2,
    #[serde(rename = "checked-f64-utf16-v1")]
    CheckedF64Utf16V1,
    #[serde(rename = "checked-f64-utf16-v2")]
    CheckedF64Utf16V2,
    #[serde(rename = "checked-f64-utf16-v3")]
    CheckedF64Utf16V3,
    #[serde(rename = "checked-f64-utf16-v4")]
    CheckedF64Utf16V4,
    #[serde(rename = "checked-f64-utf16-v5")]
    CheckedF64Utf16V5,
    #[serde(rename = "checked-f64-utf16-v6")]
    CheckedF64Utf16V6,
    #[serde(rename = "checked-f64-utf16-v7")]
    CheckedF64Utf16V7,
    #[serde(rename = "checked-f64-utf16-v8")]
    CheckedF64Utf16V8,
    #[serde(rename = "checked-f64-utf16-v9")]
    CheckedF64Utf16V9,
    #[serde(rename = "checked-f64-utf16-v10")]
    CheckedF64Utf16V10,
    #[serde(rename = "checked-f64-utf16-v11")]
    CheckedF64Utf16V11,
    #[serde(rename = "checked-f64-utf16-v12")]
    CheckedF64Utf16V12,
    #[serde(rename = "checked-f64-utf16-v13")]
    CheckedF64Utf16V13,
    #[serde(rename = "checked-f64-utf16-v14")]
    CheckedF64Utf16V14,
}
impl LanguageProfile {
    #[must_use]
    pub fn host_abi(self) -> u32 {
        match self {
            Self::CheckedI32Utf8V1 => crate::abi::VERSION,
            Self::CheckedF64Utf8V1 => crate::abi::NUMBER_VERSION,
            Self::CheckedF64Utf8V2 => crate::abi::UNDEFINED_VERSION,
            Self::CheckedF64Utf16V1 => crate::abi::UTF16_VERSION,
            Self::CheckedF64Utf16V2 => crate::abi::UNION_VERSION,
            Self::CheckedF64Utf16V3 => crate::abi::MISSING_VERSION,
            Self::CheckedF64Utf16V4 => crate::abi::REFERENCE_VERSION,
            Self::CheckedF64Utf16V5 => crate::abi::CHECKED_JSON_VERSION,
            Self::CheckedF64Utf16V6 => crate::abi::VECTOR_VERSION,
            Self::CheckedF64Utf16V7 => crate::abi::EXCEPTION_VERSION,
            Self::CheckedF64Utf16V8 => crate::abi::ERROR_VERSION,
            Self::CheckedF64Utf16V9 => crate::abi::ERROR_FAMILY_VERSION,
            Self::CheckedF64Utf16V10 => crate::abi::RECOVERABLE_JSON_VERSION,
            Self::CheckedF64Utf16V11 => crate::abi::FUNCTION_VERSION,
            Self::CheckedF64Utf16V12 => crate::abi::CLOSURE_VERSION,
            Self::CheckedF64Utf16V13 => crate::abi::SPARSE_VERSION,
            Self::CheckedF64Utf16V14 => crate::abi::ARRAY_MUTATION_VERSION,
        }
    }
    #[must_use]
    pub fn binary64(self) -> bool {
        self != Self::CheckedI32Utf8V1
    }
    #[must_use]
    pub fn has_undefined(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf8V2
                | Self::CheckedF64Utf16V1
                | Self::CheckedF64Utf16V2
                | Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn utf16(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V1
                | Self::CheckedF64Utf16V2
                | Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn unions(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V2
                | Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn missing(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn reference_unions(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn checked_json(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn resizable_arrays(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn exceptions(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn errors(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn error_families(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn recoverable_json(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn function_values(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn closures(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V12 | Self::CheckedF64Utf16V13 | Self::CheckedF64Utf16V14
        )
    }
    #[must_use]
    pub fn sparse_arrays(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V13 | Self::CheckedF64Utf16V14)
    }
    #[must_use]
    pub fn array_mutations(self) -> bool {
        self == Self::CheckedF64Utf16V14
    }
    #[must_use]
    pub fn string_bytes(self, text: &str) -> Vec<u8> {
        if self.utf16() {
            crate::text::encode(text)
        } else {
            text.as_bytes().to_vec()
        }
    }
    /// Select an explicit encoding-specific import; shared scalar imports stay unchanged.
    #[must_use]
    pub fn host_operation(self, operation: crate::HostOperation) -> crate::HostOperation {
        use crate::HostOperation::*;
        if self.sparse_arrays() {
            return match operation {
                UnionLog | ValueLog | ValueLogV3 | ValueLogV4 | ValueLogV5 => ValueLogV6,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 | ValueTypeOfV4 | ValueTypeOfV5 => {
                    ValueTypeOfV6
                }
                UnionTruthy | ValueTruthy | ValueTruthyV3 | ValueTruthyV4 | ValueTruthyV5 => {
                    ValueTruthyV6
                }
                UnionNullish | ValueNullish | ValueNullishV3 | ValueNullishV4 | ValueNullishV5 => {
                    ValueNullishV6
                }
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 | ValueStrictEqualV4
                | ValueStrictEqualV5 => ValueStrictEqualV6,
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3
                | ValueNullishEqualV4 | ValueNullishEqualV5 => ValueNullishEqualV6,
                ValueIsArray | ValueIsArrayV3 | ValueIsArrayV4 | ValueIsArrayV5 => ValueIsArrayV6,
                ValueIsError | ValueIsErrorClass => ValueIsErrorClassV2,
                other => self.utf16_operation(other),
            };
        }

        if self.error_families() {
            match operation {
                UnionLog | ValueLog | ValueLogV3 | ValueLogV4 => return ValueLogV5,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 | ValueTypeOfV4 => return ValueTypeOfV5,
                UnionTruthy | ValueTruthy | ValueTruthyV3 | ValueTruthyV4 => return ValueTruthyV5,
                UnionNullish | ValueNullish | ValueNullishV3 | ValueNullishV4 => {
                    return ValueNullishV5;
                }
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 | ValueStrictEqualV4 => {
                    return ValueStrictEqualV5;
                }
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3
                | ValueNullishEqualV4 => return ValueNullishEqualV5,
                ValueIsArray | ValueIsArrayV3 | ValueIsArrayV4 => return ValueIsArrayV5,
                ValueIsError => return ValueIsErrorClass,
                _ => {}
            }
        }
        if self.errors() {
            match operation {
                UnionLog | ValueLog | ValueLogV3 => return ValueLogV4,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 => return ValueTypeOfV4,
                UnionTruthy | ValueTruthy | ValueTruthyV3 => return ValueTruthyV4,
                UnionNullish | ValueNullish | ValueNullishV3 => return ValueNullishV4,
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 => {
                    return ValueStrictEqualV4;
                }
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3 => {
                    return ValueNullishEqualV4;
                }
                ValueIsArray | ValueIsArrayV3 => return ValueIsArrayV4,
                _ => {}
            }
        }
        if self.resizable_arrays() {
            match operation {
                UnionLog | ValueLog => return ValueLogV3,
                UnionTypeOf | ValueTypeOf => return ValueTypeOfV3,
                UnionTruthy | ValueTruthy => return ValueTruthyV3,
                UnionNullish | ValueNullish => return ValueNullishV3,
                UnionStrictEqual | ValueStrictEqual => return ValueStrictEqualV3,
                UnionNullishEqual | ValueNullishEqual => return ValueNullishEqualV3,
                ValueIsArray => return ValueIsArrayV3,
                _ => {}
            }
        }
        if self.reference_unions() {
            match operation {
                UnionLog => return ValueLog,
                UnionTypeOf => return ValueTypeOf,
                UnionTruthy => return ValueTruthy,
                UnionNullish => return ValueNullish,
                UnionStrictEqual => return ValueStrictEqual,
                UnionNullishEqual => return ValueNullishEqual,
                _ => {}
            }
        }
        self.utf16_operation(operation)
    }
    fn utf16_operation(self, operation: crate::HostOperation) -> crate::HostOperation {
        use crate::HostOperation::*;
        if !self.utf16() {
            return operation;
        }
        match operation {
            LogString => LogString16,
            EnvHas => EnvHas16,
            EnvGet => EnvGet16,
            EnvPrint => EnvPrint16,
            FsRead => FsRead16,
            FsPrint => FsPrint16,
            NetFetch => NetFetch16,
            NetGet => NetGet16,
            other => other,
        }
    }
    #[must_use]
    pub fn value_abi(self) -> u32 {
        match self {
            Self::CheckedF64Utf16V13 | Self::CheckedF64Utf16V14 => {
                crate::value::SPARSE_VALUE_VERSION
            }
            Self::CheckedI32Utf8V1 => crate::value::VALUE_VERSION,
            Self::CheckedF64Utf8V1 => crate::value::NUMBER_VALUE_VERSION,
            Self::CheckedF64Utf8V2 => crate::value::UNDEFINED_VALUE_VERSION,
            Self::CheckedF64Utf16V1 => crate::value::UTF16_VALUE_VERSION,
            Self::CheckedF64Utf16V2 => crate::value::UNION_VALUE_VERSION,
            Self::CheckedF64Utf16V3 => crate::value::MISSING_VALUE_VERSION,
            Self::CheckedF64Utf16V8 => crate::value::ERROR_VALUE_VERSION,
            Self::CheckedF64Utf16V9
            | Self::CheckedF64Utf16V10
            | Self::CheckedF64Utf16V11
            | Self::CheckedF64Utf16V12 => crate::value::ERROR_FAMILY_VALUE_VERSION,
            Self::CheckedF64Utf16V6 | Self::CheckedF64Utf16V7 => crate::value::VECTOR_VALUE_VERSION,
            Self::CheckedF64Utf16V4 | Self::CheckedF64Utf16V5 => {
                crate::value::REFERENCE_VALUE_VERSION
            }
        }
    }
    #[must_use]
    pub fn json_stringify(self) -> crate::HostOperation {
        match self {
            Self::CheckedF64Utf16V13 | Self::CheckedF64Utf16V14 => {
                crate::HostOperation::JsonStringifyV12
            }
            Self::CheckedI32Utf8V1 => crate::HostOperation::JsonStringify,
            Self::CheckedF64Utf8V1 => crate::HostOperation::JsonStringifyV3,
            Self::CheckedF64Utf8V2 => crate::HostOperation::JsonStringifyV4,
            Self::CheckedF64Utf16V1 => crate::HostOperation::JsonStringifyV5,
            Self::CheckedF64Utf16V2 => crate::HostOperation::JsonStringifyV6,
            Self::CheckedF64Utf16V3 => crate::HostOperation::JsonStringifyV7,
            Self::CheckedF64Utf16V8 => crate::HostOperation::JsonStringifyV10,
            Self::CheckedF64Utf16V9
            | Self::CheckedF64Utf16V10
            | Self::CheckedF64Utf16V11
            | Self::CheckedF64Utf16V12 => crate::HostOperation::JsonStringifyV11,
            Self::CheckedF64Utf16V6 | Self::CheckedF64Utf16V7 => {
                crate::HostOperation::JsonStringifyV9
            }
            Self::CheckedF64Utf16V4 | Self::CheckedF64Utf16V5 => {
                crate::HostOperation::JsonStringifyV8
            }
        }
    }
    #[must_use]
    pub fn json_parse(self) -> crate::HostOperation {
        match self {
            Self::CheckedF64Utf16V13 | Self::CheckedF64Utf16V14 => {
                crate::HostOperation::JsonParseV14
            }
            Self::CheckedI32Utf8V1 => crate::HostOperation::JsonParse,
            Self::CheckedF64Utf8V1 => crate::HostOperation::JsonParseV3,
            Self::CheckedF64Utf8V2 => crate::HostOperation::JsonParseV4,
            Self::CheckedF64Utf16V1 => crate::HostOperation::JsonParseV5,
            Self::CheckedF64Utf16V2 => crate::HostOperation::JsonParseV6,
            Self::CheckedF64Utf16V3 => crate::HostOperation::JsonParseV7,
            Self::CheckedF64Utf16V4 => crate::HostOperation::JsonParseV8,
            Self::CheckedF64Utf16V5 => crate::HostOperation::JsonParseV9,
            Self::CheckedF64Utf16V8 => crate::HostOperation::JsonParseV11,
            Self::CheckedF64Utf16V9 => crate::HostOperation::JsonParseV12,
            Self::CheckedF64Utf16V10 | Self::CheckedF64Utf16V11 | Self::CheckedF64Utf16V12 => {
                crate::HostOperation::JsonParseV13
            }
            Self::CheckedF64Utf16V6 | Self::CheckedF64Utf16V7 => crate::HostOperation::JsonParseV10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LanguageContract {
    pub schema_version: u32,
    pub profile: LanguageProfile,
}
impl LanguageContract {
    #[must_use]
    pub fn current() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profile: LanguageProfile::CheckedI32Utf8V1,
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, LanguageError> {
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(LanguageError("metadata exceeds byte limit".into()));
        }
        let contract: Self =
            serde_json::from_slice(bytes).map_err(|e| LanguageError(e.to_string()))?;
        contract.validate()?;
        Ok(contract)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, LanguageError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|e| LanguageError(e.to_string()))
    }
    fn validate(&self) -> Result<(), LanguageError> {
        if self.schema_version != SCHEMA_VERSION || !SUPPORTED_PROFILES.contains(&self.profile) {
            return Err(LanguageError(
                "unsupported schema version or profile".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LanguageOrigin {
    Declared,
    LegacyHostAbi,
    Unspecified,
}

/// Validated declaration or legacy interpretation, never a proof of source
/// provenance or of arbitrary Wasm obeying a particular language's semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInspection {
    pub profile: Option<LanguageProfile>,
    pub origin: LanguageOrigin,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid language profile: {0}")]
pub struct LanguageError(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_contract_is_strict_bounded_and_versioned() {
        let contract = LanguageContract::current();
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        for bytes in [
            br#"{}"#.as_slice(),
            br#"{"schemaVersion":2,"profile":"checked-i32-utf8-v1"}"#,
            br#"{"schemaVersion":1,"profile":"ecmascript-2025-v1"}"#,
            br#"{"schemaVersion":1,"profile":"checked-i32-utf8-v1","fallback":true}"#,
            br#"{"schemaVersion":1,"schemaVersion":1,"profile":"checked-i32-utf8-v1"}"#,
            br#"{"schemaVersion":1,"profile":"checked-i32-utf8-v1","profile":"checked-i32-utf8-v1"}"#,
        ] {
            assert!(LanguageContract::parse(bytes).is_err());
        }
        assert!(LanguageContract::parse(&vec![b' '; MAX_METADATA_BYTES + 1]).is_err());
        let mut unsupported = contract;
        unsupported.schema_version += 1;
        assert!(unsupported.to_bytes().is_err());
    }
}
