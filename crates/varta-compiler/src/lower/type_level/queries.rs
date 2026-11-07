//! `typeof x` type queries (TypeScript Handbook, Typeof Type Operator) give
//! module-scope values a type-level identity: a module constant is typed by
//! its annotation or, without one, by TypeScript's declaration type of its
//! initializer (a `const` primitive keeps its literal type, object and array
//! literals widen theirs, `as const` keeps them all as readonly literals and
//! tuples); a function by its signature; a class by its constructor type.
//! Queries follow value imports to the declaring module. A query of a
//! function-local binding or of an initializer whose type would need
//! lowering (a call result, an unannotated arrow) is refused with the name.
use super::*;
use crate::modules::{Graph, Unit};
use crate::source::Source;
use ir::RecordField;

#[derive(Clone)]
enum Value {
    Constant {
        declarator: Box<ast::VarDeclarator>,
        /// `const` (literal declaration types) rather than `let`/`var`.
        constant: bool,
        source: Source,
    },
    Function {
        declaration: Box<ast::FnDecl>,
        source: Source,
    },
    Import {
        module: usize,
        name: String,
    },
}

/// One expando assignment: the property name, the assigned value and its
/// module's source.
type Expando = (String, ast::Expr, Source);

#[derive(Default)]
pub(super) struct Values(
    BTreeMap<(usize, String), Value>,
    /// Expando properties of module functions (step 4, TypeScript's
    /// property assignments on a function declaration in its own scope):
    /// each `f.name = value` statement at module level, in source order.
    BTreeMap<(usize, String), Vec<Expando>>,
);

pub(super) fn register(module: usize, unit: &Unit, graph: &Graph, types: &mut ir::ValueTypes) {
    let values = &mut types.type_level.values.0;
    for item in &unit.ast.body {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(declaration)) => declaration,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            _ => continue,
        };
        match declaration {
            ast::Decl::Var(var) if !var.declare => {
                for declarator in &var.decls {
                    if let ast::Pat::Ident(binding) = &declarator.name {
                        values.insert(
                            (module, binding.id.sym.to_string()),
                            Value::Constant {
                                declarator: Box::new(declarator.clone()),
                                constant: var.kind == ast::VarDeclKind::Const,
                                source: unit.source.clone(),
                            },
                        );
                    }
                }
            }
            ast::Decl::Fn(function) if !function.declare => {
                values.insert(
                    (module, function.ident.sym.to_string()),
                    Value::Function {
                        declaration: Box::new(function.clone()),
                        source: unit.source.clone(),
                    },
                );
            }
            _ => {}
        }
    }
    // `f.name = value;` statements of the module's own functions (the
    // initializers of the constants `desugar::expandos` generates).
    for item in &unit.ast.body {
        let ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) = item else {
            continue;
        };
        let [declarator] = var.decls.as_slice() else {
            continue;
        };
        let Some(init) = declarator.init.as_deref() else {
            continue;
        };
        let Some(function) = crate::desugar::expando_target(init) else {
            continue;
        };
        let ast::Expr::Assign(assign) = init else {
            continue;
        };
        let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) = &assign.left
        else {
            continue;
        };
        let ast::MemberProp::Ident(property) = &member.prop else {
            continue;
        };
        let key = (module, function.to_owned());
        if !matches!(
            types.type_level.values.0.get(&key),
            Some(Value::Function { .. })
        ) {
            continue;
        }
        types.type_level.values.1.entry(key).or_default().push((
            property.sym.to_string(),
            (*assign.right).clone(),
            unit.source.clone(),
        ));
    }
    let values = &mut types.type_level.values.0;
    for import in &unit.imports {
        let Some(target) = graph.order.iter().position(|path| *path == import.target) else {
            continue;
        };
        for binding in import.bindings.iter().filter(|binding| !binding.type_only) {
            values
                .entry((module, binding.local.clone()))
                .or_insert(Value::Import {
                    module: target,
                    name: binding.imported.clone(),
                });
        }
    }
}

