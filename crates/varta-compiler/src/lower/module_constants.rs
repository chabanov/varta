//! Module-scope `const` bindings (v32). Initializers are a checked pure subset,
//! evaluated once per instance in dependency order before `main`. Number,
//! boolean, null and undefined constants must fold to literals and are inlined;
//! every other value lives in a permanent root slot that the collector traces for
//! the program lifetime. Functions read constants through immutable locals.
//!
//! Constants share the value export table with functions (`link::Value`), so
//! explicit and star re-exports forward them under the common rules.
use super::*;
use crate::modules::Unit;
use std::collections::BTreeSet;
use std::path::PathBuf;
use swc_ecma_visit::{Visit, VisitWith};

#[derive(Clone)]
pub(super) struct Constant {
    ty: Type,
    value: Value,
    readonly: bool,
    /// Graph-unique declaration identity: two star re-export sources that
    /// forward the same declaration are not ambiguous.
    id: usize,
}
impl Constant {
    pub(super) fn id(&self) -> usize {
        self.id
    }
}
#[derive(Clone, Copy)]
enum Value {
    Integer(i32),
    Float64(f64),
    Stored(u32),
}
/// Constants visible in one module by local name: declarations and imports.
pub(super) type Scope = BTreeMap<String, Constant>;

#[derive(Default)]
pub(super) struct Linked {
    pub scopes: BTreeMap<PathBuf, Scope>,
    /// Initializer functions in dependency order; `main` calls them first.
    pub initializers: Vec<usize>,
    pub stored: u32,
    /// Constant declarations linked so far, the source of [`Constant::id`].
    declared: usize,
}

type Synthesized<'a> = (
    &'a mut BTreeMap<usize, ir::Function>,
    &'a mut usize,
    &'a mut BTreeMap<usize, flow_calls::Impact>,
);

/// Top-level non-ambient `const` declarations, exported or not.
fn declarations(module: &ast::Module) -> Vec<&ast::VarDecl> {
    module
        .body
        .iter()
        .filter_map(|item| match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) => Some(&**var),
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
                match &export.decl {
                    ast::Decl::Var(var) => Some(&**var),
                    _ => None,
                }
            }
            _ => None,
        })
        .filter(|var| !var.declare && var.kind == ast::VarDeclKind::Const)
        .collect()
}

#[derive(Default)]
struct Names(BTreeSet<String>);
impl Visit for Names {
    fn visit_ident(&mut self, id: &ast::Ident) {
        self.0.insert(id.sym.to_string());
    }
}
fn referenced(node: &impl VisitWith<Names>) -> BTreeSet<String> {
    let mut names = Names::default();
    node.visit_with(&mut names);
    names.0
}

/// Link the constants of one module, after its dependencies. `scope` holds the
/// constants it imports, already resolved through the value export tables.
/// Declarations with stored values are lowered into synthesized initializer
/// functions of bounded size. The module's scope is recorded in `linked`.
pub(super) fn link_module(
    (unit, path): (&Unit, &PathBuf),
    functions: &BTreeMap<String, Signature>,
    mut scope: Scope,
    (types, entry): (&mut ir::ValueTypes, usize),
    synthesized: Synthesized<'_>,
    linked: &mut Linked,
) -> Result<()> {
    if !types.profile.module_constants() {
        return Ok(());
    }
    let (generated, next, call_effects) = synthesized;
    let source = &unit.source;
    let declared = declarations(&unit.ast);
    check(source, functions, &scope, &declared)?;
    for chunk in chunks(&declared) {
        let index = *next;
        let mut lower = Lower {
            source,
            types,
            functions,
            scopes: vec![BTreeMap::new()],
            next: 0,
            local_types: BTreeMap::new(),
            refinements: BTreeMap::new(),
            writes: Default::default(),
            call_writes: Default::default(),
            call_unknown: false,
            direct_writes: Default::default(),
            call_parameters: Default::default(),
            parameter_values: Default::default(),
            parameter_roots: Default::default(),
            known_calls: Default::default(),
            call_effects,
            reachable: true,
            control: Default::default(),
            guard_blocked: Default::default(),
            outer_narrows: Default::default(),
            result: Type::Void,
            infer_returns: false,
            entry,
            loops: 0,
            labels: Vec::new(),
            next_label: 0,
            switches: 0,
            depth: 0,
            repeat_slots: None,
            generated,
            next_function: next,
            index,
            outer: Default::default(),
            captures: Default::default(),
            cells: Default::default(),
            guarded: Default::default(),
            generator: None,
        };
        let (body, locals) =
            lower.initializers(chunk, &scope, (&mut linked.stored, &mut linked.declared))?;
        let stored = body
            .iter()
            .any(|statement| matches!(statement, S::InitModuleConstant(..)));
        let span = chunk[0].span;
        let function = ir::Function {
            shape: ir::ParameterShape::fixed(0),
            self_slot: None,
            receiver: None,
            cells: lower.cells,
            captures: vec![],
            environment: false,
            generator: None,
            source: source.clone(),
            span,
            params: vec![],
            result: Type::Void,
            locals: (0..lower.next)
                .map(|slot| {
                    lower
                        .local_types
                        .get(&slot)
                        .copied()
                        .unwrap_or(Type::Number)
                })
                .collect(),
            body,
        };
        if stored {
            if index >= varta_common::abi::MAX_USER_FUNCTIONS {
                return Err(source.locate(
                    invalid("too many functions including module initializers"),
                    span,
                ));
            }
            *next += 1;
            generated.insert(index, function);
            linked.initializers.push(index);
        }
        scope.extend(locals);
    }
    linked.scopes.insert(path.clone(), scope);
    Ok(())
}

