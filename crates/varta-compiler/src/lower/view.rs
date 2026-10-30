//! The engine-internal intrinsic of the v38 view prelude (`view_protocol`):
//! `varta.view.commit(batch)` lowers to the `view_commit` host import with the
//! batch string's bytes. It exists only inside the view prelude; in program
//! modules the name is an ordinary unknown call, so application code reaches
//! the view only through `View.*` and TSX.
use super::*;

/// The commit intrinsic of the view prelude.
const COMMIT: &str = "varta.view.commit";
/// The packing intrinsic of the view prelude (`ir::Intrinsic::Units`).
const UNITS: &str = "varta.view.units";

impl Lower<'_> {
    /// The intrinsic call `name(...)` of the view prelude, if it is one.
    pub(super) fn prelude_view(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        if !matches!(name, COMMIT | UNITS)
            || !self.types.profile.view_protocol()
            || !self
                .source
                .is_path(std::path::Path::new(crate::modules::engine::VIEW_PATH))
        {
            return Ok(None);
        }
        if name == UNITS {
            if call.type_args.is_some()
                || call.args.is_empty()
                || call.args.len() > ir::MAX_PACKED_UNITS
                || call.args.iter().any(|arg| arg.spread.is_some())
            {
                return Err(invalid(format!(
                    "{name} takes 1 to {} plain arguments",
                    ir::MAX_PACKED_UNITS
                )));
            }
            let args = call
                .args
                .iter()
                .map(|arg| self.expr(&arg.expr))
                .collect::<Result<Vec<_>>>()?;
            if args.iter().any(|arg| arg.ty != Type::Float64) {
                return Err(type_error(format!("{name} requires numbers")));
            }
            return Ok(Some(Expr {
                span: call.span,
                ty: Type::String,
                kind: E::Intrinsic(ir::Intrinsic::Units, args),
            }));
        }
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one argument")));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(invalid(format!("{name} expects one argument")));
        }
        let value = self.expr(&argument.expr)?;
        if value.ty != Type::String {
            return Err(type_error(format!("{name} requires a string")));
        }
        Ok(Some(Expr {
            span: call.span,
            ty: Type::Float64,
            kind: E::Host(Host::ViewCommit, vec![value]),
        }))
    }
}

/// The function the synthesized event export calls: the view prelude's
/// dispatcher, when the program loaded the prelude.
pub(super) fn event_function(types: &ir::ValueTypes) -> Option<usize> {
    if !types.profile.view_protocol() {
        return None;
    }
    let prelude = &crate::modules::engine::VIEW;
    engine::module(prelude, types)?;
    types
        .engine
        .function(prelude, crate::modules::engine::VIEW_DISPATCH)
        .map(|signature| signature.index)
}

#[cfg(test)]
mod tests {
    use varta_common::view;

    const VIEW: &str = include_str!("../prelude/view.ts");

    /// The prelude's mirrored wire constants match the shared registry.
    #[test]
    fn the_view_prelude_mirrors_the_registry() {
        let constant = |name: String, value: String| {
            let line = format!("const {name} = {value};");
            assert!(VIEW.contains(&line), "view.ts lacks `{line}`");
        };
        constant("DRAFT_VERSION".into(), view::DRAFT_VERSION.to_string());
        constant("ROOT_NODE".into(), view::ROOT_NODE.to_string());
        constant("NO_NODE".into(), view::NO_NODE.to_string());
        constant("ANY_KEY".into(), view::ANY_KEY.to_string());
        for opcode in view::Opcode::ALL {
            constant(
                format!("OP_{}", opcode.name().to_ascii_uppercase()),
                opcode.code().to_string(),
            );
        }
        for kind in [
            view::EventKind::Input,
            view::EventKind::Change,
            view::EventKind::CommandResult,
        ] {
            constant(
                format!("EVENT_{}", kind.name().to_ascii_uppercase()),
                kind.code().to_string(),
            );
        }
        for flag in view::EventFlag::ALL {
            let name = match flag {
                view::EventFlag::Shift => "FLAG_SHIFT",
                view::EventFlag::Ctrl => "FLAG_CTRL",
                view::EventFlag::Alt => "FLAG_ALT",
                view::EventFlag::Meta => "FLAG_META",
                view::EventFlag::Composing => "FLAG_COMPOSING",
                view::EventFlag::Checked => "FLAG_CHECKED",
            };
            constant(name.into(), (*flag as u32).to_string());
        }
        constant(
            "STATUS_OK".into(),
            view::CommandStatus::Ok.code().to_string(),
        );
        for field in view::EVENT_FIELDS {
            if field.name == "version" {
                continue;
            }
            constant(
                format!("RECORD_{}", field.name.to_ascii_uppercase()),
                (field.offset / 2).to_string(),
            );
        }
        constant(
            "RECORD_STRINGS".into(),
            (view::EVENT_HEADER_BYTES / 2).to_string(),
        );
        constant(
            "COPY_TEXT_RESULT".into(),
            format!("'{}'", view::ResultOperation::CopyText.name()),
        );
    }
}