/// `typeof name` and `typeof name.property`.
pub(super) fn type_of(query: &ast::TsTypeQuery, types: &mut ir::ValueTypes) -> Result<Type> {
    if query.type_args.is_some() {
        return Err(unsupported(
            "instantiation expression in a typeof query (a generic function has no single function type)",
        ));
    }
    match &query.expr_name {
        ast::TsTypeQueryExpr::TsEntityName(name) => entity(name, types),
        ast::TsTypeQueryExpr::Import(_) => Err(unsupported(
            "typeof import(...) (import the module's values instead)",
        )),
    }
}

fn entity(name: &ast::TsEntityName, types: &mut ir::ValueTypes) -> Result<Type> {
    match name {
        ast::TsEntityName::Ident(id) => named_value(id.sym.as_ref(), types),
        ast::TsEntityName::TsQualifiedName(qualified) => {
            let object = entity(&qualified.left, types)?;
            let key = literals::string_set([qualified.right.sym.as_ref()], types)?;
            objects::index_type(object, key, types)
        }
    }
}

/// The declaration type of the module value `name` in the current module.
fn named_value(name: &str, types: &mut ir::ValueTypes) -> Result<Type> {
    // A function-local binding (in an annotation of the body being lowered,
    // not in a module-level declaration it resolves) shadows module values.
    if !named::resolving_declaration(types)
        && let Some(ty) = types
            .type_level
            .locals
            .as_ref()
            .and_then(|locals| locals.get(name))
    {
        return Ok(*ty);
    }
    if let Some(class) = named::class(types, name) {
        return constructors::class_constructor(class, types);
    }
    let mut key = (named::current(types), name.to_owned());
    let mut hops = 0;
    loop {
        let value = types.type_level.values.0.get(&key).cloned();
        match value {
            Some(Value::Import { module, name }) => {
                hops += 1;
                if hops > MAX_TYPE_DEPTH {
                    return Err(invalid(format!("typeof {name}: import chain too long")));
                }
                key = (module, name);
            }
            Some(value) => {
                let caller = named::current(types);
                named::enter(types, key.0);
                let result = nested(types, |t| declared(&key.1, &value, t));
                named::enter(types, caller);
                return result;
            }
            None => {
                return Err(type_error(format!(
                    "cannot find the value {name} of the typeof query"
                )));
            }
        }
    }
}

fn declared(name: &str, value: &Value, types: &mut ir::ValueTypes) -> Result<Type> {
    match value {
        Value::Function {
            declaration,
            source,
        } => {
            if declaration.function.type_params.is_some() {
                return Err(unsupported(&format!(
                    "typeof the generic function {name} (a generic function has no single function type)"
                )));
            }
            let signature = signature::check(declaration, 0, source, types)?;
            let call = functions::intern_full(
                signature.receiver,
                signature.params,
                signature.shape,
                signature.result,
                types,
            )?;
            let module = named::current(types);
            let fields = expando_fields(module, name, types)?;
            callables::with_properties(call, fields, types)
        }
        Value::Constant {
            declarator,
            constant,
            source,
        } => {
            let ast::Pat::Ident(binding) = &declarator.name else {
                return Err(invalid("missing constant name"));
            };
            if let Some(annotation) = &binding.type_ann {
                return keyword_type(&annotation.type_ann, types)
                    .map_err(|e| source.locate(e, annotation.span));
            }
            let init = declarator.init.as_deref().ok_or_else(|| {
                type_error(format!("typeof {name}: the declaration has no initializer"))
            })?;
            infer(init, Fresh::from(*constant), types).map_err(|e| source.locate(e, init.span()))
        }
        Value::Import { .. } => Err(invalid("unresolved import in a typeof query")),
    }
}

