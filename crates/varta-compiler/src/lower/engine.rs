//! Calls into the engine preludes of v37 (`collections`). Built-in methods and
//! collection operations are typed guest functions of a prelude module; the
//! compiler calls them by name for receivers of the matching type, so no
//! source binding is needed and a module that only receives a collection from
//! another module still reaches them. A prelude is in the graph whenever any
//! module names one of its load triggers (`modules::engine::Prelude::loads`).
use super::*;
use crate::modules::engine::Prelude;

#[derive(Default)]
pub(crate) struct EngineModules {
    /// Module index of each loaded prelude, by reserved path.
    modules: BTreeMap<&'static str, usize>,
    /// Non-generic functions of the loaded preludes, by prelude path and name.
    functions: BTreeMap<(&'static str, String), Signature>,
}

/// Record a loaded prelude module and, once its declarations are known, its
/// non-generic functions.
pub(super) fn register(prelude: &'static Prelude, module: usize, types: &mut ir::ValueTypes) {
    types.engine.modules.insert(prelude.path, module);
}
pub(super) fn register_functions(
    prelude: &'static Prelude,
    symbols: &BTreeMap<String, Signature>,
    types: &mut ir::ValueTypes,
) {
    for (name, signature) in symbols {
        types
            .engine
            .functions
            .insert((prelude.path, name.clone()), signature.clone());
    }
}
/// The module index of `prelude`, if the program loaded it.
pub(super) fn module(prelude: &Prelude, types: &ir::ValueTypes) -> Option<usize> {
    types.engine.modules.get(prelude.path).copied()
}

fn missing(prelude: &Prelude, name: &str) -> CompilerError {
    CompilerError::Internal(format!(
        "engine function {name} of {} is not loaded",
        prelude.path
    ))
}

impl Lower<'_> {
    /// The signature of engine function `name` of `prelude`: a non-generic
    /// function, or the instantiation of a generic one with `type_args`.
    pub(super) fn engine_signature(
        &mut self,
        prelude: &'static Prelude,
        name: &str,
        type_args: &[Type],
    ) -> Result<Signature> {
        if let Some(signature) = self
            .types
            .engine
            .functions
            .get(&(prelude.path, name.into()))
        {
            return Ok(signature.clone());
        }
        let module = module(prelude, self.types).ok_or_else(|| missing(prelude, name))?;
        let id =
            generics::lookup(self.types, module, name).ok_or_else(|| missing(prelude, name))?;
        let args: Vec<Option<Type>> = type_args.iter().copied().map(Some).collect();
        generics::instantiate(id, &args, self.next_function, self.types)
    }
    /// Call engine function `name` of `prelude` with lowered `leading`
    /// arguments followed by the source arguments `trailing`, checked against
    /// the function's remaining parameters.
    pub(super) fn engine_call(
        &mut self,
        (prelude, name, type_args): (&'static Prelude, &str, &[Type]),
        leading: Vec<Expr>,
        trailing: &[ast::ExprOrSpread],
        span: swc_common::Span,
    ) -> Result<Expr> {
        let signature = self.engine_signature(prelude, name, type_args)?;
        let fixed = leading.len();
        if fixed > signature.params.len() {
            return Err(CompilerError::Internal(format!(
                "engine function {name} takes fewer arguments"
            )));
        }
        if trailing.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(unsupported("spread argument to a built-in method"), span));
        }
        let shape = ir::ParameterShape {
            optional: signature.shape.optional[fixed..].to_vec(),
            rest: signature.shape.rest,
        };
        let mut values = leading;
        values.extend(
            self.call_arguments(trailing, &signature.params[fixed..], &shape)
                .map_err(|e| self.source.locate(e, span))?,
        );
        let args = self.bind_arguments(values, &signature.params, &signature.shape, span)?;
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::Call(signature.index, args),
        })
    }
    /// Call engine function `name` with lowered arguments only.
    pub(super) fn engine_apply(
        &mut self,
        function: (&'static Prelude, &str, &[Type]),
        args: Vec<Expr>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        self.engine_call(function, args, &[], span)
    }
}
