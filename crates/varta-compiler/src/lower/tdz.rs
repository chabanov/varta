//! Temporal dead zones of closure captures (v37 `language_core`).
//!
//! A closure may capture a `let`/`const` binding or a hoisted nested function
//! before the binding's own initialization runs, as in JavaScript: the
//! capture is legal, and a read or write through the closure before the
//! initialization throws a `ReferenceError`. Only bindings that a syntactic
//! scan finds referenced by a closure at or before their declaration get the
//! run-time state, so every other binding keeps its storage and code:
//!
//! - their binding cell is allocated with a zero payload and a separate
//!   Boolean flag cell is cleared at the entry of the declaring block;
//! - their initialization stores into that cell and sets the flag;
//! - a closure that captured the binding uninitialized captures the flag too
//!   and tests it before every read or write of the binding.
//!
//! Reads in the declaring function itself stay statically checked: a direct
//! read before the initialization is still the compile-time error
//! `binding used before initialization`.
use super::*;
use std::collections::BTreeSet;
use swc_ecma_visit::{Visit, VisitWith};

/// The message of the `ReferenceError` Node throws for a TDZ access.
fn message(name: &str) -> String {
    format!("Cannot access '{name}' before initialization")
}
/// The `name` of the thrown Error-class instance.
const REFERENCE_ERROR: &str = "ReferenceError";

/// Identifiers mentioned inside function bodies (closures) of a node.
#[derive(Default)]
struct ClosureNames {
    depth: usize,
    names: BTreeSet<String>,
}
impl Visit for ClosureNames {
    fn visit_ts_type(&mut self, _: &ast::TsType) {}
    fn visit_function(&mut self, node: &ast::Function) {
        self.depth += 1;
        node.visit_children_with(self);
        self.depth -= 1;
    }
    fn visit_arrow_expr(&mut self, node: &ast::ArrowExpr) {
        self.depth += 1;
        node.visit_children_with(self);
        self.depth -= 1;
    }
    fn visit_constructor(&mut self, node: &ast::Constructor) {
        self.depth += 1;
        node.visit_children_with(self);
        self.depth -= 1;
    }
    fn visit_getter_prop(&mut self, node: &ast::GetterProp) {
        self.depth += 1;
        node.visit_children_with(self);
        self.depth -= 1;
    }
    fn visit_setter_prop(&mut self, node: &ast::SetterProp) {
        self.depth += 1;
        node.visit_children_with(self);
        self.depth -= 1;
    }
    fn visit_ident(&mut self, node: &ast::Ident) {
        if self.depth > 0 {
            self.names.insert(node.sym.to_string());
        }
    }
}

/// Names declared by `statements` (lexical declarations and nested function
/// declarations) that a closure created at or before the declaration's
/// position mentions. Nested function declarations are hoisted to the block
/// entry in source order, before every other statement. The scan ignores
/// shadowing, so it may over-approximate; that only costs a flag cell.
pub(super) fn early_names(statements: &[ast::Stmt]) -> BTreeSet<String> {
    let functions: Vec<&ast::FnDecl> = statements
        .iter()
        .filter_map(|statement| match statement {
            ast::Stmt::Decl(ast::Decl::Fn(function)) => Some(function),
            _ => None,
        })
        .collect();
    let mut referenced = BTreeSet::new();
    let mut early = BTreeSet::new();
    for function in &functions {
        let mut names = ClosureNames {
            depth: 1,
            ..Default::default()
        };
        function.function.visit_with(&mut names);
        referenced.extend(names.names);
        if referenced.contains(function.ident.sym.as_ref()) {
            early.insert(function.ident.sym.to_string());
        }
    }
    for statement in statements {
        if matches!(statement, ast::Stmt::Decl(ast::Decl::Fn(_))) {
            continue;
        }
        let mut names = ClosureNames::default();
        statement.visit_with(&mut names);
        referenced.extend(names.names);
        if let ast::Stmt::Decl(ast::Decl::Var(var)) = statement {
            for declarator in &var.decls {
                for (name, _) in
                    patterns::names(std::slice::from_ref(&declarator.name)).unwrap_or_default()
                {
                    if referenced.contains(&name) {
                        early.insert(name);
                    }
                }
            }
        }
    }
    early
}

