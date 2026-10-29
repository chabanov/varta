//! Interface inheritance and declaration merging (v32 `interface_extends()`).
//! `interface B extends A, C { … }` is the closed record whose fields are the
//! own members of every merged `interface B` declaration of the module plus
//! the fields of each base. Bases are resolved like annotations in the
//! declaring module (local, imported, or aliases of closed record types), so
//! interfaces from other modules and cycles follow the v31 rules. The result is
//! an ordinary structural record shape: it adds no layout, tag or subtyping.
use super::*;
use ir::RecordField;
use swc_common::Span;

/// One `interface` declaration of a merged interface.
#[derive(Clone)]
struct Part {
    members: Vec<ast::TsTypeElement>,
    extends: Vec<ast::TsExprWithTypeArgs>,
    span: Span,
    exported: bool,
}

/// Every declaration of one interface name in one module, in source order.
#[derive(Clone)]
pub(super) struct Interface {
    name: String,
    source: Source,
    parts: Vec<Part>,
    /// Type parameters of a generic (v32 `generics()`) interface. A generic
    /// interface has exactly one declaration.
    params: Option<Box<ast::TsTypeParamDecl>>,
}

/// Interfaces of one module by name.
pub(super) type Merged = BTreeMap<String, Interface>;

/// Add one interface declaration of the module to `merged`. Declaration
/// merging of generic interfaces is rejected: TypeScript requires identical
/// type parameter lists, which this subset does not compare.
pub(super) fn collect(
    merged: &mut Merged,
    interface: &ast::TsInterfaceDecl,
    exported: bool,
    source: &Source,
) -> Result<()> {
    let name = interface.id.sym.to_string();
    let part = Part {
        members: interface.body.body.clone(),
        extends: interface.extends.clone(),
        span: interface.span,
        exported,
    };
    let entry = merged.entry(name.clone()).or_insert_with(|| Interface {
        name: name.clone(),
        source: source.clone(),
        parts: Vec::new(),
        params: interface.type_params.clone(),
    });
    if !entry.parts.is_empty() && (entry.params.is_some() || interface.type_params.is_some()) {
        return Err(source.locate(
            unsupported(&format!("declaration merging of generic interface {name}")),
            interface.id.span,
        ));
    }
    if entry.parts.iter().any(|p| p.exported != exported) {
        return Err(source.locate(
            invalid(format!(
                "declarations of merged interface {name} must be all exported or all local"
            )),
            interface.id.span,
        ));
    }
    entry.parts.push(part);
    Ok(())
}

impl Interface {
    pub(super) fn span(&self) -> Span {
        self.parts[0].span
    }
    pub(super) fn source(&self) -> &Source {
        &self.source
    }
    pub(super) fn params(&self) -> Option<&ast::TsTypeParamDecl> {
        self.params.as_deref()
    }
    /// Type parameters, the type literal of the own members and each base
    /// reference (`A<T>`), used to infer generic call arguments through a
    /// generic interface with bases.
    pub(super) fn generic_patterns(&self) -> Option<(Box<ast::TsTypeParamDecl>, Vec<ast::TsType>)> {
        let params = self.params.clone()?;
        let members = self.parts.iter().flat_map(|p| p.members.clone()).collect();
        let mut patterns = vec![ast::TsType::TsTypeLit(ast::TsTypeLit {
            span: self.span(),
            members,
        })];
        for base in self.parts.iter().flat_map(|p| &p.extends) {
            if let ast::Expr::Ident(id) = &*base.expr {
                patterns.push(base_reference(base, id));
            }
        }
        Some((params, patterns))
    }
    /// A single declaration without `extends` keeps the v31 representation: a
    /// type literal resolved by the ordinary annotation rules.
    pub(super) fn plain(&self) -> Option<ast::TsType> {
        match self.parts.as_slice() {
            [part] if part.extends.is_empty() => Some(ast::TsType::TsTypeLit(ast::TsTypeLit {
                span: part.span,
                members: part.members.clone(),
            })),
            _ => None,
        }
    }

    /// The combined closed record type. Runs in the declaring module, with the
    /// type parameters of a generic interface bound in the current frame, so
    /// own members and base arguments (`extends A<T>`) see them.
    pub(super) fn resolve(&self, types: &mut ir::ValueTypes) -> Result<Type> {
        let own = self.own_fields(types)?;
        let mut inherited: Vec<(RecordField, String)> = Vec::new();
        for part in &self.parts {
            for base in &part.extends {
                let (base_name, fields) = self
                    .base(base, types)
                    .map_err(|e| self.source.locate(e, base.span))?;
                for field in fields {
                    if let Some((own, span)) = own.iter().find(|(f, _)| f.name == field.name) {
                        self.redeclaration(own, &field, &base_name, types)
                            .map_err(|e| self.source.locate(e, *span))?;
                    } else if let Some((existing, other)) =
                        inherited.iter().find(|(f, _)| f.name == field.name)
                    {
                        if existing != &field {
                            return Err(self.source.locate(
                                type_error(format!(
                                    "interface {} inherits field {} from {other} and {base_name} with different types",
                                    self.name, field.name
                                )),
                                base.span,
                            ));
                        }
                    } else {
                        inherited.push((field, base_name.clone()));
                    }
                }
            }
        }
        let fields = own
            .into_iter()
            .map(|(field, _)| field)
            .chain(inherited.into_iter().map(|(field, _)| field))
            .collect();
        records::intern(fields, types).map_err(|e| self.source.locate(e, self.span()))
    }

