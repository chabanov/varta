//! Constructor initializers. Instances are allocated with placeholder slots, so
//! no field may be observed before it is definitely assigned. A syntactic
//! discipline guarantees this without flow analysis: in a constructor (and in
//! field initializers) `this` may only read already-assigned fields or write
//! fields; it never escapes, is never captured and never receives a method
//! call, because a subclass override could observe its unassigned fields.
use super::*;
use swc_ecma_visit::{Visit, VisitWith};

/// The first discipline violation with its source position.
type Violation = Option<(String, Span)>;

struct ThisUse<'a> {
    fields: &'a BTreeSet<String>,
    assigned: &'a BTreeSet<String>,
    /// False before `super(...)` and inside field initializers.
    allow_this: bool,
    /// Inside an arrow body: `this` would be captured; returns are local.
    closure: bool,
    /// V37 (`language_core`): a closure created after `super(...)` may read
    /// the fields assigned where it is created and write any field through
    /// `this.field`, but never let `this` escape or call a method on it.
    capture: bool,
    /// Whether closures created at this statement may capture `this.field`.
    complete: bool,
    /// v32 `error_subclasses`: `new.target.name` is admitted in constructor
    /// parameters and bodies (not in field initializers or closures).
    new_target: bool,
    violation: Violation,
}
impl ThisUse<'_> {
    fn fail(&mut self, message: impl Into<String>, span: Span) {
        if self.violation.is_none() {
            self.violation = Some((message.into(), span));
        }
    }
    fn this_member(&mut self, member: &ast::MemberExpr, write: bool) {
        if !self.allow_this || self.closure && !self.capture {
            return self.fail(self.this_message(), member.span);
        }
        let ast::MemberProp::Ident(name) = &member.prop else {
            return self.fail(
                "constructor this may only access declared fields",
                member.span,
            );
        };
        let name = name.sym.to_string();
        if !self.fields.contains(&name) {
            return self.fail(
                "constructor this may only access declared fields",
                member.span,
            );
        }
        if !write && !self.assigned.contains(&name) {
            self.fail(
                format!("class field {name} is read before it is definitely assigned"),
                member.span,
            );
        }
    }
    fn this_message(&self) -> &'static str {
        if self.closure {
            "constructor closures cannot capture this"
        } else {
            "this is unavailable before super(), in field initializers and parameter defaults"
        }
    }
}
fn this_target(target: &ast::AssignTarget) -> Option<&ast::MemberExpr> {
    match target {
        ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member))
            if matches!(&*member.obj, ast::Expr::This(_)) =>
        {
            Some(member)
        }
        _ => None,
    }
}
impl Visit for ThisUse<'_> {
    fn visit_ts_type(&mut self, _: &ast::TsType) {}
    // Ordinary functions own their receiver; nested classes are rejected later.
    fn visit_function(&mut self, _: &ast::Function) {}
    fn visit_class(&mut self, _: &ast::Class) {}
    fn visit_this_expr(&mut self, node: &ast::ThisExpr) {
        let message = if self.allow_this && !self.closure {
            "constructor this may only access declared fields"
        } else {
            self.this_message()
        };
        self.fail(message, node.span);
    }
    fn visit_arrow_expr(&mut self, node: &ast::ArrowExpr) {
        let closure = std::mem::replace(&mut self.closure, true);
        let capture = self.capture;
        self.capture |= !closure && self.complete;
        node.visit_children_with(self);
        self.closure = closure;
        self.capture = capture;
    }
    fn visit_member_expr(&mut self, node: &ast::MemberExpr) {
        if self.new_target && error_classes::new_target_name(node) {
            if self.closure {
                self.fail("constructor closures cannot capture new.target", node.span);
            }
            return;
        }
        if matches!(&*node.obj, ast::Expr::This(_)) {
            self.this_member(node, false);
            if let ast::MemberProp::Computed(key) = &node.prop {
                key.visit_with(self);
            }
            return;
        }
        node.visit_children_with(self);
    }
    fn visit_assign_expr(&mut self, node: &ast::AssignExpr) {
        if node.op == ast::AssignOp::Assign
            && let Some(member) = this_target(&node.left)
        {
            self.this_member(member, true);
            return node.right.visit_with(self);
        }
        node.visit_children_with(self);
    }
    fn visit_super_prop_expr(&mut self, node: &ast::SuperPropExpr) {
        self.fail("super property access in a constructor", node.span);
    }
    fn visit_call_expr(&mut self, node: &ast::CallExpr) {
        if matches!(node.callee, ast::Callee::Super(_)) {
            self.fail(
                "super(...) must be a top-level statement of a derived constructor",
                node.span,
            );
        }
        node.visit_children_with(self);
    }
    fn visit_meta_prop_expr(&mut self, node: &ast::MetaPropExpr) {
        self.fail("new.target", node.span);
    }
    fn visit_return_stmt(&mut self, node: &ast::ReturnStmt) {
        if !self.closure {
            self.fail("return in a class constructor", node.span);
        }
        node.visit_children_with(self);
    }
}

