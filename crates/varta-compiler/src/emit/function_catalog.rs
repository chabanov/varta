//! Metadata for typed inspection of the existing private closure capture graph.
use super::*;
use varta_common::function::{
    CaptureBinding, CaptureField, CaptureFunction, CaptureType, FunctionCatalog,
};
pub(super) fn build(
    program: &ir::Program,
    bindings: &BTreeMap<Type, u32>,
    targets: &BTreeMap<u32, Vec<usize>>,
    frames: Vec<varta_common::function::FrameLayout>,
) -> Result<Vec<u8>> {
    struct Builder<'a> {
        types: &'a ir::ValueTypes,
        nodes: Vec<CaptureType>,
        indices: BTreeMap<Type, u32>,
    }
    impl Builder<'_> {
        fn node(&mut self, ty: Type) -> Result<u32> {
            let ty = ty.result_value().unwrap_or(ty);
            if let Some(id) = self.indices.get(&ty) {
                return Ok(*id);
            }
            let node = match ty {
                Type::Number => CaptureType::I32,
                Type::Float64 => CaptureType::Float64,
                Type::Boolean => CaptureType::Boolean,
                Type::String => CaptureType::String,
                Type::Null => CaptureType::Null,
                Type::Undefined | Type::Void => CaptureType::Undefined,
                Type::BigInt => CaptureType::BigInt,
                Type::Error => CaptureType::Error { class: None },
                Type::NativeError(class) => CaptureType::Error {
                    class: Some(class as u32),
                },
                Type::Function(signature) => CaptureType::Function { signature },
                Type::Array(id) => CaptureType::Array {
                    element: self.node(self.types.arrays[id as usize])?,
                },
                Type::Class(_) => CaptureType::Class,
                Type::Generator(_) => CaptureType::Generator,
                Type::Promise(_) => CaptureType::Promise,
                Type::Record(id) => {
                    let mut fields = Vec::new();
                    for field in self.types.records[id as usize].fields.clone() {
                        fields.push(CaptureField {
                            name: field.name,
                            ty: self.node(field.ty)?,
                            optional: field.optional,
                        });
                    }
                    CaptureType::Record { fields }
                }
                Type::Union(id) | Type::ReferenceUnion(id) => {
                    let members = self.types.unions[id as usize]
                        .clone()
                        .into_iter()
                        .map(|ty| self.node(ty))
                        .collect::<Result<Vec<_>>>()?;
                    if ty.is_union() && matches!(ty, Type::ReferenceUnion(_)) {
                        CaptureType::ReferenceUnion { members }
                    } else {
                        CaptureType::PrimitiveUnion { members }
                    }
                }
                _ => {
                    return Err(CompilerError::Internal(
                        "unsupported private capture catalog type".into(),
                    ));
                }
            };
            let id = self.nodes.len() as u32;
            self.nodes.push(node);
            self.indices.insert(ty, id);
            Ok(id)
        }
    }
    let mut builder = Builder {
        types: &program.types,
        nodes: Vec::new(),
        indices: BTreeMap::new(),
    };
    let mut binding_rows = vec![None; bindings.len()];
    for (&ty, &id) in bindings {
        binding_rows[id as usize] = Some(CaptureBinding {
            ty: builder.node(ty)?,
            result: ty.result_value().is_some(),
        });
    }
    let mut functions = BTreeMap::new();
    for (&signature, codes) in targets {
        for &code in codes {
            let f = &program.functions[code];
            let captures = f
                .captures
                .iter()
                .map(|slot| bindings[&f.locals[*slot as usize]])
                .collect();
            let row = CaptureFunction {
                code: code as u32,
                signature,
                captures,
            };
            if functions.insert(code, row).is_some() {
                return Err(CompilerError::Internal(
                    "ambiguous private capture signature".into(),
                ));
            }
        }
    }
    FunctionCatalog {
        // Frame layouts exist only in the versioned frame catalog.
        version: if frames.is_empty() {
            varta_common::function::CATALOG_VERSION
        } else {
            varta_common::function::frame::FRAME_CATALOG_VERSION
        },
        value_version: program.types.value_version(),
        types: builder.nodes,
        bindings: binding_rows
            .into_iter()
            .map(|row| row.ok_or_else(|| CompilerError::Internal("missing capture binding".into())))
            .collect::<Result<_>>()?,
        functions: functions.into_values().collect(),
        frames,
    }
    .to_bytes()
    .map_err(|e| CompilerError::Invalid(e.to_string()))
}