    /// Own members of every declaration with their member spans. A merged
    /// redeclaration must repeat the field exactly (TypeScript rule).
    fn own_fields(&self, types: &mut ir::ValueTypes) -> Result<Vec<(RecordField, Span)>> {
        let mut own: Vec<(RecordField, Span)> = Vec::new();
        for part in &self.parts {
            let fields = records::type_fields(&part.members, types)
                .map_err(|e| self.source.locate(e, part.span))?;
            let first = own.len();
            for (field, member) in fields.into_iter().zip(&part.members) {
                let span = member.span();
                match own.iter().position(|(f, _)| f.name == field.name) {
                    Some(index) if index >= first => {
                        return Err(self
                            .source
                            .locate(unsupported("duplicate object key"), span));
                    }
                    Some(index) if own[index].0 != field => {
                        return Err(self.source.locate(
                            type_error(format!(
                                "merged declarations of interface {} declare field {} with different types",
                                self.name, field.name
                            )),
                            span,
                        ));
                    }
                    Some(_) => {}
                    None => own.push((field, span)),
                }
            }
        }
        Ok(own)
    }

    /// The fields of one `extends` entry, which must name a closed record type.
    fn base(
        &self,
        base: &ast::TsExprWithTypeArgs,
        types: &mut ir::ValueTypes,
    ) -> Result<(String, Vec<RecordField>)> {
        let ast::Expr::Ident(id) = &*base.expr else {
            return Err(unsupported(
                "interface extends a qualified or computed base",
            ));
        };
        // Resolve `A<…>` exactly like the annotation `A<…>` would be.
        let reference = base_reference(base, id);
        let name = id.sym.to_string();
        match keyword_type(&reference, types)? {
            Type::Record(record) => Ok((name, types.records[record as usize].fields.clone())),
            Type::Class(_) => Err(unsupported(&format!(
                "interface {} extends class {name}; class instance types are nominal",
                self.name
            ))),
            _ => Err(unsupported(&format!(
                "interface {} extends {name}, which is not a closed record type",
                self.name
            ))),
        }
    }

    /// An own member may redeclare an inherited field with a type assignable
    /// to it (e.g. a literal or union member of the base type), and may make an
    /// optional field required, but never the reverse.
    fn redeclaration(
        &self,
        own: &RecordField,
        base: &RecordField,
        base_name: &str,
        types: &ir::ValueTypes,
    ) -> Result<()> {
        if own.optional && !base.optional {
            return Err(type_error(format!(
                "field {} of interface {} is optional but required in {base_name}",
                own.name, self.name
            )));
        }
        if !arguments::accepts(base.ty, own.ty, types) {
            return Err(type_error(format!(
                "field {} of interface {} is not assignable to the field inherited from {base_name}",
                own.name, self.name
            )));
        }
        Ok(())
    }
}

/// The annotation `A<…>` equivalent to the `extends A<…>` entry `base`.
fn base_reference(base: &ast::TsExprWithTypeArgs, id: &ast::Ident) -> ast::TsType {
    ast::TsType::TsTypeRef(ast::TsTypeRef {
        span: base.span,
        type_name: ast::TsEntityName::Ident(id.clone()),
        type_params: base.type_args.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{Graph, ModuleSource, NoImports};
    use varta_common::language::LanguageProfile;

    fn lower(text: &str) -> Result<ir::Program> {
        let graph = Graph::load_with_profile(
            ModuleSource {
                path: "main.ts".into(),
                text: format!("{text}export function main():void{{}}"),
            },
            &NoImports,
            LanguageProfile::CheckedF64Utf16V32,
        )?;
        super::super::lower(&graph, LanguageProfile::CheckedF64Utf16V32)
    }

    #[test]
    fn extended_interfaces_intern_one_structural_record_shape() {
        let program = lower(
            "interface A{x:number;y?:string}interface B extends A{}\
             interface C extends A{y:string}interface C{z:boolean}\
             type L={y?:string;x:number};type M={x:number;y:string;z:boolean};\
             function f(a:A,b:B,l:L,c:C,m:M):void{}",
        )
        .unwrap();
        let params = &program.functions[0].params;
        // B adds nothing, so it is the shape of A and of the equal literal L.
        assert_eq!((params[0], params[1]), (params[2], params[2]));
        // C redeclares `y` as required and merges `z` from a second declaration.
        assert_eq!(params[3], params[4]);
        let Type::Record(id) = params[3] else {
            panic!("record expected");
        };
        let fields = &program.types.records[id as usize].fields;
        assert_eq!(
            fields
                .iter()
                .map(|f| (f.name.as_str(), f.optional))
                .collect::<Vec<_>>(),
            [("x", false), ("y", false), ("z", false)]
        );
    }

    #[test]
    fn invalid_redeclarations_and_bases_point_at_their_source() {
        for (source, target) in [
            (
                "interface A{x:number}interface B extends A{y:number;x:string}function f(b:B):void{}",
                "x:string",
            ),
            (
                "interface A{x:number}interface B extends Missing,A{}function f(b:B):void{}",
                "Missing",
            ),
        ] {
            let error = lower(source).err().unwrap();
            let diagnostic = error.diagnostic().unwrap();
            let range = diagnostic.range.start.byte_offset..diagnostic.range.end.byte_offset;
            assert_eq!(&source[range], target, "{error}");
        }
    }
}