/// The expando properties of the module function `name` of `module` (with
/// their declaration types, widened as TypeScript's property assignment
/// declarations are; the first assignment of a name declares it).
pub(super) fn expando_fields(
    module: usize,
    name: &str,
    types: &mut ir::ValueTypes,
) -> Result<Vec<RecordField>> {
    let Some(assignments) = types
        .type_level
        .values
        .1
        .get(&(module, name.to_owned()))
        .cloned()
    else {
        return Ok(Vec::new());
    };
    let caller = named::current(types);
    named::enter(types, module);
    let mut fields: Vec<RecordField> = Vec::new();
    let mut result = Ok(());
    for (property, value, source) in assignments {
        if fields.iter().any(|field| field.name == property) {
            continue;
        }
        match infer(&value, Fresh::Widened, types) {
            Ok(ty) => fields.push(RecordField {
                name: property,
                ty,
                optional: false,
            }),
            Err(error) => {
                result = Err(source.locate(
                    unsupported(&format!(
                        "expando property {property} of function {name} whose value needs \
                         lowering to be typed ({error}); declare the function's type with the \
                         property instead"
                    )),
                    value.span(),
                ));
                break;
            }
        }
    }
    named::enter(types, caller);
    result.map(|()| fields)
}

/// The module and name a value `name` of the current module resolves to
/// through imports.
pub(super) fn declaring(name: &str, types: &ir::ValueTypes) -> Option<(usize, String)> {
    let mut key = (named::current(types), name.to_owned());
    for _ in 0..MAX_TYPE_DEPTH {
        match types.type_level.values.0.get(&key)? {
            Value::Import { module, name } => key = (*module, name.clone()),
            Value::Function { .. } => return Some(key),
            Value::Constant { .. } => return None,
        }
    }
    None
}

/// The type of `expression as const`.
pub(super) fn const_type(expression: &ast::Expr, types: &mut ir::ValueTypes) -> Option<Type> {
    infer(expression, Fresh::Const, types).ok()
}

/// How literal types of an initializer are kept.
#[derive(Clone, Copy, PartialEq)]
enum Fresh {
    /// A `const` declaration's own primitive keeps its literal type.
    Declaration,
    /// Literals widen (object properties, array elements, `let`).
    Widened,
    /// `as const`: every literal stays, arrays are readonly tuples.
    Const,
}
impl From<bool> for Fresh {
    fn from(constant: bool) -> Self {
        if constant {
            Self::Declaration
        } else {
            Self::Widened
        }
    }
}

fn literal(ty: Type, fresh: Fresh) -> Type {
    if fresh == Fresh::Widened {
        ty.erased()
    } else {
        ty
    }
}

