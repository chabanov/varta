use super::*;
use varta_common::json::{JsonField, JsonSchema, JsonType, SCHEMA_VERSION};

/// The schema nodes of several root types in one graph (v38 compiled
/// decoders, [`super::decoders`]): one node per value type, shared by every
/// root that reaches it, with the record shape behind each record node. Each
/// root's own schema ([`build`]) holds the same nodes, numbered from its root.
pub(super) struct Graph {
    pub nodes: Vec<JsonType>,
    /// The record shape of each record node.
    pub records: Vec<Option<u32>>,
    pub roots: BTreeMap<Type, u32>,
}

struct Builder<'a> {
    types: &'a ir::ValueTypes,
    layouts: &'a BTreeMap<records::LayoutKey, u32>,
    indices: BTreeMap<Type, u32>,
    nodes: Vec<JsonType>,
    records: Vec<Option<u32>>,
}
impl Builder<'_> {
    fn new<'a>(
        types: &'a ir::ValueTypes,
        layouts: &'a BTreeMap<records::LayoutKey, u32>,
    ) -> Builder<'a> {
        Builder {
            types,
            layouts,
            indices: BTreeMap::new(),
            nodes: Vec::new(),
            records: Vec::new(),
        }
    }
    fn node(&mut self, ty: Type) -> Result<u32> {
        if let Some(index) = self.indices.get(&ty) {
            return Ok(*index);
        }
        let record = match ty {
            Type::Record(id) => Some(id),
            _ => None,
        };
        // Schema 15 admits recursive types: reserve the node before its
        // children so a cycle refers back to it.
        if self.types.profile.dynamic_values() {
            let index = self.nodes.len() as u32;
            self.nodes.push(JsonType::Null);
            self.records.push(record);
            self.indices.insert(ty, index);
            let node = self.shape(ty)?;
            self.nodes[index as usize] = node;
            return Ok(index);
        }
        let node = self.shape(ty)?;
        let index = self.nodes.len() as u32;
        self.nodes.push(node);
        self.records.push(record);
        self.indices.insert(ty, index);
        Ok(index)
    }
    fn shape(&mut self, ty: Type) -> Result<JsonType> {
        Ok(match ty {
            Type::BigInt => JsonType::BigInt,
            Type::Null => JsonType::Null,
            Type::Undefined => JsonType::Undefined,
            Type::Union(id) | Type::ReferenceUnion(id) => {
                let members = self.types.unions[id as usize]
                    .iter()
                    .map(|ty| self.node(*ty))
                    .collect::<Result<Vec<_>>>()?;
                if matches!(ty, Type::ReferenceUnion(_)) {
                    JsonType::ReferenceUnion { members }
                } else {
                    JsonType::PrimitiveUnion { members }
                }
            }
            Type::Number => JsonType::I32,
            Type::Float64 => JsonType::Float64,
            Type::Boolean => JsonType::Boolean,
            Type::String => JsonType::String,
            Type::Record(id) => {
                let fields = self.types.records[id as usize]
                    .fields
                    .iter()
                    .map(|f| {
                        Ok(JsonField {
                            name: f.name.clone(),
                            optional: f.optional,
                            ty: self.node(f.ty)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                JsonType::Record {
                    fields,
                    descriptor: self.layouts[&records::canonical_layout(id, &self.types.records)],
                }
            }
            Type::Array(id) => JsonType::Array {
                element: self.node(self.types.arrays[id as usize])?,
            },
            // The JSON value graph of `unknown` (v35 `unknown_json`):
            // primitives, `unknown[]` and `Record<string, unknown>`, plus
            // `undefined` for missing optional positions.
            Type::Unknown => {
                let (array, dictionary) = crate::lower::json::unknown_graph_types(self.types)
                    .ok_or_else(|| {
                        CompilerError::Internal("missing unknown JSON graph types".into())
                    })?;
                let members = [
                    Type::Null,
                    Type::Undefined,
                    Type::Boolean,
                    Type::Float64,
                    Type::String,
                    Type::Array(array),
                    Type::Dictionary(dictionary),
                ]
                .into_iter()
                .map(|ty| self.node(ty))
                .collect::<Result<Vec<_>>>()?;
                JsonType::ReferenceUnion { members }
            }
            Type::Dictionary(id) => {
                let dictionary = self.types.dictionaries[id as usize];
                JsonType::Dictionary {
                    descriptor: self.layouts
                        [&records::canonical_layout(dictionary.storage, &self.types.records)],
                    value: self.node(dictionary.value)?,
                }
            }
            _ => return Err(CompilerError::Internal("invalid JSON schema type".into())),
        })
    }
}

/// The one graph of every root type in `roots`.
pub(super) fn graph(
    roots: impl IntoIterator<Item = Type>,
    types: &ir::ValueTypes,
    layouts: &BTreeMap<records::LayoutKey, u32>,
) -> Result<Graph> {
    let mut builder = Builder::new(types, layouts);
    let roots = roots
        .into_iter()
        .map(|root| Ok((root, builder.node(root)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Graph {
        nodes: builder.nodes,
        records: builder.records,
        roots,
    })
}

pub(super) fn build(
    root: Type,
    types: &ir::ValueTypes,
    layouts: &BTreeMap<records::LayoutKey, u32>,
) -> Result<Vec<u8>> {
    let mut builder = Builder::new(types, layouts);
    let root = builder.node(root)?;
    JsonSchema {
        version: if types.profile.dictionary_hashing() {
            varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION
        } else if types.profile.function_fields() {
            varta_common::json::FUNCTION_FIELD_SCHEMA_VERSION
        } else if types.profile.promise_values() {
            varta_common::json::PROMISE_SCHEMA_VERSION
        } else if types.profile.dynamic_values() {
            varta_common::json::DYNAMIC_SCHEMA_VERSION
        } else if types.profile.bigint_values() {
            varta_common::json::BIGINT_SCHEMA_VERSION
        } else if types.profile.sparse_arrays() {
            varta_common::json::SPARSE_SCHEMA_VERSION
        } else if types.profile.error_families() {
            varta_common::json::ERROR_FAMILY_SCHEMA_VERSION
        } else if types.profile.errors() {
            varta_common::json::ERROR_SCHEMA_VERSION
        } else if types.profile.resizable_arrays() {
            varta_common::json::VECTOR_SCHEMA_VERSION
        } else if types.profile.checked_json() {
            varta_common::json::CHECKED_SCHEMA_VERSION
        } else if types.profile.reference_unions() {
            varta_common::json::REFERENCE_SCHEMA_VERSION
        } else if types.profile.missing() {
            varta_common::json::MISSING_SCHEMA_VERSION
        } else if types.profile.unions() {
            varta_common::json::UNION_SCHEMA_VERSION
        } else if types.profile.utf16() {
            varta_common::json::UTF16_SCHEMA_VERSION
        } else if types.profile.has_undefined() {
            varta_common::json::UNDEFINED_SCHEMA_VERSION
        } else if types.profile.binary64() {
            varta_common::json::NUMBER_SCHEMA_VERSION
        } else {
            SCHEMA_VERSION
        },
        root,
        types: builder.nodes,
    }
    .to_bytes_within(&types.profile.limits())
    .map_err(|e| CompilerError::Invalid(e.to_string()))
}