/// Whether any block of `module` declares a binding that a closure may
/// capture before its initialization. Such a program throws `ReferenceError`
/// instances, which are Error-class instances of the Error root.
pub(super) fn module_has_early_captures(module: &ast::Module) -> bool {
    struct Blocks(bool);
    impl Visit for Blocks {
        fn visit_ts_type(&mut self, _: &ast::TsType) {}
        fn visit_block_stmt(&mut self, node: &ast::BlockStmt) {
            if !self.0 && !early_names(&node.stmts).is_empty() {
                self.0 = true;
            }
            node.visit_children_with(self);
        }
        fn visit_function_body(&mut self, node: &ast::FunctionBody) {
            if !self.0 && !early_names(&node.stmts).is_empty() {
                self.0 = true;
            }
            node.visit_children_with(self);
        }
    }
    let mut blocks = Blocks(false);
    module.visit_with(&mut blocks);
    blocks.0
}

/// The declared type of a function initializer that is fixed by its syntax:
/// annotated (or no) parameters, no receiver and type parameters, and an
/// annotated result or a block body without value returns (`void`). Lowering
/// the initializer later must produce exactly this type.
fn function_type(
    params: &[ast::Pat],
    result: Option<&ast::TsTypeAnn>,
    body: Option<&[ast::Stmt]>,
    types: &mut ir::ValueTypes,
) -> Option<Type> {
    let plans = parameters::analyze(params, None, None, types, None).ok()?;
    let result = match (result, body) {
        (Some(ann), _) => annotation(Some(ann), types).ok()??,
        (None, Some(body)) if !contains_value_return(body) => Type::Void,
        _ => return None,
    };
    let params = plans.iter().map(|plan| plan.incoming).collect();
    functions::intern_full(None, params, parameters::shape(&plans), result, types).ok()
}
fn function_value_type(function: &ast::Function, types: &mut ir::ValueTypes) -> Option<Type> {
    if function.is_generator
        || function.type_params.is_some()
        || function.this_param.is_some()
        || function.is_async && function.return_type.is_none()
    {
        return None;
    }
    let params: Vec<_> = function.params.iter().map(|p| p.pat.clone()).collect();
    function_type(
        &params,
        function.return_type.as_deref(),
        function.body.as_ref().map(|body| &body.stmts[..]),
        types,
    )
}
fn initializer_type(init: &ast::Expr, types: &mut ir::ValueTypes) -> Option<Type> {
    match init {
        ast::Expr::Paren(paren) => initializer_type(&paren.expr, types),
        ast::Expr::Fn(function) => function_value_type(&function.function, types),
        ast::Expr::Arrow(arrow)
            if !arrow.is_generator
                && arrow.type_params.is_none()
                && !(arrow.is_async && arrow.return_type.is_none()) =>
        {
            let body = match &*arrow.body {
                ast::ArrowFunctionBody::FunctionBody(block) => Some(&block.stmts[..]),
                ast::ArrowFunctionBody::Expr(_) => None,
            };
            function_type(&arrow.params, arrow.return_type.as_deref(), body, types)
        }
        _ => None,
    }
}