/// The declaration type of an initializer expression.
fn infer(expression: &ast::Expr, fresh: Fresh, types: &mut ir::ValueTypes) -> Result<Type> {
    let refuse = |what: &str| {
        unsupported(&format!(
            "typeof query of a constant initialized by {what} (annotate the constant to give it a type-level identity)"
        ))
    };
    let literal_type = |lit: ast::TsLit, types: &mut ir::ValueTypes| -> Result<Type> {
        literals::annotation(
            &ast::TsLitType {
                span: expression.span(),
                lit,
            },
            types,
        )
    };
    Ok(match expression {
        ast::Expr::Paren(inner) => infer(&inner.expr, fresh, types)?,
        ast::Expr::Lit(ast::Lit::Str(text)) => {
            literal(literal_type(ast::TsLit::Str(text.clone()), types)?, fresh)
        }
        ast::Expr::Lit(ast::Lit::Num(number)) => literal(
            literal_type(ast::TsLit::Number(number.clone()), types)?,
            fresh,
        ),
        ast::Expr::Lit(ast::Lit::Bool(flag)) => {
            literal(literal_type(ast::TsLit::Bool(*flag), types)?, fresh)
        }
        ast::Expr::Lit(ast::Lit::Null(_)) => Type::Null,
        ast::Expr::Lit(ast::Lit::BigInt(_)) => Type::BigInt,
        ast::Expr::Tpl(template) if template.exprs.is_empty() => {
            let cooked = template.quasis[0]
                .cooked
                .clone()
                .ok_or_else(|| refuse("an invalid template"))?;
            let text = ast::Str {
                span: template.span,
                value: cooked,
                raw: None,
            };
            literal(literal_type(ast::TsLit::Str(text), types)?, fresh)
        }
        ast::Expr::Tpl(_) => Type::String,
        ast::Expr::Unary(unary)
            if unary.op == ast::UnaryOp::Minus
                && matches!(&*unary.arg, ast::Expr::Lit(ast::Lit::Num(_))) =>
        {
            let ast::Expr::Lit(ast::Lit::Num(number)) = &*unary.arg else {
                return Err(invalid("missing number literal"));
            };
            let negative = ast::Number {
                span: number.span,
                value: -number.value,
                raw: None,
            };
            literal(literal_type(ast::TsLit::Number(negative), types)?, fresh)
        }
        ast::Expr::Ident(id) if id.sym == "undefined" => Type::Undefined,
        ast::Expr::Ident(id) => {
            let ty = named_value(id.sym.as_ref(), types)?;
            if fresh == Fresh::Declaration {
                ty
            } else {
                literal(ty, Fresh::Widened)
            }
        }
        ast::Expr::TsConstAssertion(assertion) => infer(&assertion.expr, Fresh::Const, types)?,
        ast::Expr::TsAs(assertion) => keyword_type(&assertion.type_ann, types)?,
        ast::Expr::TsSatisfies(satisfies) => infer(&satisfies.expr, fresh, types)?,
        ast::Expr::Object(object) => {
            let inner = if fresh == Fresh::Const {
                Fresh::Const
            } else {
                Fresh::Widened
            };
            let mut fields = Vec::new();
            for property in &object.props {
                let ast::PropOrSpread::Prop(property) = property else {
                    return Err(refuse("an object spread"));
                };
                let (name, ty) = match &**property {
                    ast::Prop::KeyValue(pair) => {
                        let name = match &pair.key {
                            ast::PropName::Ident(id) => id.sym.to_string(),
                            ast::PropName::Str(text) => text
                                .value
                                .as_str()
                                .ok_or_else(|| refuse("a non-UTF-8 key"))?
                                .to_owned(),
                            ast::PropName::Num(number) => template_types::number_text(number.value),
                            _ => return Err(refuse("a computed key")),
                        };
                        (name, infer(&pair.value, inner, types)?)
                    }
                    ast::Prop::Shorthand(id) => {
                        let ty = named_value(id.sym.as_ref(), types)?;
                        (id.sym.to_string(), literal(ty, inner))
                    }
                    _ => return Err(refuse("an object literal method or accessor")),
                };
                fields.push(RecordField {
                    name: records::key(name)?,
                    ty,
                    optional: false,
                });
            }
            records::intern(fields, types)?
        }
        ast::Expr::Array(array) => {
            let inner = if fresh == Fresh::Const {
                Fresh::Const
            } else {
                Fresh::Widened
            };
            let mut elements = Vec::new();
            for element in &array.elems {
                let Some(element) = element else {
                    return Err(refuse("an array literal with holes"));
                };
                if element.spread.is_some() {
                    return Err(refuse("an array spread"));
                }
                elements.push(infer(&element.expr, inner, types)?);
            }
            if fresh == Fresh::Const {
                let count = elements.len();
                tuples::intern_rest(elements, count, None, true, types)?
            } else if elements.is_empty() {
                return Err(refuse("an empty array literal"));
            } else {
                let element = union_of(elements, types)?;
                arrays::intern(element, types)?
            }
        }
        ast::Expr::New(new) => match &*new.callee {
            ast::Expr::Ident(id) if named::class(types, id.sym.as_ref()).is_some() => {
                let reference = ast::TsTypeRef {
                    span: new.span,
                    type_name: ast::TsEntityName::Ident(id.clone()),
                    type_params: new.type_args.clone(),
                };
                keyword_type(&ast::TsType::TsTypeRef(reference), types)?
            }
            ast::Expr::Ident(id) if collections::builtin(id.sym.as_ref(), types).is_some() => {
                let reference = ast::TsTypeRef {
                    span: new.span,
                    type_name: ast::TsEntityName::Ident(id.clone()),
                    type_params: new.type_args.clone(),
                };
                if reference.type_params.is_none() {
                    return Err(refuse("a collection constructor without type arguments"));
                }
                keyword_type(&ast::TsType::TsTypeRef(reference), types)?
            }
            _ => return Err(refuse("a constructor call")),
        },
        ast::Expr::Arrow(arrow) => {
            if arrow.type_params.is_some() || arrow.is_async || arrow.is_generator {
                return Err(refuse("a generic, async or generator arrow function"));
            }
            let result = arrow
                .return_type
                .as_deref()
                .ok_or_else(|| refuse("an arrow function without a return type annotation"))?;
            let plans = parameters::analyze(&arrow.params, None, None, types, None)?;
            let shape = parameters::shape(&plans);
            let params = plans.iter().map(|p| p.incoming).collect();
            let result = annotation(Some(result), types)?
                .ok_or_else(|| invalid("missing arrow result type"))?;
            functions::intern_full(None, params, shape, result, types)?
        }
        ast::Expr::Call(call) => {
            if let ast::Callee::Expr(callee) = &call.callee
                && let ast::Expr::Ident(id) = &**callee
                && call.type_args.is_none()
                && let Type::Function(function) = named_value(id.sym.as_ref(), types)?
                && !types.functions[function as usize].construct
            {
                types.functions[function as usize].result
            } else {
                return Err(refuse("a call of something other than a module function"));
            }
        }
        _ => return Err(refuse("this expression")),
    })
}

