//! The language items of the v38 SDK-path stream (`sdk_paths`): forms that
//! the pinned pilot SDK and its applications reach on their multipart, query
//! and error paths. `instanceof Map`/`Set`/`Uint8Array` of closed unions is
//! here; error `cause` reads, `new Error(message, options)` and the refused
//! `"cause" in` an Error instance are in `errors` and
//! `classes::error_classes`, `xs[i].m<T>()` in `function_fields`,
//! `for...of` over `FormData` in `for_of`, the view checks of spreads and
//! enumerations in `subtyping`, and `File` with the `FormData` entries in
//! the blob prelude.
use super::*;
use ir::CollectionKind as K;
use references::Guard;

impl Lower<'_> {
    /// The collection kind `Map`, `Set` or `Uint8Array` that the right
    /// operand of `instanceof` names, unless a binding, function or
    /// declaration shadows it (v38 `sdk_paths`).
    fn collection_callee(&self, expression: &ast::Expr) -> Option<K> {
        let ast::Expr::Ident(name) = stmt::unparen(expression) else {
            return None;
        };
        let name = name.sym.as_ref();
        if !self.types.profile.sdk_paths()
            || self.binding(name).is_ok()
            || self.functions.contains_key(name)
        {
            return None;
        }
        // `ReadonlyMap`/`ReadonlySet` are type names, not values.
        collections::builtin(name, self.types).filter(|kind| kind.name() == name)
    }
    /// The member of a value of type `ty` that `instanceof` of collection
    /// `kind` selects: `Ok(None)` when no member is such a collection.
    /// Collections carry no value tag of their own (a map or set shares the
    /// record tag, a `Uint8Array` the array tag), so a union whose other
    /// members share the selected member's tag, or an `unknown`/`object`
    /// value that never holds a collection, is refused rather than guessed.
    fn instance_of_member(&self, ty: Type, kind: K) -> Result<Option<Type>> {
        if matches!(ty, Type::Unknown | Type::Object(_)) {
            return Err(unsupported(&format!(
                "instanceof {} of an unknown or object value (a Map, Set or Uint8Array never \
                 enters unknown in value layout 18; test a closed union instead)",
                kind.name()
            )));
        }
        let members = unions::members(ty, self.types);
        let selected: Vec<Type> = members
            .iter()
            .copied()
            .filter(|member| matches!(member, Type::Collection(k, _) if *k == kind))
            .collect();
        let [member] = selected.as_slice() else {
            return if selected.is_empty() {
                Ok(None)
            } else {
                Err(unsupported(&format!(
                    "instanceof {} of a union with several {} types",
                    kind.name(),
                    kind.name()
                )))
            };
        };
        let tag = member.value_kind();
        if members
            .iter()
            .any(|other| other != member && other.value_kind() == tag)
        {
            return Err(unsupported(&format!(
                "instanceof {} of a union whose other members share its value tag \
                 (records, class instances or arrays)",
                kind.name()
            )));
        }
        Ok(Some(*member))
    }
    /// `value instanceof Map|Set|Uint8Array` (v38 `sdk_paths`): a member tag
    /// test of a closed union, or a constant for any other closed type (the
    /// operand is still evaluated once). `None` when the right operand is no
    /// such collection.
    pub(super) fn collection_instance_of(&mut self, binary: &ast::BinExpr) -> Result<Option<Expr>> {
        let Some(kind) = self.collection_callee(&binary.right) else {
            return Ok(None);
        };
        let value = self.expr(&binary.left)?;
        let member = self
            .instance_of_member(value.ty, kind)
            .map_err(|e| self.source.locate(e, binary.span))?;
        let span = binary.span;
        let kind = match member {
            Some(member) if value.ty.is_union() => E::IsUnionMember(Box::new(value), member),
            matched => E::Sequence(
                Box::new(value),
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(i32::from(matched.is_some())),
                }),
            ),
        };
        Ok(Some(Expr {
            span,
            ty: Type::Boolean,
            kind,
        }))
    }
    /// Narrowing by `name instanceof Map|Set|Uint8Array` (v38 `sdk_paths`):
    /// the selected collection member, or the rest of the union. Returns
    /// whether the test was such a guard.
    pub(super) fn collection_guard(&mut self, test: &ast::Expr, yes: bool) -> Result<bool> {
        let ast::Expr::Bin(binary) = test else {
            return Ok(false);
        };
        if binary.op != ast::BinaryOp::InstanceOf {
            return Ok(false);
        }
        let Some(kind) = self.collection_callee(&binary.right) else {
            return Ok(false);
        };
        let ast::Expr::Ident(id) = stmt::unparen(&binary.left) else {
            return Ok(true);
        };
        let name = id.sym.as_ref();
        let Ok(binding) = self.binding(name).or_else(|_| self.resolve_binding(name)) else {
            return Ok(true);
        };
        let Some(ty) = self.refinements.get(&binding.slot).copied().or(binding.ty) else {
            return Ok(true);
        };
        // An inadmissible operand fails when the test itself is lowered.
        if let Ok(Some(member)) = self.instance_of_member(ty, kind)
            && ty.is_union()
        {
            self.refine(name, Guard::Exact(member), yes)?;
        }
        Ok(true)
    }
}