#[derive(Default)]
struct Identifiers(BTreeSet<String>);
impl Visit for Identifiers {
    fn visit_ts_type(&mut self, _: &ast::TsType) {}
    fn visit_ident(&mut self, node: &ast::Ident) {
        self.0.insert(node.sym.to_string());
    }
}

fn super_statement(statement: &ast::Stmt) -> Option<&ast::CallExpr> {
    let ast::Stmt::Expr(expression) = statement else {
        return None;
    };
    match stmt::unparen(&expression.expr) {
        ast::Expr::Call(call) if matches!(call.callee, ast::Callee::Super(_)) => Some(call),
        _ => None,
    }
}
fn this_assignment(statement: &ast::Stmt) -> Option<&ast::MemberExpr> {
    let ast::Stmt::Expr(expression) = statement else {
        return None;
    };
    let ast::Expr::Assign(assign) = stmt::unparen(&expression.expr) else {
        return None;
    };
    (assign.op == ast::AssignOp::Assign)
        .then(|| this_target(&assign.left))
        .flatten()
}

struct Discipline<'a> {
    fields: BTreeSet<String>,
    assigned: BTreeSet<String>,
    source: &'a Source,
    new_target: bool,
    /// V37 (`language_core`): constructor closures may capture `this.field`.
    closures: bool,
}
impl Discipline<'_> {
    fn check(&self, node: &dyn Checked, allow_this: bool) -> Result<()> {
        self.visit(node, allow_this, self.new_target)
    }
    /// Field initializers run with an undefined `new.target`.
    fn check_initializer(&self, node: &dyn Checked) -> Result<()> {
        self.visit(node, false, false)
    }
    fn visit(&self, node: &dyn Checked, allow_this: bool, new_target: bool) -> Result<()> {
        let mut visitor = ThisUse {
            fields: &self.fields,
            assigned: &self.assigned,
            allow_this,
            closure: false,
            capture: false,
            complete: self.closures && allow_this,
            new_target,
            violation: None,
        };
        node.accept(&mut visitor);
        match visitor.violation {
            Some((message, span)) => Err(self.source.locate(unsupported(&message), span)),
            None => Ok(()),
        }
    }
}
/// Object-safe visiting of the statement/expression nodes checked above.
trait Checked {
    fn accept(&self, visitor: &mut ThisUse<'_>);
}
impl Checked for ast::Stmt {
    fn accept(&self, visitor: &mut ThisUse<'_>) {
        self.visit_with(visitor);
    }
}
impl Checked for ast::Expr {
    fn accept(&self, visitor: &mut ThisUse<'_>) {
        self.visit_with(visitor);
    }
}
impl Checked for ast::Param {
    fn accept(&self, visitor: &mut ThisUse<'_>) {
        self.visit_with(visitor);
    }
}
impl Checked for Vec<ast::ExprOrSpread> {
    fn accept(&self, visitor: &mut ThisUse<'_>) {
        for argument in self {
            argument.visit_with(visitor);
        }
    }
}

fn this_assign(name: &str, value: Box<ast::Expr>) -> ast::Stmt {
    let span = value.span();
    ast::Stmt::Expr(ast::ExprStmt {
        span,
        expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
            span,
            op: ast::AssignOp::Assign,
            left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(ast::MemberExpr {
                span,
                obj: Box::new(ast::Expr::This(ast::ThisExpr { span })),
                prop: ast::MemberProp::Ident(ast::IdentName::new(name.into(), span)),
            })),
            right: value,
        })),
    })
}

