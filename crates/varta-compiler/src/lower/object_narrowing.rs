//! Narrowing `unknown` to TypeScript's `object` (v35 `unknown_json`).
//!
//! `typeof x === "object"` proves `object | null` and a following null or
//! truth test proves `object`; these are facts about the binding's `unknown`
//! cell, kept as `Type::Object` refinements and read as the `unknown` cell.
//! Only the operations TypeScript admits on `object` use the fact: `key in x`,
//! `Object.keys`/`values`/`entries`/`hasOwn` and `for...in`. They convert the
//! cell to `Record<string, unknown>` with the checked dictionary test (tag and
//! exact storage descriptor), which every JSON object built from `unknown`
//! passes; an array, record, class instance or Error held by the cell throws
//! `TypeError` there instead of being enumerated. A positive `"key" in x`
//! then refines the binding to an `object` view, a dictionary of `unknown`
//! that declares the proven keys, so `x.key` reads `unknown` (`undefined`
//! after a later delete) while undeclared and dynamic keys stay rejected.
use super::*;
use ir::RecordField;

/// The `TypeError` message of an `object` that is not a JSON-style dictionary.
const NOT_A_DICTIONARY: &str = "object held by unknown is not a Record<string, unknown> dictionary";

/// Whether `test` is `typeof <identifier> === "object"` (either operand order).
pub(super) fn typeof_object(test: &ast::Expr) -> bool {
    let ast::Expr::Bin(binary) = stmt::unparen(test) else {
        return false;
    };
    let side = |a: &ast::Expr, b: &ast::Expr| {
        matches!(a, ast::Expr::Unary(u) if u.op == ast::UnaryOp::TypeOf
            && matches!(&*u.arg, ast::Expr::Ident(_)))
            && matches!(b, ast::Expr::Lit(ast::Lit::Str(text)) if text.value.as_str() == Some("object"))
    };
    binary.op == ast::BinaryOp::EqEqEq
        && (side(&binary.left, &binary.right) || side(&binary.right, &binary.left))
}

/// The object view of `unknown` that declares `keys` (each `unknown`).
fn view(keys: Vec<String>, types: &mut ir::ValueTypes) -> Option<Type> {
    let fields = keys
        .into_iter()
        .map(|name| RecordField {
            name,
            ty: Type::Unknown,
            optional: false,
        })
        .collect();
    mixed_records::intern(Type::Unknown, fields, true, types).ok()
}

impl Lower<'_> {
    /// The `object` fact of an identifier operand: `Some(nullable)`.
    fn object_fact(&mut self, expression: &ast::Expr) -> Option<bool> {
        if !self.types.profile.unknown_json() {
            return None;
        }
        let ast::Expr::Ident(id) = stmt::unparen(expression) else {
            return None;
        };
        let binding = self
            .binding(id.sym.as_ref())
            .or_else(|_| self.resolve_binding(id.sym.as_ref()))
            .ok()?;
        match self.refinements.get(&binding.slot) {
            Some(Type::Object(nullable)) => Some(*nullable),
            _ => None,
        }
    }
    /// Lower an operand of `in`, `Object.keys`/`values`/`entries`/`hasOwn` or
    /// `for...in`: an identifier narrowed to `object` becomes the checked
    /// `Record<string, unknown>` view of its cell; anything else lowers as is.
    pub(super) fn object_value(&mut self, expression: &ast::Expr) -> Result<Expr> {
        let fact = self.object_fact(expression);
        let value = self.expr(expression)?;
        match fact {
            Some(false) => {
                let span = value.span;
                let dictionary = dictionaries::intern(Type::Unknown, self.types)?;
                self.assert_value(
                    value,
                    dictionary,
                    "Record<string, unknown>",
                    NOT_A_DICTIONARY,
                    span,
                )
            }
            Some(true) => Err(self.source.locate(
                type_error(
                    "object narrowed from unknown may be null; test the value against null first",
                ),
                value.span,
            )),
            None => Ok(value),
        }
    }
    /// `"key" in x` (positive branch): an `object` or object view gains the
    /// declared key. Inherited `Object.prototype` names prove no own key.
    pub(super) fn in_guard(&mut self, key: &ast::Expr, value: &ast::Expr) -> Result<()> {
        let (ast::Expr::Lit(ast::Lit::Str(text)), ast::Expr::Ident(id)) =
            (stmt::unparen(key), stmt::unparen(value))
        else {
            return Ok(());
        };
        let Some(name) = text.value.as_str().map(str::to_owned) else {
            return Ok(());
        };
        if records::inherited(&name) || records::key(name.clone()).is_err() {
            return Ok(());
        }
        self.refine_with(id.sym.as_ref(), |ty, types| {
            let mut keys: Vec<String> = match ty {
                Type::Object(false) => Vec::new(),
                Type::Dictionary(_) if types.declared(ty).is_some_and(|m| m.object) => types
                    .declared(ty)?
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect(),
                _ => return None,
            };
            if keys.contains(&name) {
                return None;
            }
            keys.push(name);
            view(keys, types).map(|ty| vec![ty])
        })
    }
}
