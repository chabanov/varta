//! Scope-aware rewriting of enum and namespace references.
//!
//! Frames map the names visible at module level and in each enclosing
//! namespace declaration to their symbols, innermost last; a name bound by
//! a local scope (a parameter, a local declaration) shadows every frame.
//! Rewritten forms:
//!
//! - `E.X`, `E["X"]` (value) become the member's literal; `E[k]` becomes
//!   the name of the member whose numeric value is the constant `k`, or a
//!   call of the reverse-mapping function;
//! - `NS.a.b` and an unqualified member name inside its namespace become the
//!   member's generated name, in expressions, shorthand properties, type
//!   references, `typeof` queries and heritage clauses;
//! - `E.X` as a type becomes the member's literal type.
use super::scope::{self, Names};
use super::statics::Access;
use super::{Symbol, Value, enums, type_error, unsupported};
use crate::CompilerError;
use crate::source::Source;
use std::collections::{BTreeMap, BTreeSet};
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{VisitMut, VisitMutWith};

pub(super) type Frame = BTreeMap<String, Symbol>;

/// Module-level class hierarchy, for `protected` static field checks.
pub(super) struct Context<'a> {
    pub parents: &'a BTreeMap<String, String>,
}
const NO_PARENTS: &BTreeMap<String, String> = &BTreeMap::new();
const EMPTY: Context<'static> = Context {
    parents: NO_PARENTS,
};

/// Rewrite one module item.
pub(super) fn item(
    item: ast::ModuleItem,
    source: &Source,
    frames: &[&Frame],
) -> Result<ast::ModuleItem, CompilerError> {
    item_with(item, source, frames, &EMPTY, None)
}

/// Rewrite one module item with the class hierarchy; `class` is the class
/// whose static initializer the item is.
pub(super) fn item_with(
    mut item: ast::ModuleItem,
    source: &Source,
    frames: &[&Frame],
    context: &Context<'_>,
    class: Option<&str>,
) -> Result<ast::ModuleItem, CompilerError> {
    let mut rewriter = Rewriter::new(source, frames);
    rewriter.parents = context.parents;
    rewriter.classes.extend(class.map(str::to_owned));
    item.visit_mut_with(&mut rewriter);
    rewriter.finish().map(|_| item)
}

/// Rewrite one hoisted declaration.
pub(super) fn declaration(
    mut decl: ast::Decl,
    source: &Source,
    frames: &[&Frame],
) -> Result<ast::Decl, CompilerError> {
    let mut rewriter = Rewriter::new(source, frames);
    decl.visit_mut_with(&mut rewriter);
    rewriter.finish().map(|_| decl)
}

/// Rewrite one statement.
pub(super) fn statement(
    statement: &mut ast::Stmt,
    source: &Source,
    frames: &[&Frame],
) -> Result<(), CompilerError> {
    let mut rewriter = Rewriter::new(source, frames);
    statement.visit_mut_with(&mut rewriter);
    rewriter.finish().map(|_| ())
}

/// Rewrite a whole module; return the generated names it now references.
pub(super) fn module(
    module: &mut ast::Module,
    source: &Source,
    frame: &Frame,
) -> Result<BTreeSet<String>, CompilerError> {
    let frames = [frame];
    let mut rewriter = Rewriter::new(source, &frames);
    rewriter.imported = true;
    module.visit_mut_with(&mut rewriter);
    rewriter.finish()
}

struct Rewriter<'a> {
    source: &'a Source,
    frames: &'a [&'a Frame],
    scopes: Vec<Names>,
    used: BTreeSet<String>,
    error: Option<CompilerError>,
    /// The module-level classes whose bodies enclose the current node.
    classes: Vec<String>,
    parents: &'a BTreeMap<String, String>,
    /// Rewriting another module's declarations: their static fields are
    /// written through their setters.
    imported: bool,
}

/// What a member path denotes.
enum Resolved {
    Symbol(Symbol),
    /// A member value (an enum member) already lowered to its literal.
    Done,
    /// Not a path of a symbol.
    Other,
}

