//! String and array methods of v37 (`collections`) implemented by the text
//! engine prelude (`modules::engine::TEXT`). The receiver is lowered once;
//! a class instance receiver keeps ordinary class dispatch.
use super::*;
use crate::modules::engine::TEXT;

/// `String.prototype` methods and their prelude functions.
const STRING_METHODS: [(&str, &str); 7] = [
    ("padStart", "stringPadStart"),
    ("padEnd", "stringPadEnd"),
    ("toUpperCase", "stringToUpperCase"),
    ("toLowerCase", "stringToLowerCase"),
    ("split", "stringSplit"),
    ("codePointAt", "stringCodePointAt"),
    ("concat", "stringConcat"),
];
/// `Array.prototype` methods of the text prelude.
const ARRAY_METHODS: [&str; 1] = ["join"];

impl Lower<'_> {
    /// `receiver.method(...)` for a v37 built-in string or array method, or a
    /// collection method on a receiver whose static type is a collection.
    pub(super) fn builtin_method(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Option<Expr>> {
        let ast::MemberProp::Ident(method) = &member.prop else {
            return Ok(None);
        };
        let name = method.sym.as_ref();
        let mut root = &*member.obj;
        while let ast::Expr::Member(inner) = stmt::unparen(root) {
            root = &inner.obj;
        }
        if matches!(stmt::unparen(root), ast::Expr::Ident(id) if id.sym == "varta") {
            // Engine intrinsics (`varta.text.toUpperCase`) are not methods.
            return Ok(None);
        }
        if self.collection_receiver(&member.obj, 0) {
            let receiver = self.expr(&member.obj)?;
            let Type::Collection(kind, id) = receiver.ty else {
                return Err(invalid("missing collection receiver type"));
            };
            return self
                .collection_method(call, receiver, (kind, id), name)
                .map(Some);
        }
        let string = STRING_METHODS.iter().find(|(method, _)| *method == name);
        if string.is_none() && !ARRAY_METHODS.contains(&name) {
            return Ok(None);
        }
        if call.type_args.is_some() {
            return Err(self
                .source
                .locate(invalid(format!("{name} has no type arguments")), call.span));
        }
        let receiver = self.expr(&member.obj)?;
        if matches!(receiver.ty, Type::Class(_)) {
            return self
                .class_method_call(call, member, Some(receiver))
                .map(Some);
        }
        if let Type::Collection(kind, id) = receiver.ty {
            return self
                .collection_method(call, receiver, (kind, id), name)
                .map(Some);
        }
        match (string, receiver.ty) {
            (Some((_, function)), Type::String) => self
                .engine_call(
                    (&TEXT, function, &[]),
                    vec![receiver],
                    &call.args,
                    call.span,
                )
                .map(Some),
            (None, Type::Array(_) | Type::Tuple(_)) => {
                let array = self.physical_array(receiver)?;
                self.array_join(call, array).map(Some)
            }
            (Some(_), _) => Err(self.source.locate(
                type_error(format!(
                    "String.prototype.{name} requires a string receiver, found {}",
                    receiver.ty
                )),
                receiver.span,
            )),
            (None, _) => Err(self.source.locate(
                type_error(format!(
                    "Array.prototype.{name} requires an array receiver, found {}",
                    receiver.ty
                )),
                receiver.span,
            )),
        }
    }
    /// `array.join(separator?)` over primitive elements.
    fn array_join(&mut self, call: &ast::CallExpr, array: Expr) -> Result<Expr> {
        let Type::Array(id) = array.ty else {
            return Err(invalid("missing join array type"));
        };
        let element = self.types.arrays[id as usize];
        let primitive = unions::members(element, self.types).iter().all(|ty| {
            matches!(
                ty.erased(),
                Type::String
                    | Type::Float64
                    | Type::Number
                    | Type::Boolean
                    | Type::BigInt
                    | Type::Undefined
                    | Type::Null
            )
        });
        if !primitive {
            return Err(self.source.locate(
                unsupported("Array.prototype.join of non-primitive elements (object to string conversion is not modelled)"),
                array.span,
            ));
        }
        self.engine_call(
            (&TEXT, "arrayJoin", &[element]),
            vec![array],
            &call.args,
            call.span,
        )
    }
    /// The array value that physically holds a tuple (v37); other arrays as is.
    pub(super) fn physical_array(&mut self, value: Expr) -> Result<Expr> {
        match value.ty {
            Type::Tuple(id) => {
                let ty = self.types.tuples[id as usize].array;
                Ok(Expr {
                    span: value.span,
                    ty,
                    kind: E::Reinterpret(Box::new(value)),
                })
            }
            _ => Ok(value),
        }
    }
    /// Whether `expression` statically is a collection: a typed binding or
    /// field, `new Map/Set/Uint8Array(...)`, or a chained `add`/`set` call.
    pub(super) fn collection_receiver(&self, expression: &ast::Expr, depth: usize) -> bool {
        if depth >= 64 {
            return false;
        }
        if matches!(
            self.peek_type(expression, depth),
            Some(Type::Collection(..))
        ) {
            return true;
        }
        match stmt::unparen(expression) {
            ast::Expr::New(new) => matches!(
                stmt::unparen(&new.callee),
                ast::Expr::Ident(id) if collections::builtin(id.sym.as_ref(), self.types).is_some()
                    && self.binding(id.sym.as_ref()).is_err()
            ),
            ast::Expr::Call(call) => {
                let ast::Callee::Expr(callee) = &call.callee else {
                    return false;
                };
                match stmt::unparen(callee) {
                    ast::Expr::Member(member) => {
                        let ast::MemberProp::Ident(method) = &member.prop else {
                            return false;
                        };
                        if matches!(method.sym.as_ref(), "add" | "set") {
                            return self.collection_receiver(&member.obj, depth + 1);
                        }
                        // A class method declared to return a collection
                        // (`new TextEncoder().encode(text)`).
                        self.class_receiver(&member.obj, depth + 1)
                            .and_then(|class| {
                                self.types
                                    .classes
                                    .get(class)
                                    .methods
                                    .get(method.sym.as_ref())
                                    .map(|m| m.signature.result)
                            })
                            .is_some_and(|ty| matches!(ty, Type::Collection(..)))
                    }
                    // A function declared to return a collection.
                    ast::Expr::Ident(name) => self
                        .functions
                        .get(name.sym.as_ref())
                        .is_some_and(|f| matches!(f.result, Type::Collection(..))),
                    _ => false,
                }
            }
            _ => false,
        }
    }
}