/// Conservative upper estimate of emitted code per initializer weight unit
/// (about four times the bytes measured for one string array element).
const EMITTED_BYTES_PER_NODE: usize = 256;
/// Weight budget of one initializer function, so that a module with many
/// constants is split before any initializer can exceed the body budget.
const INITIALIZER_WEIGHT: usize =
    varta_common::abi::MAX_WASM_FUNCTION_BODY_BYTES / EMITTED_BYTES_PER_NODE;

#[derive(Default)]
struct Weight(usize);
impl Visit for Weight {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        self.0 += 1;
        expr.visit_children_with(self);
    }
}
type Declared<'a> = &'a ast::VarDecl;
/// Split declarations into initializer chunks of bounded weight; a declaration
/// is never split, so one oversized declaration forms its own chunk.
fn chunks<'d, 'a>(declared: &'d [Declared<'a>]) -> Vec<&'d [Declared<'a>]> {
    let mut chunks = Vec::new();
    let (mut start, mut weight) = (0, 0);
    for (index, var) in declared.iter().enumerate() {
        let mut nodes = Weight(var.decls.len());
        var.visit_with(&mut nodes);
        if index > start && weight + nodes.0 > INITIALIZER_WEIGHT {
            chunks.push(&declared[start..index]);
            (start, weight) = (index, 0);
        }
        weight += nodes.0;
    }
    if start < declared.len() {
        chunks.push(&declared[start..]);
    }
    chunks
}