impl<'a> Rewriter<'a> {
    fn new(source: &'a Source, frames: &'a [&'a Frame]) -> Self {
        Self {
            source,
            frames,
            scopes: Vec::new(),
            used: BTreeSet::new(),
            error: None,
            classes: Vec::new(),
            parents: NO_PARENTS,
            imported: false,
        }
    }
    /// The imported static field `member` resolves to, with its setter.
    fn imported_field(&mut self, member: &ast::MemberExpr) -> Option<(Symbol, String)> {
        if !self.imported {
            return None;
        }
        let mut expr = ast::Expr::Member(member.clone());
        let Resolved::Symbol(symbol) = self.path(&mut expr) else {
            return None;
        };
        let setter = symbol.field.as_ref()?.setter.clone()?;
        Some((symbol, setter))
    }
    /// A write of an imported static field through its setter:
    /// `C.x = v` is `set(v)`, `C.x op= v` is `set(x op v)`, `++C.x` is
    /// `set(x + 1)` and `C.x++` is `set(x + 1) - 1`.
    fn imported_write(&mut self, expr: &mut ast::Expr) -> bool {
        let span = expr.span();
        let (member, value) = match expr {
            ast::Expr::Assign(assign) => {
                let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) =
                    &assign.left
                else {
                    return false;
                };
                let op = match assign.op {
                    ast::AssignOp::Assign => None,
                    ast::AssignOp::AddAssign => Some(ast::BinaryOp::Add),
                    ast::AssignOp::SubAssign => Some(ast::BinaryOp::Sub),
                    ast::AssignOp::MulAssign => Some(ast::BinaryOp::Mul),
                    ast::AssignOp::DivAssign => Some(ast::BinaryOp::Div),
                    ast::AssignOp::ModAssign => Some(ast::BinaryOp::Mod),
                    ast::AssignOp::ExpAssign => Some(ast::BinaryOp::Exp),
                    ast::AssignOp::LShiftAssign => Some(ast::BinaryOp::LShift),
                    ast::AssignOp::RShiftAssign => Some(ast::BinaryOp::RShift),
                    ast::AssignOp::ZeroFillRShiftAssign => Some(ast::BinaryOp::ZeroFillRShift),
                    ast::AssignOp::BitOrAssign => Some(ast::BinaryOp::BitOr),
                    ast::AssignOp::BitXorAssign => Some(ast::BinaryOp::BitXor),
                    ast::AssignOp::BitAndAssign => Some(ast::BinaryOp::BitAnd),
                    _ => return false,
                };
                (member.clone(), Some((op, assign.right.clone(), None)))
            }
            ast::Expr::Update(update) => {
                let ast::Expr::Member(member) = &*update.arg else {
                    return false;
                };
                let op = match update.op {
                    ast::UpdateOp::PlusPlus => ast::BinaryOp::Add,
                    ast::UpdateOp::MinusMinus => ast::BinaryOp::Sub,
                };
                let one = Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
                    span,
                    value: 1.0,
                    raw: None,
                })));
                let undo = (!update.prefix).then_some(match op {
                    ast::BinaryOp::Add => ast::BinaryOp::Sub,
                    _ => ast::BinaryOp::Add,
                });
                (member.clone(), Some((Some(op), one, undo)))
            }
            _ => return false,
        };
        let Some((symbol, setter)) = self.imported_field(&member) else {
            return false;
        };
        let Some((op, mut right, undo)) = value else {
            return false;
        };
        self.check_field(&symbol, true, span);
        right.visit_mut_with(self);
        let current = ast::Expr::Ident(self.renamed(&symbol, span));
        let value = match op {
            Some(op) => Box::new(ast::Expr::Bin(ast::BinExpr {
                span,
                op,
                left: Box::new(current),
                right,
            })),
            None => right,
        };
        let callee = ast::Expr::Ident(self.renamed(&Symbol::plain(setter), span));
        let call = ast::Expr::Call(ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(callee)),
            args: vec![ast::ExprOrSpread {
                spread: None,
                expr: value,
            }],
            type_args: None,
        });
        *expr = match undo {
            Some(op) => ast::Expr::Bin(ast::BinExpr {
                span,
                op,
                left: Box::new(call),
                right: Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
                    span,
                    value: 1.0,
                    raw: None,
                }))),
            }),
            None => call,
        };
        true
    }
    /// TypeScript's accessibility check of a static field reference.
    fn check_field(&mut self, symbol: &Symbol, write: bool, span: Span) {
        let Some(field) = &symbol.field else {
            return;
        };
        let name = super::display_name(&symbol.name);
        let inside = |rewriter: &Self, class: &str| rewriter.classes.iter().any(|c| c == class);
        let allowed = match field.access {
            Access::Public => true,
            Access::Private => inside(self, &field.owner),
            Access::Protected => self.classes.iter().any(|class| {
                let mut current = Some(class.clone());
                let mut depth = 0;
                while let Some(c) = current {
                    if c == field.owner {
                        return true;
                    }
                    depth += 1;
                    if depth > self.parents.len() {
                        break;
                    }
                    current = self.parents.get(&c).cloned();
                }
                // A subclass of a class from another module.
                self.parents.is_empty()
            }),
        };
        if !allowed {
            self.fail(
                type_error(format!(
                    "static field {name} is {} and only accessible within class {} (TS2341)",
                    if field.access == Access::Private {
                        "private"
                    } else {
                        "protected"
                    },
                    field.owner
                )),
                span,
            );
            return;
        }
        if write && field.readonly {
            self.fail(
                type_error(format!(
                    "cannot assign to {name} because it is a read-only static field (TS2540)"
                )),
                span,
            );
        } else if write && field.inherited {
            self.fail(
                unsupported(&format!(
                    "assignment to the inherited static field {name} through a subclass (it would create an own property of the subclass)"
                )),
                span,
            );
        }
    }
    fn finish(self) -> Result<BTreeSet<String>, CompilerError> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.used),
        }
    }
    fn fail(&mut self, error: CompilerError, span: Span) {
        if self.error.is_none() {
            self.error = Some(self.source.locate(error, span));
        }
    }
    fn lookup(&self, name: &str) -> Option<Symbol> {
        if self.scopes.iter().any(|scope| scope.contains(name)) {
            return None;
        }
        self.frames
            .iter()
            .rev()
            .find_map(|frame| frame.get(name))
            .cloned()
    }
    fn renamed(&mut self, symbol: &Symbol, span: Span) -> ast::Ident {
        if symbol.name.starts_with(super::PREFIX) {
            self.used.insert(symbol.name.clone());
        }
        ast::Ident::new_no_ctxt(symbol.name.as_str().into(), span)
    }
    /// Replace a whole symbol expression by its value binding.
    fn value(&mut self, expr: &mut ast::Expr, symbol: &Symbol, original: &str) {
        let span = expr.span();
        self.check_field(symbol, false, span);
        if !symbol.value {
            // A non-const enum used as a value is its enum object.
            if let Some(object) = symbol.enumeration.as_ref().and_then(|e| e.object.clone()) {
                *expr = ast::Expr::Ident(self.renamed(&Symbol::plain(object), span));
                return;
            }
            if let Some(object) = symbol.namespace.as_ref().and_then(|n| n.object.clone()) {
                *expr = ast::Expr::Ident(self.renamed(&Symbol::plain(object), span));
                return;
            }
            let kind = if symbol.enumeration.is_some() {
                "const enum object (TS2475)"
            } else {
                "namespace object (a class or let member has no value in it)"
            };
            self.fail(
                unsupported(&format!("{kind} {original} used as a value")),
                span,
            );
            return;
        }
        if symbol.name != original {
            *expr = ast::Expr::Ident(self.renamed(symbol, span));
        }
    }
    /// Resolve `expr` as a path of symbols, lowering enum members in place.
    fn path(&mut self, expr: &mut ast::Expr) -> Resolved {
        match expr {
            ast::Expr::Ident(id) => match self.lookup(id.sym.as_ref()) {
                Some(symbol) if symbol.structured() || symbol.name != id.sym.as_ref() => {
                    Resolved::Symbol(symbol)
                }
                _ => Resolved::Other,
            },
            ast::Expr::Member(member) => {
                let base = match self.path(&mut member.obj) {
                    Resolved::Symbol(symbol) => symbol,
                    _ => return Resolved::Other,
                };
                let original = base_name(&member.obj);
                let key = static_key(&member.prop);
                if let (Some(key), Some(namespace)) = (&key, &base.namespace)
                    && let Some(found) = namespace.members.get(key)
                {
                    return Resolved::Symbol(found.symbol.clone());
                }
                if let Some(enumeration) = base.enumeration.clone() {
                    let span = member.span;
                    match &key {
                        Some(key) => match enumeration.members.iter().find(|(n, _)| n == key) {
                            Some((_, value)) => *expr = enums::literal(value, span),
                            None => self.fail(
                                type_error(format!(
                                    "property {key} does not exist on enum {}",
                                    enumeration.name_for_display()
                                )),
                                member.prop.span(),
                            ),
                        },
                        None => self.reverse(expr, &enumeration),
                    }
                    return Resolved::Done;
                }
                if base.namespace.is_some() && !base.value {
                    self.fail(
                        type_error(format!(
                            "namespace {original} has no exported member {}",
                            key.unwrap_or_default()
                        )),
                        member.prop.span(),
                    );
                    return Resolved::Done;
                }
                // A member of a function or class merged with a namespace.
                self.value(&mut member.obj, &base, &original);
                Resolved::Done
            }
            _ => Resolved::Other,
        }
    }
    /// `E[k]` with a key that is not a string literal: the reverse mapping.
    fn reverse(&mut self, expr: &mut ast::Expr, enumeration: &super::Enum) {
        let ast::Expr::Member(member) = expr else {
            return;
        };
        let span = member.span;
        let ast::MemberProp::Computed(computed) = &mut member.prop else {
            return;
        };
        computed.expr.visit_mut_with(self);
        // A string-valued enum's dynamic key reads its enum object
        // (`E[k]` with `k: keyof typeof E`).
        if enumeration.reverse.is_none()
            && let Some(object) = enumeration.object.clone()
        {
            *member.obj = ast::Expr::Ident(self.renamed(&Symbol::plain(object), member.span));
            return;
        }
        let Some(reverse) = enumeration.reverse.clone() else {
            let message = if enumeration.is_const {
                "a const enum member can only be accessed using a string literal (TS2476)"
            } else {
                "a string enum has no reverse mapping"
            };
            self.fail(type_error(message), computed.span);
            return;
        };
        // A constant key names its member directly.
        if let Some(number) = number(&computed.expr) {
            let name = enumeration
                .members
                .iter()
                .rev()
                .find(|(_, v)| *v == Value::Number(number))
                .map(|(n, _)| n.clone());
            if let Some(name) = name {
                *expr = ast::Expr::Lit(ast::Lit::Str(enums::string(&name, span)));
                return;
            }
        }
        let key = std::mem::replace(
            &mut *computed.expr,
            ast::Expr::Invalid(ast::Invalid { span }),
        );
        let callee = ast::Expr::Ident(self.renamed(&Symbol::plain(reverse), span));
        *expr = ast::Expr::Call(ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(callee)),
            args: vec![ast::ExprOrSpread {
                spread: None,
                expr: Box::new(key),
            }],
            type_args: None,
        });
    }
    /// Resolve a type entity name.
    fn type_path(&mut self, name: &ast::TsEntityName) -> Option<TypeResolved> {
        match name {
            ast::TsEntityName::Ident(id) => {
                let symbol = self.lookup(id.sym.as_ref())?;
                Some(TypeResolved::Symbol(symbol, id.sym.to_string()))
            }
            ast::TsEntityName::TsQualifiedName(qualified) => {
                let TypeResolved::Symbol(base, original) = self.type_path(&qualified.left)? else {
                    return None;
                };
                let key = qualified.right.sym.to_string();
                if let Some(namespace) = &base.namespace
                    && let Some(found) = namespace.members.get(&key)
                {
                    return Some(TypeResolved::Symbol(found.symbol.clone(), key));
                }
                if let Some(enumeration) = &base.enumeration {
                    return match enumeration.members.iter().find(|(n, _)| *n == key) {
                        Some((_, value)) => Some(TypeResolved::Literal(value.clone())),
                        None => {
                            self.fail(
                                type_error(format!(
                                    "property {key} does not exist on enum {original}"
                                )),
                                qualified.right.span,
                            );
                            None
                        }
                    };
                }
                self.fail(
                    type_error(format!("namespace {original} has no exported member {key}")),
                    qualified.right.span,
                );
                None
            }
        }
    }
    fn push_scope(&mut self, names: Names) {
        self.scopes.push(names);
    }
}

