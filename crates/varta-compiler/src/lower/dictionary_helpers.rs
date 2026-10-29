//! Shared dictionary operation functions (v35 `mixed_index_records`).
//!
//! V34 inlines every dictionary read, write and delete. A write alone is
//! several kilobytes of Wasm (own-key position search, tail shift, entry limit
//! and `__proto__` checks), so the v35 operations that multiply them — mixed
//! records, compound and logical updates, spreads and `for...in` — would
//! exhaust the per-function body budget after a few dozen uses. V35 instead
//! lowers each operation once per dictionary value type into a synthesized
//! function with exactly the inline body, and every use becomes a call whose
//! arguments (receiver, key, value) are evaluated in source order. Mixed
//! records and `object` views pass their storage as the plain dictionary of
//! the same value type, which is the same layout.
use super::*;
use ir::DictionaryHelper;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    /// Call the shared `helper` for the dictionary `args[0]`; the remaining
    /// arguments are the string key and, for `Set`, the value of the index type.
    pub(super) fn dictionary_helper_call(
        &mut self,
        helper: DictionaryHelper,
        mut args: Vec<Expr>,
    ) -> Result<Expr> {
        let Some(Type::Dictionary(id)) = args.first().map(|arg| arg.ty) else {
            return Err(invalid("missing dictionary type"));
        };
        let value = self.types.dictionaries[id as usize].value;
        let plain = dictionaries::intern(value, self.types)?;
        let Type::Dictionary(plain_id) = plain else {
            return Err(invalid("missing dictionary type"));
        };
        // Declared members are a static view of the same storage.
        args[0].ty = plain;
        let span = args[0].span;
        let (params, result) = match helper {
            DictionaryHelper::Get => (
                vec![plain, Type::String],
                unions::intern(vec![value, Type::Undefined], self.types)?,
            ),
            DictionaryHelper::Set => (vec![plain, Type::String, value], value),
            DictionaryHelper::Delete => (vec![plain, Type::String], Type::Boolean),
        };
        let index = match self.types.dictionary_helpers.get(&(helper, plain_id)) {
            Some(index) => *index,
            None => {
                let index = self.synthesize_dictionary_helper(span, helper, params, result)?;
                self.types
                    .dictionary_helpers
                    .insert((helper, plain_id), index);
                index
            }
        };
        Ok(Expr {
            span,
            ty: result,
            kind: E::Call(index, args),
        })
    }
    /// Lower one helper with its own locals and insert it into the program.
    fn synthesize_dictionary_helper(
        &mut self,
        span: swc_common::Span,
        helper: DictionaryHelper,
        params: Vec<Type>,
        result: Type,
    ) -> Result<usize> {
        let index = *self.next_function;
        if index >= varta_common::abi::MAX_USER_FUNCTIONS {
            return Err(invalid(
                "too many functions including dictionary operations",
            ));
        }
        *self.next_function += 1;
        let mut child = Lower {
            source: self.source,
            types: self.types,
            functions: self.functions,
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
            call_effects: self.call_effects,
            reachable: true,
            control: Default::default(),
            guard_blocked: Default::default(),
            outer_narrows: Default::default(),
            result,
            infer_returns: false,
            entry: self.entry,
            loops: 0,
            labels: Vec::new(),
            next_label: 0,
            switches: 0,
            depth: self.depth,
            repeat_slots: None,
            generated: self.generated,
            next_function: self.next_function,
            index,
            outer: Default::default(),
            captures: Default::default(),
            cells: Default::default(),
            guarded: Default::default(),
            generator: None,
            asynchronous: None,
        };
        let slots: Vec<u32> = params.iter().map(|ty| child.hidden(*ty)).collect();
        let argument = |position: usize| local(span, slots[position], params[position]);
        let value = match helper {
            DictionaryHelper::Get => child.dictionary_get_inline(argument(0), argument(1))?,
            DictionaryHelper::Set => {
                let block = child.dictionary_block(argument(0), argument(1))?;
                child.dictionary_store(block, argument(2), result)?
            }
            DictionaryHelper::Delete => child.dictionary_delete_inline(argument(0), argument(1))?,
        };
        let function = ir::Function {
            shape: ir::ParameterShape::fixed(params.len()),
            self_slot: None,
            receiver: None,
            cells: std::mem::take(&mut child.cells),
            captures: Vec::new(),
            environment: false,
            generator: None,
            source: child.source.clone(),
            span,
            params,
            result,
            locals: (0..child.next)
                .map(|slot| {
                    child
                        .local_types
                        .get(&slot)
                        .copied()
                        .unwrap_or(Type::Number)
                })
                .collect(),
            body: vec![S::Return(Some(value))],
        };
        child.generated.insert(index, function);
        Ok(index)
    }
}