/// Whether module `module` declares (or imports) `name` as an unannotated
/// `const` initialized by a primitive literal: a widening literal type.
pub(super) fn literal_constant(types: &ir::ValueTypes, module: usize, name: &str) -> bool {
    let mut key = (module, name.to_owned());
    for _ in 0..MAX_TYPE_DEPTH {
        match types.type_level.values.0.get(&key) {
            Some(Value::Import { module, name }) => key = (*module, name.clone()),
            Some(Value::Constant {
                declarator,
                constant: true,
                ..
            }) => {
                let annotated = matches!(&declarator.name, ast::Pat::Ident(binding) if binding.type_ann.is_some());
                return !annotated
                    && declarator.init.as_deref().is_some_and(|init| {
                        matches!(
                            stmt::unparen(init),
                            ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_) | ast::Lit::Bool(_))
                        ) || matches!(stmt::unparen(init), ast::Expr::Tpl(t) if t.exprs.is_empty())
                            || matches!(stmt::unparen(init), ast::Expr::Unary(u)
                                if u.op == ast::UnaryOp::Minus
                                    && matches!(&*u.arg, ast::Expr::Lit(ast::Lit::Num(_))))
                    });
            }
            _ => return false,
        }
    }
    false
}

impl Lower<'_> {
    /// Make the visible local bindings' types available to `typeof` queries
    /// while `statement` is lowered; the previous snapshot is returned.
    pub(in crate::lower) fn enter_local_types(
        &mut self,
        statement: &ast::Stmt,
    ) -> Option<Option<BTreeMap<String, Type>>> {
        use swc_ecma_visit::{Visit, VisitWith};
        struct Query(bool);
        impl Visit for Query {
            fn visit_ts_type_query(&mut self, _: &ast::TsTypeQuery) {
                self.0 = true;
            }
        }
        if !self.types.profile.type_level() {
            return None;
        }
        let mut query = Query(false);
        statement.visit_with(&mut query);
        if !query.0 {
            return None;
        }
        let mut locals = BTreeMap::new();
        for (name, binding) in &self.outer {
            if let Some(ty) = binding.ty {
                locals.insert(name.clone(), ty);
            }
        }
        for scope in &self.scopes {
            for (name, binding) in scope {
                if let Some(ty) = binding.ty
                    && !name.starts_with('#')
                {
                    locals.insert(name.clone(), ty);
                }
            }
        }
        Some(self.types.type_level.locals.replace(locals))
    }
    pub(in crate::lower) fn leave_local_types(
        &mut self,
        outer: Option<Option<BTreeMap<String, Type>>>,
    ) {
        if let Some(outer) = outer {
            self.types.type_level.locals = outer;
        }
    }
}
