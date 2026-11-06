//! Console output of v38 (`node_console`, K9 D1): `console.log`,
//! `console.info` and `console.debug` with any number of arguments, printed
//! as Node 24 prints them (`util.formatWithOptions` with the default
//! `util.inspect` options).
//!
//! A single string argument is logged as it is. Every other call boxes its
//! arguments into `unknown` cells (`unknown::logged`, which also admits Map
//! and Set storage, `Date` instances and instances with `#private` fields),
//! collects them in an `unknown[]` array and logs the text the linked
//! `value_format_v12` formats for the array's cell: `%` directives of a
//! leading string, then every remaining argument, strings as they are and
//! other values in Node's `util.inspect` notation. Class names and the
//! engine-owned layouts reach that notation through the display trailers
//! of value18 record descriptors ([`record_displays`]).
use super::*;
use crate::modules::engine::{AGGREGATE_ERROR_CLASS, DATE_CLASS};
use varta_common::value::RecordDisplay;

/// The display trailer of every record shape that is not structural, by
/// shape id: class instances (Error classes, the engine's `AggregateError`
/// and `Date`) and the Map and Set storage of the collections prelude.
/// Empty for profiles before v38.
pub(crate) fn record_displays(types: &ir::ValueTypes) -> BTreeMap<u32, (RecordDisplay, String)> {
    let mut displays = BTreeMap::new();
    if !types.profile.node_console() {
        return displays;
    }
    for (id, class) in types.classes.laid_out() {
        // A generic class instance prints as its template's name.
        let name = class
            .template
            .map_or(&class.name, |template| &types.classes.get(template).name);
        let display = if class.prelude && name == DATE_CLASS {
            RecordDisplay::Date
        } else if class.prelude && name == AGGREGATE_ERROR_CLASS {
            RecordDisplay::AggregateError
        } else if types.classes.is_error(id) {
            RecordDisplay::Error
        } else {
            RecordDisplay::Class
        };
        displays
            .entry(class.shape)
            .or_insert_with(|| (display, name.clone()));
    }
    for collection in &types.collections {
        let display = match collection.kind {
            ir::CollectionKind::Map => RecordDisplay::Map,
            ir::CollectionKind::Set => RecordDisplay::Set,
            ir::CollectionKind::Uint8Array => continue,
        };
        if let Type::Record(id) = collection.storage {
            displays
                .entry(id)
                .or_insert_with(|| (display, collection.kind.name().to_owned()));
        }
    }
    displays
}

impl Lower<'_> {
    /// `console.log(...)` and its v38 aliases (`node_console`).
    pub(super) fn console_log(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let profile = self.types.profile;
        let mut cells = Vec::with_capacity(call.args.len());
        for argument in &call.args {
            if argument.spread.is_some() {
                return Err(self.source.locate(
                    unsupported("spread argument of console.log"),
                    argument.expr.span(),
                ));
            }
            let value = self.expr(&argument.expr)?;
            if call.args.len() == 1 && value.ty.erased() == Type::String {
                // One string prints as it is.
                let value = literals::erase(value, self.types);
                return Ok(Expr {
                    span: call.span,
                    ty: Type::Void,
                    kind: E::Host(profile.host_operation(Host::LogString), vec![value]),
                });
            }
            cells.push(Some(self.logged_cell(value)?));
        }
        let array = Expr {
            span: call.span,
            ty: arrays::intern(Type::Unknown, self.types)?,
            kind: E::Array(cells),
        };
        let cell = unknown::widen(array, self.types)?;
        Ok(expr::logged_text(profile, call.span, cell))
    }

    /// The `unknown` cell of one logged argument.
    fn logged_cell(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        if collections::reaches_unlogged(value.ty, self.types)
            || classes::reaches_json_form(value.ty, self.types)
        {
            return Err(self.source.locate(
                unsupported(
                    "logging of a value holding a URL, a RegExp match array or another builtin \
                     object without a Node notation",
                ),
                span,
            ));
        }
        let value = match value.ty {
            // Map and Set values are their prelude storage records.
            Type::Collection(ir::CollectionKind::Map | ir::CollectionKind::Set, _) => {
                self.storage(value)
            }
            Type::Void => {
                return Err(self.source.locate(
                    type_error("cannot log a void expression; Node prints undefined"),
                    span,
                ));
            }
            _ => value,
        };
        unknown::logged(value, self.types).map_err(|error| self.source.locate(error, span))
    }
}
