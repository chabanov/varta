use super::*;
use crate::value::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonSchema {
    pub version: u32,
    pub root: u32,
    pub types: Vec<JsonType>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum JsonType {
    BigInt,
    Null,
    Undefined,
    PrimitiveUnion {
        members: Vec<u32>,
    },
    ReferenceUnion {
        members: Vec<u32>,
    },
    I32,
    Float64,
    Boolean,
    String,
    Record {
        descriptor: u32,
        fields: Vec<JsonField>,
    },
    Array {
        element: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonField {
    pub name: String,
    pub ty: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}
impl JsonType {
    #[must_use]
    pub fn kind(&self) -> ValueKind {
        match self {
            Self::BigInt => ValueKind::BigInt,
            Self::Null => ValueKind::Null,
            Self::Undefined => ValueKind::Undefined,
            Self::PrimitiveUnion { .. } => ValueKind::PrimitiveUnion,
            Self::ReferenceUnion { .. } => ValueKind::ReferenceUnion,
            Self::I32 => ValueKind::I32,
            Self::Float64 => ValueKind::Float64,
            Self::Boolean => ValueKind::Boolean,
            Self::String => ValueKind::String,
            Self::Record { .. } => ValueKind::Record,
            Self::Array { .. } => ValueKind::Array,
        }
    }
}
impl JsonSchema {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CheckedJsonError> {
        if bytes.len() > MAX_SCHEMA_BYTES {
            return Err(CheckedJsonError::Schema("byte limit".into()));
        }
        let schema: Self =
            serde_json::from_slice(bytes).map_err(|e| CheckedJsonError::Schema(e.to_string()))?;
        schema.validate()?;
        Ok(schema)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, CheckedJsonError> {
        self.validate()?;
        let bytes =
            serde_json::to_vec(self).map_err(|e| CheckedJsonError::Schema(e.to_string()))?;
        if bytes.len() > MAX_SCHEMA_BYTES {
            return Err(CheckedJsonError::Schema("byte limit".into()));
        }
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), CheckedJsonError> {
        fn bad(message: &str) -> CheckedJsonError {
            CheckedJsonError::Schema(message.into())
        }
        if !matches!(
            self.version,
            SCHEMA_VERSION
                | NUMBER_SCHEMA_VERSION
                | UNDEFINED_SCHEMA_VERSION
                | UTF16_SCHEMA_VERSION
                | UNION_SCHEMA_VERSION
                | MISSING_SCHEMA_VERSION
                | REFERENCE_SCHEMA_VERSION
                | CHECKED_SCHEMA_VERSION
                | VECTOR_SCHEMA_VERSION
                | ERROR_SCHEMA_VERSION
                | ERROR_FAMILY_SCHEMA_VERSION
                | SPARSE_SCHEMA_VERSION
                | BIGINT_SCHEMA_VERSION
                | FUNCTION_SCHEMA_VERSION
        ) || self.types.is_empty()
            || self.types.len()
                > if matches!(
                    self.version,
                    CHECKED_SCHEMA_VERSION
                        | VECTOR_SCHEMA_VERSION
                        | ERROR_SCHEMA_VERSION
                        | ERROR_FAMILY_SCHEMA_VERSION
                        | SPARSE_SCHEMA_VERSION
                        | BIGINT_SCHEMA_VERSION
                        | FUNCTION_SCHEMA_VERSION
                ) {
                    MAX_SCHEMA_TYPES
                } else {
                    LEGACY_MAX_SCHEMA_TYPES
                }
            || self.root as usize >= self.types.len()
        {
            return Err(bad("version, root or type count"));
        }
        // Strict backward edges give a bounded DAG without recursive validation.
        let mut depths = Vec::new();
        for (index, ty) in self.types.iter().enumerate() {
            if !ty.kind().supported_by(self.value_version()) {
                return Err(bad("numeric type does not match schema version"));
            }
            if !matches!(
                self.version,
                CHECKED_SCHEMA_VERSION
                    | VECTOR_SCHEMA_VERSION
                    | ERROR_SCHEMA_VERSION
                    | ERROR_FAMILY_SCHEMA_VERSION
                    | SPARSE_SCHEMA_VERSION
                    | BIGINT_SCHEMA_VERSION
                    | FUNCTION_SCHEMA_VERSION
            ) && matches!(
                ty,
                JsonType::Undefined
                    | JsonType::PrimitiveUnion { .. }
                    | JsonType::ReferenceUnion { .. }
            ) {
                return Err(bad("extended types require checked schema version"));
            }
            let child_depth = |child: u32| -> Result<usize, CheckedJsonError> {
                if child as usize >= index {
                    return Err(bad("cyclic or forward type reference"));
                }
                Ok(depths[child as usize] + 1)
            };
            let depth = match ty {
                JsonType::Record { fields, .. } => {
                    if fields.len() > MAX_RECORD_FIELDS
                        || fields.windows(2).any(|pair| pair[0].name >= pair[1].name)
                        || fields
                            .iter()
                            .any(|f| f.name.len() > MAX_RECORD_KEY_BYTES || f.name == "__proto__")
                    {
                        return Err(bad("record field count, ordering or key"));
                    }
                    let mut depth = 0;
                    for field in fields {
                        if field.optional
                            && (!matches!(
                                self.version,
                                CHECKED_SCHEMA_VERSION
                                    | VECTOR_SCHEMA_VERSION
                                    | ERROR_SCHEMA_VERSION
                                    | ERROR_FAMILY_SCHEMA_VERSION
                                    | SPARSE_SCHEMA_VERSION
                                    | BIGINT_SCHEMA_VERSION
                                    | FUNCTION_SCHEMA_VERSION
                            ) || !self.admits_undefined(field.ty))
                        {
                            return Err(bad(
                                "optional field must admit undefined under checked schema",
                            ));
                        }
                        depth = depth.max(child_depth(field.ty)?);
                    }
                    depth
                }
                JsonType::Array { element } => child_depth(*element)?,
                JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members } => {
                    if members.len() < 2 || members.len() > MAX_UNION_MEMBERS {
                        return Err(bad("union member count"));
                    }
                    let mut kinds = std::collections::BTreeSet::new();
                    let mut depth = 0;
                    let mut references = false;
                    for member in members {
                        depth = depth.max(child_depth(*member)?);
                        let kind = self.types[*member as usize].kind();
                        if matches!(kind, ValueKind::PrimitiveUnion | ValueKind::ReferenceUnion)
                            || !kinds.insert(kind as u32)
                        {
                            return Err(bad("nested or ambiguous union members"));
                        }
                        references |= matches!(
                            kind,
                            ValueKind::BigInt | ValueKind::Record | ValueKind::Array
                        );
                    }
                    if references != matches!(ty, JsonType::ReferenceUnion { .. }) {
                        return Err(bad("union cell kind"));
                    }
                    depth
                }
                _ => 0,
            };
            if depth > MAX_JSON_DEPTH {
                return Err(bad("depth limit"));
            }
            depths.push(depth);
        }
        Ok(())
    }
    #[must_use]
    pub fn admits_undefined(&self, ty: u32) -> bool {
        match self.types.get(ty as usize) {
            Some(JsonType::Undefined) => true,
            Some(JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members }) => {
                members
                    .iter()
                    .any(|m| matches!(self.types.get(*m as usize), Some(JsonType::Undefined)))
            }
            _ => false,
        }
    }
    #[must_use]
    pub fn value_version(&self) -> u32 {
        if self.version == FUNCTION_SCHEMA_VERSION {
            return FUNCTION_VALUE_VERSION;
        }
        if self.version == BIGINT_SCHEMA_VERSION {
            return BIGINT_VALUE_VERSION;
        }
        if self.version == SPARSE_SCHEMA_VERSION {
            return SPARSE_VALUE_VERSION;
        }
        if self.version == ERROR_FAMILY_SCHEMA_VERSION {
            return ERROR_FAMILY_VALUE_VERSION;
        }
        if self.version == ERROR_SCHEMA_VERSION {
            return ERROR_VALUE_VERSION;
        }
        if self.version == VECTOR_SCHEMA_VERSION {
            return VECTOR_VALUE_VERSION;
        }
        if matches!(
            self.version,
            REFERENCE_SCHEMA_VERSION | CHECKED_SCHEMA_VERSION
        ) {
            return REFERENCE_VALUE_VERSION;
        }
        if self.version == MISSING_SCHEMA_VERSION {
            return MISSING_VALUE_VERSION;
        }
        if self.version == UNION_SCHEMA_VERSION {
            return UNION_VALUE_VERSION;
        }
        if self.version == UTF16_SCHEMA_VERSION {
            return UTF16_VALUE_VERSION;
        }
        if self.version == UNDEFINED_SCHEMA_VERSION {
            return UNDEFINED_VALUE_VERSION;
        }
        if self.version == NUMBER_SCHEMA_VERSION {
            NUMBER_VALUE_VERSION
        } else {
            VALUE_VERSION
        }
    }
}