/// Validate bindings and initializer forms of one module before lowering. A
/// reference to a constant that is declared later (or to itself) is a
/// temporal-dead-zone error.
fn check(
    source: &Source,
    functions: &BTreeMap<String, Signature>,
    imported: &Scope,
    declared: &[Declared<'_>],
) -> Result<()> {
    let mut names: BTreeSet<String> = imported.keys().cloned().collect();
    for var in declared {
        for declarator in &var.decls {
            let ast::Pat::Ident(binding) = &declarator.name else {
                return Err(source.locate(
                    unsupported("top-level destructuring declaration"),
                    declarator.name.span(),
                ));
            };
            let name = binding.id.sym.as_ref();
            if functions.contains_key(name) || imported.contains_key(name) {
                return Err(source.locate(
                    invalid(format!("duplicate module binding: {name}")),
                    binding.id.span,
                ));
            }
            if declarator.init.is_none() {
                return Err(source.locate(
                    invalid("module constant requires an initializer"),
                    declarator.span,
                ));
            }
            names.insert(name.to_owned());
        }
    }
    let mut ready: BTreeSet<String> = imported.keys().cloned().collect();
    for var in declared {
        for declarator in &var.decls {
            let (Some(init), ast::Pat::Ident(binding)) =
                (declarator.init.as_deref(), &declarator.name)
            else {
                continue;
            };
            let value = match stmt::unparen(init) {
                ast::Expr::TsConstAssertion(assertion) => &assertion.expr,
                _ => init,
            };
            initializer_form(value, &names).map_err(|e| source.locate(e, value.span()))?;
            if let Some(early) = referenced(value)
                .into_iter()
                .find(|name| names.contains(name) && !ready.contains(name))
            {
                return Err(source.locate(
                    invalid(format!("binding used before initialization: {early}")),
                    value.span(),
                ));
            }
            ready.insert(binding.id.sym.to_string());
        }
    }
    Ok(())
}

/// Why an initializer leaves the admitted pure subset, naming the construct.
fn initializer_form(expr: &ast::Expr, names: &BTreeSet<String>) -> Result<()> {
    let form = match expr {
        ast::Expr::Lit(
            ast::Lit::Str(_)
            | ast::Lit::Num(_)
            | ast::Lit::Bool(_)
            | ast::Lit::Null(_)
            | ast::Lit::BigInt(_),
        ) => return Ok(()),
        ast::Expr::Ident(id) => {
            let name = id.sym.as_ref();
            if names.contains(name) || matches!(name, "undefined" | "NaN" | "Infinity") {
                return Ok(());
            }
            return Err(unsupported(&format!(
                "reference to {name} in a module constant initializer; only module constants are admitted"
            )));
        }
        ast::Expr::Paren(paren) => return initializer_form(&paren.expr, names),
        ast::Expr::Unary(unary)
            if matches!(unary.op, ast::UnaryOp::Minus | ast::UnaryOp::Plus)
                && matches!(
                    stmt::unparen(&unary.arg),
                    ast::Expr::Lit(ast::Lit::Num(_) | ast::Lit::BigInt(_))
                ) =>
        {
            return Ok(());
        }
        ast::Expr::Bin(binary) if binary.op == ast::BinaryOp::Add => {
            initializer_form(&binary.left, names)?;
            return initializer_form(&binary.right, names);
        }
        ast::Expr::Array(array) => {
            for element in &array.elems {
                match element {
                    Some(ast::ExprOrSpread { spread: None, expr }) => {
                        initializer_form(expr, names)?
                    }
                    Some(_) => return Err(unsupported("spread in a module constant initializer")),
                    None => return Err(unsupported("array hole in a module constant initializer")),
                }
            }
            return Ok(());
        }
        ast::Expr::Object(object) => {
            for property in &object.props {
                match property {
                    ast::PropOrSpread::Prop(prop) => match &**prop {
                        ast::Prop::KeyValue(pair)
                            if matches!(
                                pair.key,
                                ast::PropName::Ident(_) | ast::PropName::Str(_)
                            ) =>
                        {
                            initializer_form(&pair.value, names)?
                        }
                        ast::Prop::Shorthand(id) => {
                            initializer_form(&ast::Expr::Ident(id.clone()), names)?
                        }
                        _ => {
                            return Err(unsupported(
                                "object member form in a module constant initializer",
                            ));
                        }
                    },
                    ast::PropOrSpread::Spread(_) => {
                        return Err(unsupported("spread in a module constant initializer"));
                    }
                }
            }
            return Ok(());
        }
        ast::Expr::TsConstAssertion(_) => "nested as const",
        ast::Expr::Call(_) | ast::Expr::New(_) | ast::Expr::OptChain(_) => "call",
        ast::Expr::Member(_) => "member access",
        ast::Expr::Tpl(_) | ast::Expr::TaggedTpl(_) => "template literal",
        ast::Expr::Arrow(_) | ast::Expr::Fn(_) | ast::Expr::Class(_) => "function or class",
        ast::Expr::Assign(_) | ast::Expr::Update(_) => "assignment",
        ast::Expr::Unary(_) => "unary operator",
        ast::Expr::Bin(_) => "binary operator other than +",
        _ => "expression form",
    };
    Err(unsupported(&format!(
        "{form} in a module constant initializer"
    )))
}

/// Literal value of a folded scalar initializer, if it has one.
fn fold(value: &Expr, folded: &BTreeMap<u32, Value>) -> Option<Value> {
    match &value.kind {
        E::Integer(n) => Some(Value::Integer(*n)),
        E::Float64(n) => Some(Value::Float64(*n)),
        E::Negative(inner) => match fold(inner, folded)? {
            Value::Integer(n) => n.checked_neg().map(Value::Integer),
            Value::Float64(n) => Some(Value::Float64(-n)),
            Value::Stored(_) => None,
        },
        E::Local(slot) => folded.get(slot).copied(),
        _ => None,
    }
}

impl Lower<'_> {
    /// Lower one chunk of constant declarations into an initializer body that
    /// publishes every reference value to its permanent slot. `visible` holds the
    /// imported constants and those of earlier chunks, already initialized.
    fn initializers(
        &mut self,
        declared: &[Declared<'_>],
        visible: &Scope,
        (stored, ids): (&mut u32, &mut usize),
    ) -> Result<(Vec<S>, Scope)> {
        let mut used = BTreeSet::new();
        for var in declared {
            for declarator in &var.decls {
                if let Some(init) = declarator.init.as_deref() {
                    used.extend(referenced(init));
                }
            }
        }
        let mut body = self.materialize(visible, &used, swc_common::DUMMY_SP)?;
        let mut folded = BTreeMap::new();
        for statement in &body {
            if let S::Init(slot, value) = statement
                && let Some(literal) = fold(value, &folded)
            {
                folded.insert(*slot, literal);
            }
        }
        for var in declared {
            self.predeclare(var)
                .map_err(|e| self.source.locate(e, var.span()))?;
        }
        let mut locals = Scope::new();
        for var in declared {
            for (declarator, statement) in var.decls.iter().zip(self.variables(var)?) {
                let ast::Pat::Ident(name) = &declarator.name else {
                    return Err(invalid("missing module constant binding"));
                };
                let S::Init(slot, value) = statement else {
                    return Err(invalid("missing module constant initializer"));
                };
                let binding = self.binding(name.id.sym.as_ref())?;
                let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
                let span = value.span;
                let constant = if ty.needs_root() {
                    let index = *stored;
                    *stored += 1;
                    body.push(S::Init(slot, value));
                    body.push(S::InitModuleConstant(
                        index,
                        Expr {
                            span,
                            ty,
                            kind: E::Local(slot),
                        },
                    ));
                    Value::Stored(index)
                } else if let Some(literal) = fold(&value, &folded) {
                    folded.insert(slot, literal);
                    body.push(S::Init(slot, value));
                    literal
                } else {
                    return Err(self.source.locate(
                        unsupported(&format!(
                            "non-literal {ty} module constant initializer; number, boolean, null and undefined constants must be literals"
                        )),
                        span,
                    ));
                };
                locals.insert(
                    name.id.sym.to_string(),
                    Constant {
                        ty,
                        value: constant,
                        readonly: binding.readonly,
                        id: *ids,
                    },
                );
                *ids += 1;
            }
        }
        Ok((body, locals))
    }
    /// Bind the module constants that `names` mentions as immutable locals in a
    /// new outermost scope, so parameters and locals shadow them. Parameters must
    /// already be reserved: their slots are the leading Wasm locals.
    fn materialize(
        &mut self,
        scope: &Scope,
        names: &BTreeSet<String>,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let mut prologue = Vec::new();
        let mut bindings = BTreeMap::new();
        for (name, constant) in scope.iter().filter(|(name, _)| names.contains(*name)) {
            let slot = self.hidden(constant.ty);
            bindings.insert(
                name.clone(),
                Binding {
                    identity: (self.index, slot),
                    slot,
                    ty: Some(constant.ty),
                    initialized: true,
                    mutable: false,
                    readonly: constant.readonly,
                },
            );
            let kind = match constant.value {
                Value::Integer(n) => E::Integer(n),
                Value::Float64(n) => E::Float64(n),
                Value::Stored(index) => E::ModuleConstant(index),
            };
            prologue.push(S::Init(
                slot,
                Expr {
                    span,
                    ty: constant.ty,
                    kind,
                },
            ));
        }
        self.scopes.insert(0, bindings);
        Ok(prologue)
    }
    /// Materialize the constants a function declaration mentions.
    pub(super) fn module_constants(
        &mut self,
        scope: &Scope,
        function: &ast::Function,
    ) -> Result<Vec<S>> {
        if scope.is_empty() {
            return Ok(vec![]);
        }
        self.materialize(scope, &referenced(function), function.span)
    }
    /// `as const` around a whole declaration initializer (v32): the value keeps
    /// its element and field types and the binding becomes readonly.
    pub(super) fn const_assertion<'e>(&self, init: &'e ast::Expr) -> (&'e ast::Expr, bool) {
        match stmt::unparen(init) {
            ast::Expr::TsConstAssertion(assertion) if self.types.profile.module_constants() => {
                (&assertion.expr, true)
            }
            _ => (init, false),
        }
    }
    pub(super) fn mark_const_assertion(&mut self, name: &str, readonly: bool) -> Result<()> {
        if readonly {
            let binding = self
                .scopes
                .iter_mut()
                .rev()
                .find_map(|scope| scope.get_mut(name))
                .ok_or_else(|| invalid("missing binding"))?;
            binding.readonly = true;
        }
        Ok(())
    }
    /// Reject a write whose base is reached from a readonly `as const` binding.
    pub(super) fn reject_readonly_write(&self, mut base: &ast::Expr) -> Result<()> {
        loop {
            base = match base {
                ast::Expr::Paren(paren) => &paren.expr,
                ast::Expr::Member(member) => &member.obj,
                ast::Expr::TsNonNull(value) => &value.expr,
                ast::Expr::Ident(id) => {
                    if self.binding(id.sym.as_ref()).is_ok_and(|b| b.readonly) {
                        return Err(type_error(format!(
                            "cannot modify readonly `as const` value {}",
                            id.sym
                        )));
                    }
                    return Ok(());
                }
                _ => return Ok(()),
            };
        }
    }
}