impl Lower<'_> {
    /// The type of an initializer known without lowering it: a timer handle
    /// (`setTimeout(...)`/`setInterval(...)`, so that
    /// `const id = setInterval(() => clearInterval(id), ms)` captures `id`) or
    /// an instance `new C(...)` of a declared class without type arguments.
    fn known_initializer(&self, init: &ast::Expr) -> Option<Type> {
        match stmt::unparen(init) {
            ast::Expr::Call(call) => {
                let ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let ast::Expr::Ident(id) = stmt::unparen(callee) else {
                    return None;
                };
                (matches!(id.sym.as_ref(), "setTimeout" | "setInterval")
                    && self.timer_builtin(id.sym.as_ref()))
                .then(|| self.types.number())
            }
            ast::Expr::New(new) if new.type_args.is_none() => {
                let ast::Expr::Ident(id) = stmt::unparen(&new.callee) else {
                    return None;
                };
                named::class(self.types, id.sym.as_ref()).map(Type::Class)
            }
            _ => None,
        }
    }
    /// A nested function declaration (v37) is a constant binding of its block.
    pub(super) fn predeclare_function(&mut self, function: &ast::FnDecl) -> Result<()> {
        if function.declare {
            return Err(unsupported("ambient nested function declaration"));
        }
        self.declare(function.ident.sym.as_ref(), None, false, false)
            .map_err(|e| self.source.locate(e, function.ident.span))
    }
    /// Reserve a flag for every simple binding of the current block that the
    /// scan finds captured early, and fix the type of such a binding without
    /// an annotation when its function initializer determines it. Returns the
    /// binding slots.
    pub(super) fn reserve_early(&mut self, statements: &[ast::Stmt]) -> Result<Vec<u32>> {
        let names = early_names(statements);
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let mut candidates: Vec<(String, Option<Type>)> = Vec::new();
        for statement in statements {
            match statement {
                ast::Stmt::Decl(ast::Decl::Fn(function))
                    if names.contains(function.ident.sym.as_ref()) =>
                {
                    let ty = function_value_type(&function.function, self.types);
                    candidates.push((function.ident.sym.to_string(), ty));
                }
                ast::Stmt::Decl(ast::Decl::Var(var)) => {
                    for declarator in &var.decls {
                        let ast::Pat::Ident(id) = &declarator.name else {
                            continue;
                        };
                        if !names.contains(id.id.sym.as_ref()) {
                            continue;
                        }
                        let ty = match (&id.type_ann, &declarator.init) {
                            (None, Some(init)) => initializer_type(init, self.types)
                                .or_else(|| parameters::literal(init, self.types))
                                .or_else(|| self.known_initializer(init)),
                            _ => None,
                        };
                        candidates.push((id.id.sym.to_string(), ty));
                    }
                }
                _ => {}
            }
        }
        let mut slots = Vec::new();
        for (name, inferred) in candidates {
            let flag = self.hidden(Type::Boolean);
            let scope = self
                .scopes
                .last_mut()
                .ok_or_else(|| invalid("missing lexical scope"))?;
            let Some(binding) = scope.get_mut(&name) else {
                continue;
            };
            binding.tdz = Some(flag);
            if binding.ty.is_none()
                && let Some(ty) = inferred
            {
                binding.ty = Some(ty);
                self.local_types.insert(binding.slot, ty);
            }
            slots.push(binding.slot);
        }
        Ok(slots)
    }
    /// Statements that hoist the nested function declarations of a block
    /// (v37): each becomes the initialization of its predeclared constant
    /// binding by an equivalent function expression, in source order.
    pub(super) fn hoisted_functions(&mut self, statements: &[ast::Stmt]) -> Result<Vec<S>> {
        let mut output = Vec::new();
        for statement in statements {
            let ast::Stmt::Decl(ast::Decl::Fn(function)) = statement else {
                continue;
            };
            let name = function.ident.sym.as_ref();
            let old = self.binding(name)?;
            // A named expression binds its own name for direct recursion only
            // when its result type is known before its body is lowered.
            let ident = function
                .function
                .return_type
                .is_some()
                .then(|| function.ident.clone());
            let expression = ast::FnExpr {
                ident,
                function: function.function.clone(),
            };
            let hoisting = std::mem::replace(&mut self.core.hoisting, true);
            let value = self.function_expression(&expression, old.ty);
            self.core.hoisting = hoisting;
            let value = value.map_err(|e| self.source.locate(e, function.span()))?;
            if old.ty.is_some_and(|ty| ty != value.ty) {
                return Err(self.source.locate(
                    type_error(format!(
                        "type mismatch for {name}: expected {}, found {}",
                        old.ty.unwrap_or(Type::Void),
                        value.ty
                    )),
                    function.ident.span,
                ));
            }
            self.update(name, value.ty)?;
            output.extend(self.initialize(&old, value));
        }
        Ok(output)
    }
    /// The initialization of `binding` by `value`: a store into the early
    /// cell plus setting its flag when a closure captured it uninitialized.
    pub(super) fn initialize(&mut self, binding: &Binding, value: Expr) -> Vec<S> {
        let Some(flag) = binding
            .tdz
            .filter(|_| self.core.early.contains(&binding.slot))
        else {
            return vec![S::Init(binding.slot, value)];
        };
        let span = value.span;
        vec![
            S::Set(binding.slot, value),
            S::Set(
                flag,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                },
            ),
        ]
    }
    /// The block-entry statements of the early bindings among `slots`.
    pub(super) fn tdz_prologue(&self, bindings: &[Binding], span: swc_common::Span) -> Vec<S> {
        let mut output = Vec::new();
        for binding in bindings {
            let (Some(flag), Some(ty)) = (binding.tdz, binding.ty) else {
                continue;
            };
            if !self.core.early.contains(&binding.slot) {
                continue;
            }
            output.push(S::Init(
                flag,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(0),
                },
            ));
            output.push(S::Init(
                binding.slot,
                Expr {
                    span,
                    ty,
                    kind: E::Zero,
                },
            ));
        }
        output
    }
    /// Capture of an uninitialized binding: legal when the scan reserved a
    /// flag and the binding's type is known. The flag is captured as well,
    /// and the declaring function allocates both cells at block entry.
    pub(super) fn capture_early(&mut self, binding: &Binding) -> Result<()> {
        let Some(flag) = binding.tdz else {
            return Err(unsupported("capture before binding initialization"));
        };
        if binding.ty.is_none() {
            return Err(unsupported(
                "capture before initialization of a binding whose type is inferred from its \
                 initializer; annotate the binding",
            ));
        }
        let flag_binding = Binding {
            identity: (binding.identity.0, flag),
            slot: flag,
            ty: Some(Type::Boolean),
            initialized: true,
            mutable: true,
            readonly: false,
            tdz: None,
            global: None,
        };
        self.capture_binding(&flag_binding)?;
        if binding.identity.0 == self.index {
            self.core.early.insert(binding.identity.1);
        }
        Ok(())
    }
    /// The `ReferenceError` Error-class instance a TDZ access throws, widened
    /// to the exception channel.
    pub(super) fn reference_error(&mut self, name: &str, span: swc_common::Span) -> Result<Expr> {
        let root = self
            .types
            .classes
            .error_root(varta_common::value::ErrorClass::Error)
            .ok_or_else(|| CompilerError::Internal("missing Error root for TDZ".into()))?;
        let shape = self.types.classes.shape(root);
        let text = |this: &Self, text: &str| Expr {
            span,
            ty: Type::String,
            kind: E::String(this.types.profile.string_bytes(text)),
        };
        let mut fields = Vec::new();
        for field in self.types.classes.get(root).fields.clone() {
            let (slot, _) = self
                .types
                .classes
                .field(root, &field.name)
                .ok_or_else(|| CompilerError::Internal("missing Error root field".into()))?;
            let value = match field.name.as_str() {
                classes::MESSAGE => text(self, &message(name)),
                classes::NAME => text(self, REFERENCE_ERROR),
                _ => unions::widen(
                    Expr {
                        span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                    Some(field.ty),
                    self.types,
                )?,
            };
            fields.push((slot, value));
        }
        let instance = self.hidden(Type::Class(root));
        let marker = self.hidden(Type::Number);
        let value = Expr {
            span,
            ty: Type::Class(root),
            kind: E::ValueBlock {
                output: instance,
                marker,
                roots: vec![instance],
                body: vec![S::Init(
                    instance,
                    Expr {
                        span,
                        ty: Type::Record(shape),
                        kind: E::Record(fields),
                    },
                )],
            },
        };
        let thrown = self.exception_type()?;
        unions::widen(value, Some(thrown), self.types)
    }
    /// `if (!flag) throw ReferenceError` before an access through a closure.
    pub(super) fn tdz_check(&mut self, name: &str, flag: u32, span: swc_common::Span) -> Result<S> {
        let error = self.reference_error(name, span)?;
        Ok(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Not(Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Local(flag),
                })),
            },
            vec![S::Throw(error)],
            vec![],
        ))
    }
    /// The local slot of the flag of an early `binding` in this function.
    pub(super) fn tdz_flag(&self, binding: &Binding) -> Option<u32> {
        let flag = binding.tdz?;
        if binding.identity.0 == self.index {
            Some(flag)
        } else {
            self.captures
                .get(&(binding.identity.0, flag))
                .map(|captured| captured.slot)
        }
    }
    /// `name = value` through a closure that captured the binding
    /// uninitialized: the right-hand side runs first, then the flag test
    /// (PutValue throws on an uninitialized binding), then the store.
    pub(super) fn tdz_write(&mut self, name: &str, binding: &Binding, value: Expr) -> Result<S> {
        let Some(flag) = self.tdz_flag(binding) else {
            return Err(invalid("assignment to constant or uninitialized binding"));
        };
        let span = value.span;
        let ty = value.ty;
        let saved = self.hidden(ty);
        let check = self.tdz_check(name, flag, span)?;
        Ok(S::Scope(
            vec![saved],
            vec![
                S::Init(saved, value),
                check,
                S::Set(
                    binding.slot,
                    Expr {
                        span,
                        ty,
                        kind: E::Local(saved),
                    },
                ),
            ],
        ))
    }
    /// A read of the identifier `name` bound to `binding` that this closure
    /// captured before its initialization: the flag is tested first.
    pub(super) fn tdz_ident(
        &mut self,
        name: &str,
        binding: &Binding,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
        let value = Expr {
            span,
            ty,
            kind: E::Local(binding.slot),
        };
        self.tdz_read(name, binding, value)
    }
    /// A read of `binding` through a closure that captured it uninitialized.
    pub(super) fn tdz_read(&mut self, name: &str, binding: &Binding, value: Expr) -> Result<Expr> {
        let Some(flag) = self.tdz_flag(binding) else {
            return Err(invalid(format!(
                "binding used before initialization: {name}"
            )));
        };
        let span = value.span;
        let ty = value.ty;
        let check = self.tdz_check(name, flag, span)?;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![check, S::Init(output, value)],
            },
        })
    }
}