/// Validate the constructor of `class` and build its initializer declaration:
/// own field initializers run first in a base class and right after the
/// `super(...)` statement in a derived class.
pub(super) fn declaration(id: u32, types: &ir::ValueTypes) -> Result<ast::FnDecl> {
    let class = types.classes.get(id);
    let source = &class.source;
    let constructor = class
        .declaration
        .body
        .iter()
        .find_map(|member| match member {
            ast::ClassMember::Constructor(ctor) => Some(ctor),
            _ => None,
        });
    let derived = class.parent.is_some();
    let fields: BTreeSet<String> = class.fields.iter().map(|f| f.name.clone()).collect();
    let inherited: BTreeSet<String> = class
        .fields
        .iter()
        .filter(|f| f.owner != id)
        .map(|f| f.name.clone())
        .collect();
    let initialized: BTreeSet<String> = class.initializers.iter().map(|(n, _)| n.clone()).collect();
    let mut discipline = Discipline {
        fields,
        assigned: BTreeSet::new(),
        source,
        new_target: types.profile.error_subclasses(),
        closures: types.profile.language_core(),
    };
    // Field initializers never see `this` or constructor-scoped bindings.
    let mut scoped = BTreeSet::new();
    let (function, statements) = match constructor {
        Some(ctor) => {
            let function = constructor_function(ctor, source)?;
            // Parameter defaults run before any field is assigned.
            for param in &function.params {
                discipline.check(param, false)?;
            }
            let patterns: Vec<_> = function.params.iter().map(|p| p.pat.clone()).collect();
            let names = patterns::names(&patterns).map_err(|(e, span)| source.locate(e, span))?;
            scoped.extend(names.into_iter().map(|(name, _)| name));
            let statements = function
                .body
                .as_ref()
                .map(|body| body.stmts.clone())
                .unwrap_or_default();
            for statement in &statements {
                if let ast::Stmt::Decl(ast::Decl::Var(var)) = statement {
                    for declarator in &var.decls {
                        let names = patterns::names(std::slice::from_ref(&declarator.name))
                            .map_err(|(e, span)| source.locate(e, span))?;
                        scoped.extend(names.into_iter().map(|(name, _)| name));
                    }
                }
            }
            (function, statements)
        }
        None => implicit(id, types)?,
    };
    for (_, value) in &class.initializers {
        discipline.check_initializer(&**value)?;
        let mut identifiers = Identifiers::default();
        value.visit_with(&mut identifiers);
        if let Some(name) = identifiers.0.intersection(&scoped).next() {
            return Err(source.locate(
                unsupported(&format!(
                    "field initializer referencing constructor binding {name}"
                )),
                value.span(),
            ));
        }
    }
    if !derived {
        discipline.assigned.extend(initialized.iter().cloned());
    }
    let mut body = Vec::new();
    let mut called_super = false;
    let injected: Vec<_> = class
        .initializers
        .iter()
        .map(|(name, value)| this_assign(name, value.clone()))
        .collect();
    if !derived {
        body.extend(injected.iter().cloned());
    }
    for statement in statements {
        if derived && !called_super {
            if let Some(call) = super_statement(&statement) {
                discipline.check(&call.args, false)?;
                called_super = true;
                discipline.assigned.extend(inherited.iter().cloned());
                discipline.assigned.extend(initialized.iter().cloned());
                body.push(statement);
                body.extend(injected.iter().cloned());
                continue;
            }
            discipline.check(&statement, false)?;
        } else if let Some(member) = this_assignment(&statement) {
            // The right-hand side sees only fields assigned before it.
            discipline.check(&statement, true)?;
            if let ast::MemberProp::Ident(name) = &member.prop {
                discipline.assigned.insert(name.sym.to_string());
            }
        } else {
            discipline.check(&statement, true)?;
        }
        body.push(statement);
    }
    if derived && !called_super {
        return Err(source.locate(
            unsupported("derived class constructor without a top-level super(...) statement"),
            class.span,
        ));
    }
    if let Some(field) = class
        .fields
        .iter()
        .find(|f| f.owner == id && !discipline.assigned.contains(&f.name))
    {
        return Err(source.locate(
            invalid(format!(
                "class field {} is not definitely assigned in the constructor",
                field.name
            )),
            class.span,
        ));
    }
    let mut function = function;
    function.body = Some(ast::FunctionBody {
        span: function.span,
        stmts: body,
    });
    Ok(function_declaration(CONSTRUCTOR_NAME, class.span, function))
}

/// `constructor() {}` for a base class; `constructor(...p) { super(...p); }`
/// with the base initializer's fixed/optional parameters for a derived class.
fn implicit(id: u32, types: &ir::ValueTypes) -> Result<(Box<ast::Function>, Vec<ast::Stmt>)> {
    let class = types.classes.get(id);
    let span = class.span;
    let count = class.init.as_ref().map_or(0, |init| init.params.len());
    let names: Vec<String> = (0..count).map(implicit_parameter).collect();
    let ident = |name: &str| ast::Ident::new(name.into(), span, Default::default());
    let params = names
        .iter()
        .map(|name| ast::Param {
            span,
            decorators: Vec::new(),
            pat: ast::Pat::Ident(ast::BindingIdent {
                id: ident(name),
                type_ann: None,
            }),
        })
        .collect();
    let mut statements = Vec::new();
    if class.parent.is_some() {
        statements.push(ast::Stmt::Expr(ast::ExprStmt {
            span,
            expr: Box::new(ast::Expr::Call(ast::CallExpr {
                span,
                ctxt: Default::default(),
                callee: ast::Callee::Super(ast::Super { span }),
                args: names
                    .iter()
                    .map(|name| ast::ExprOrSpread {
                        spread: None,
                        expr: Box::new(ast::Expr::Ident(ident(name))),
                    })
                    .collect(),
                type_args: None,
            })),
        }));
    }
    let function = Box::new(ast::Function {
        params,
        decorators: Vec::new(),
        span,
        ctxt: Default::default(),
        body: None,
        is_generator: false,
        is_async: false,
        type_params: None,
        return_type: None,
        this_param: None,
    });
    Ok((function, statements))
}
