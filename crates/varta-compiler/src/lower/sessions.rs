//! The v40 session prelude's closed host boundary.
use super::*;
impl Lower<'_> {
    pub(super) fn prelude_session(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.protected_sessions()
            || !crate::modules::engine::is_prelude_source(self.source)
        {
            return Ok(None);
        }
        let (host, argument, result) = match name {
            "varta.sessionHost.read" => (Host::HttpSessionRead, None, Type::String),
            "varta.sessionHost.write" => (Host::HttpSessionWrite, Some(Type::String), Type::String),
            "varta.sessionHost.clear" => (Host::HttpSessionClear, None, Type::String),
            "varta.sessionHost.start" => {
                (Host::SessionRequestStart, Some(Type::String), Type::Float64)
            }
            "varta.sessionHost.reset" => (Host::SessionReset, None, Type::Void),
            _ => return Ok(None),
        };
        if call.type_args.is_some() || call.args.len() != usize::from(argument.is_some()) {
            return Err(invalid("invalid session intrinsic arguments"));
        }
        let mut arguments = Vec::new();
        if let Some(ty) = argument {
            let value = self.expr(&call.args[0].expr)?;
            if call.args[0].spread.is_some() || value.ty != ty {
                return Err(type_error("session intrinsic argument type"));
            }
            arguments.push(value);
        }
        Ok(Some(Expr {
            span: call.span,
            ty: result,
            kind: E::Host(host, arguments),
        }))
    }
}