enum TypeResolved {
    Symbol(Symbol, String),
    Literal(Value),
}

impl super::Enum {
    fn name_for_display(&self) -> &str {
        super::display_name(&self.name)
    }
}

fn base_name(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Ident(id) => id.sym.to_string(),
        ast::Expr::Member(member) => match &member.prop {
            ast::MemberProp::Ident(id) => id.sym.to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// The key of `.x` or `["x"]`.
fn static_key(prop: &ast::MemberProp) -> Option<String> {
    match prop {
        ast::MemberProp::Ident(id) => Some(id.sym.to_string()),
        ast::MemberProp::Computed(computed) => match &*computed.expr {
            ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
            _ => None,
        },
        ast::MemberProp::PrivateName(_) => None,
    }
}

/// The value of a numeric literal key, possibly negated.
fn number(expr: &ast::Expr) -> Option<f64> {
    match expr {
        ast::Expr::Lit(ast::Lit::Num(n)) => Some(n.value),
        ast::Expr::Paren(paren) => number(&paren.expr),
        ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => {
            number(&unary.arg).map(|n| -n)
        }
        _ => None,
    }
}

impl Rewriter<'_> {
    /// The enum type an unannotated mutable binding initialized with an enum
    /// member widens to (TypeScript widens `let x = E.A` to `E`, not to the
    /// member's literal type nor to `number`).
    fn widened_enum(&mut self, init: &ast::Expr) -> Option<Box<ast::TsTypeAnn>> {
        let ast::Expr::Member(member) = init else {
            return None;
        };
        let key = static_key(&member.prop)?;
        let mut base = (*member.obj).clone();
        let errors = self.error.is_some();
        let Resolved::Symbol(symbol) = self.path(&mut base) else {
            return None;
        };
        if self.error.is_some() && !errors {
            return None;
        }
        let enumeration = symbol.enumeration.as_ref()?;
        enumeration.members.iter().find(|(name, _)| *name == key)?;
        let span = member.span;
        let name = self.renamed(&symbol, span);
        Some(Box::new(ast::TsTypeAnn {
            span,
            type_ann: Box::new(ast::TsType::TsTypeRef(ast::TsTypeRef {
                span,
                type_name: ast::TsEntityName::Ident(name),
                type_params: None,
            })),
        }))
    }
    /// An unannotated parameter `p = E.A` has the enum type.
    fn widen_default(&mut self, pat: &mut ast::Pat) {
        if let ast::Pat::Assign(pattern) = pat
            && let ast::Pat::Ident(binding) = &mut *pattern.left
            && binding.type_ann.is_none()
        {
            binding.type_ann = self.widened_enum(&pattern.right);
        }
    }
}

impl VisitMut for Rewriter<'_> {
    fn visit_mut_var_decl(&mut self, var: &mut ast::VarDecl) {
        if var.kind != ast::VarDeclKind::Const {
            for declarator in &mut var.decls {
                if let (ast::Pat::Ident(binding), Some(init)) =
                    (&mut declarator.name, declarator.init.as_deref())
                    && binding.type_ann.is_none()
                {
                    binding.type_ann = self.widened_enum(init);
                }
            }
        }
        var.visit_mut_children_with(self);
    }
    fn visit_mut_class_prop(&mut self, property: &mut ast::ClassProp) {
        if property.type_ann.is_none()
            && !property.readonly
            && let Some(value) = property.value.as_deref()
        {
            property.type_ann = self.widened_enum(value);
        }
        property.visit_mut_children_with(self);
    }
    fn visit_mut_param(&mut self, param: &mut ast::Param) {
        self.widen_default(&mut param.pat);
        param.visit_mut_children_with(self);
    }
    fn visit_mut_simple_assign_target(&mut self, target: &mut ast::SimpleAssignTarget) {
        if self.error.is_some() {
            return;
        }
        let ast::SimpleAssignTarget::Member(member) = target else {
            target.visit_mut_children_with(self);
            return;
        };
        let mut expr = ast::Expr::Member(member.clone());
        let original = base_name(&expr);
        match self.path(&mut expr) {
            Resolved::Symbol(symbol) => {
                self.check_field(&symbol, true, member.span);
                if !symbol.value || symbol.namespace.is_some() || symbol.enumeration.is_some() {
                    self.value(&mut expr, &symbol, &original);
                    return;
                }
                let span = member.span;
                *target = ast::SimpleAssignTarget::Ident(ast::BindingIdent {
                    id: self.renamed(&symbol, span),
                    type_ann: None,
                });
            }
            Resolved::Done => {
                if let ast::Expr::Member(rewritten) = expr {
                    *member = rewritten;
                }
                member.visit_mut_children_with(self);
            }
            Resolved::Other => member.visit_mut_children_with(self),
        }
    }
    fn visit_mut_update_expr(&mut self, update: &mut ast::UpdateExpr) {
        if let ast::Expr::Member(member) = &*update.arg {
            let mut expr = ast::Expr::Member(member.clone());
            if let Resolved::Symbol(symbol) = self.path(&mut expr) {
                self.check_field(&symbol, true, member.span);
            }
        }
        update.visit_mut_children_with(self);
    }
    fn visit_mut_class_decl(&mut self, class: &mut ast::ClassDecl) {
        self.classes.push(class.ident.sym.to_string());
        class.visit_mut_children_with(self);
        self.classes.pop();
    }
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        if self.error.is_some() || self.imported_write(expr) {
            return;
        }
        match expr {
            ast::Expr::Ident(id) => {
                let original = id.sym.to_string();
                if let Some(symbol) = self.lookup(&original)
                    && (symbol.structured() || symbol.name != original)
                {
                    self.value(expr, &symbol, &original);
                }
            }
            ast::Expr::Member(_) => {
                let original = base_name(expr);
                match self.path(expr) {
                    Resolved::Symbol(symbol) => self.value(expr, &symbol, &original),
                    Resolved::Done | Resolved::Other => {}
                }
                expr.visit_mut_children_with(self);
            }
            _ => expr.visit_mut_children_with(self),
        }
    }
    fn visit_mut_prop(&mut self, prop: &mut ast::Prop) {
        if let ast::Prop::Shorthand(id) = prop
            && let Some(symbol) = self.lookup(id.sym.as_ref())
            && (symbol.structured() || symbol.name != id.sym.as_ref())
        {
            let key = ast::PropName::Ident(ast::IdentName::new(id.sym.clone(), id.span));
            let mut value = ast::Expr::Ident(id.clone());
            let original = id.sym.to_string();
            self.value(&mut value, &symbol, &original);
            *prop = ast::Prop::KeyValue(ast::KeyValueProp {
                key,
                value: Box::new(value),
            });
            return;
        }
        prop.visit_mut_children_with(self);
    }
    fn visit_mut_ts_type(&mut self, ty: &mut ast::TsType) {
        if self.error.is_some() {
            return;
        }
        if let ast::TsType::TsTypeRef(reference) = ty {
            match self.type_path(&reference.type_name) {
                Some(TypeResolved::Literal(value)) => {
                    *ty = enums::literal_type(&value, reference.span);
                    return;
                }
                Some(TypeResolved::Symbol(symbol, original)) => {
                    if symbol.namespace.is_some() && symbol.enumeration.is_none() && !symbol.value {
                        self.fail(
                            type_error(format!("namespace {original} used as a type")),
                            reference.span,
                        );
                        return;
                    }
                    if symbol.name != original
                        || matches!(reference.type_name, ast::TsEntityName::TsQualifiedName(_))
                    {
                        let id = self.renamed(&symbol, reference.span);
                        reference.type_name = ast::TsEntityName::Ident(id);
                    }
                }
                None => {}
            }
        }
        if let ast::TsType::TsTypeQuery(query) = ty
            && let ast::TsTypeQueryExpr::TsEntityName(name) = &query.expr_name
            && let Some(TypeResolved::Symbol(symbol, original)) = self.type_path(name)
        {
            if let Some(object) = symbol
                .enumeration
                .as_ref()
                .and_then(|e| e.object.clone())
                .or_else(|| symbol.namespace.as_ref().and_then(|n| n.object.clone()))
                && !symbol.value
            {
                // `typeof E`: the type of the enum object.
                let id = self.renamed(&Symbol::plain(object), query.span);
                query.expr_name = ast::TsTypeQueryExpr::TsEntityName(ast::TsEntityName::Ident(id));
                return;
            }
            if symbol.structured() {
                self.fail(
                    unsupported("typeof of a const enum or namespace object"),
                    query.span,
                );
                return;
            }
            if symbol.name != original || matches!(name, ast::TsEntityName::TsQualifiedName(_)) {
                let id = self.renamed(&symbol, query.span);
                query.expr_name = ast::TsTypeQueryExpr::TsEntityName(ast::TsEntityName::Ident(id));
            }
        }
        ty.visit_mut_children_with(self);
    }
    fn visit_mut_ts_expr_with_type_args(&mut self, heritage: &mut ast::TsExprWithTypeArgs) {
        // `implements NS.I` and `interface X extends NS.I` name types.
        if let Some(name) = entity(&heritage.expr)
            && let Some(TypeResolved::Symbol(symbol, original)) = self.type_path(&name)
            && (symbol.name != original || matches!(name, ast::TsEntityName::TsQualifiedName(_)))
        {
            let id = self.renamed(&symbol, heritage.span);
            *heritage.expr = ast::Expr::Ident(id);
        }
        heritage.type_args.visit_mut_with(self);
    }
    fn visit_mut_function(&mut self, function: &mut ast::Function) {
        let mut names = Names::new();
        for param in &function.params {
            scope::pattern(&param.pat, &mut names);
        }
        if let Some(body) = &function.body {
            scope::hoisted(&body.stmts, &mut names);
            scope::block(&body.stmts, &mut names);
        }
        self.push_scope(names);
        function.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_constructor(&mut self, constructor: &mut ast::Constructor) {
        let mut names = Names::new();
        for param in &constructor.params {
            match param {
                ast::ParamOrTsParamProp::Param(param) => scope::pattern(&param.pat, &mut names),
                ast::ParamOrTsParamProp::TsParamProp(prop) => match &prop.param {
                    ast::TsParamPropParam::Ident(id) => {
                        names.insert(id.id.sym.to_string());
                    }
                    ast::TsParamPropParam::Assign(assign) => {
                        scope::pattern(&assign.left, &mut names)
                    }
                },
            }
        }
        if let Some(body) = &constructor.body {
            scope::hoisted(&body.stmts, &mut names);
            scope::block(&body.stmts, &mut names);
        }
        self.push_scope(names);
        constructor.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_arrow_expr(&mut self, arrow: &mut ast::ArrowExpr) {
        for param in &mut arrow.params {
            self.widen_default(param);
        }
        let mut names = Names::new();
        for param in &arrow.params {
            scope::pattern(param, &mut names);
        }
        if let ast::ArrowFunctionBody::FunctionBody(body) = &*arrow.body {
            scope::hoisted(&body.stmts, &mut names);
            scope::block(&body.stmts, &mut names);
        }
        self.push_scope(names);
        arrow.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_block_stmt(&mut self, block: &mut ast::BlockStmt) {
        let mut names = Names::new();
        scope::block(&block.stmts, &mut names);
        self.push_scope(names);
        block.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_for_stmt(&mut self, statement: &mut ast::ForStmt) {
        let mut names = Names::new();
        if let Some(ast::VarDeclOrExpr::VarDecl(var)) = &statement.init {
            for declarator in &var.decls {
                scope::pattern(&declarator.name, &mut names);
            }
        }
        self.push_scope(names);
        statement.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_for_in_stmt(&mut self, statement: &mut ast::ForInStmt) {
        self.push_scope(head(&statement.left));
        statement.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_for_of_stmt(&mut self, statement: &mut ast::ForOfStmt) {
        self.push_scope(head(&statement.left));
        statement.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_catch_clause(&mut self, clause: &mut ast::CatchClause) {
        let mut names = Names::new();
        if let Some(param) = &clause.param {
            scope::pattern(param, &mut names);
        }
        self.push_scope(names);
        clause.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_class_expr(&mut self, class: &mut ast::ClassExpr) {
        let mut names = Names::new();
        if let Some(id) = &class.ident {
            names.insert(id.sym.to_string());
        }
        self.push_scope(names);
        class.visit_mut_children_with(self);
        self.scopes.pop();
    }
    fn visit_mut_fn_expr(&mut self, function: &mut ast::FnExpr) {
        let mut names = Names::new();
        if let Some(id) = &function.ident {
            names.insert(id.sym.to_string());
        }
        self.push_scope(names);
        function.visit_mut_children_with(self);
        self.scopes.pop();
    }
}

fn head(left: &ast::ForHead) -> Names {
    let mut names = Names::new();
    if let ast::ForHead::VarDecl(var) = left {
        for declarator in &var.decls {
            scope::pattern(&declarator.name, &mut names);
        }
    }
    names
}

/// The entity name of a heritage expression `A.B.C`.
fn entity(expr: &ast::Expr) -> Option<ast::TsEntityName> {
    match expr {
        ast::Expr::Ident(id) => Some(ast::TsEntityName::Ident(id.clone())),
        ast::Expr::Member(member) => {
            let ast::MemberProp::Ident(right) = &member.prop else {
                return None;
            };
            Some(ast::TsEntityName::TsQualifiedName(Box::new(
                ast::TsQualifiedName {
                    span: member.span,
                    left: entity(&member.obj)?,
                    right: right.clone(),
                },
            )))
        }
        _ => None,
    }
}
