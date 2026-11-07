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
use crate::modules::engine::{AGGREGATE_ERROR_CLASS, CONSOLE, DATE_CLASS};
use varta_common::console::{RenderMode, StderrLevel};
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

/// A console method of v38 (`node_console`, `console_methods`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Method {
    /// `log`, `info`, `debug`, `dirxml` (and the bare `log`).
    Log,
    Error,
    Warn,
    Trace,
    Assert,
    Dir,
    Table,
    /// `group` and `groupCollapsed`.
    Group,
    GroupEnd,
    Count,
    CountReset,
    Time,
    TimeEnd,
    TimeLog,
}

impl Method {
    /// The method a callee name calls under `profile`, if any.
    pub(super) fn of(name: &str, profile: varta_common::language::LanguageProfile) -> Option<Self> {
        if !profile.node_console() {
            return None;
        }
        let method = match name {
            "console.log" | "log" | "console.info" | "console.debug" => Self::Log,
            _ if !profile.console_methods() => return None,
            "console.dirxml" => Self::Log,
            "console.error" => Self::Error,
            "console.warn" => Self::Warn,
            "console.trace" => Self::Trace,
            "console.assert" => Self::Assert,
            "console.dir" => Self::Dir,
            "console.table" => Self::Table,
            "console.group" | "console.groupCollapsed" => Self::Group,
            "console.groupEnd" => Self::GroupEnd,
            "console.count" => Self::Count,
            "console.countReset" => Self::CountReset,
            "console.time" => Self::Time,
            "console.timeEnd" => Self::TimeEnd,
            "console.timeLog" => Self::TimeLog,
            _ => return None,
        };
        Some(method)
    }
}

/// The default label of the counter and timer methods.
const DEFAULT_LABEL: &str = "default";
/// The prelude-only intrinsics of the console prelude.
const FORMAT: &str = "varta.console.format";
const OUT: &str = "varta.console.out";
const ERROR: &str = "varta.console.error";
const WARN: &str = "varta.console.warn";
const INSPECT: &str = "varta.console.inspect";
const TABLE: &str = "varta.console.table";

impl Lower<'_> {
    /// A v38 console call (`node_console`, `console_methods`).
    pub(super) fn console_call(&mut self, call: &ast::CallExpr, method: Method) -> Result<Expr> {
        let span = call.span;
        let prelude = engine::module(&CONSOLE, self.types).is_some();
        let direct = |level: Option<StderrLevel>| (!prelude).then_some(level);
        match method {
            Method::Log => match direct(None) {
                Some(level) => self.console_direct(call, level),
                None => self.console_prelude(call, "consoleLog", &call.args),
            },
            Method::Error => match direct(Some(StderrLevel::Error)) {
                Some(level) => self.console_direct(call, level),
                None => self.console_prelude(call, "consoleError", &call.args),
            },
            Method::Warn => match direct(Some(StderrLevel::Warn)) {
                Some(level) => self.console_direct(call, level),
                None => self.console_prelude(call, "consoleWarn", &call.args),
            },
            Method::Trace => self.console_prelude(call, "consoleTrace", &call.args),
            Method::Group => self.console_prelude(call, "consoleGroup", &call.args),
            Method::Assert => {
                let (condition, rest) = match call.args.split_first() {
                    Some((condition, rest)) if condition.spread.is_none() => {
                        let value = self.expr(&condition.expr)?;
                        (self.logged_cell(value)?, rest)
                    }
                    Some((condition, _)) => {
                        return Err(self.source.locate(
                            unsupported("spread condition of console.assert"),
                            condition.expr.span(),
                        ));
                    }
                    // `console.assert()` fails: its condition is `undefined`.
                    None => (unknown::logged(undefined(span), self.types)?, &[][..]),
                };
                let args = self.console_arguments(span, rest)?;
                self.engine_apply(
                    (&CONSOLE, "consoleAssert", &[]),
                    vec![condition, args],
                    span,
                )
            }
            Method::GroupEnd => {
                if !call.args.is_empty() {
                    // Node's `groupEnd()` ignores its arguments, but still
                    // evaluates them.
                    let args = self.console_arguments(span, &call.args)?;
                    let end =
                        self.engine_apply((&CONSOLE, "consoleGroupEnd", &[]), vec![], span)?;
                    return Ok(sequence(args, end));
                }
                self.engine_apply((&CONSOLE, "consoleGroupEnd", &[]), vec![], span)
            }
            Method::Count | Method::CountReset | Method::Time | Method::TimeEnd => {
                let function = match method {
                    Method::Count => "consoleCount",
                    Method::CountReset => "consoleCountReset",
                    Method::Time => "consoleTime",
                    _ => "consoleTimeEnd",
                };
                let label = self.console_label(call, call.args.first())?;
                self.engine_apply((&CONSOLE, function, &[]), vec![label], span)
            }
            Method::TimeLog => {
                let label = self.console_label(call, call.args.first())?;
                let rest = call.args.get(1..).unwrap_or_default();
                let args = self.console_arguments(span, rest)?;
                self.engine_apply((&CONSOLE, "consoleTimeLog", &[]), vec![label, args], span)
            }
            Method::Dir => {
                let mut values = Vec::with_capacity(1);
                let value = match call.args.first() {
                    Some(argument) if argument.spread.is_none() => {
                        let value = self.expr(&argument.expr)?;
                        self.logged_cell(value)?
                    }
                    Some(argument) => {
                        return Err(self.source.locate(
                            unsupported("spread argument of console.dir"),
                            argument.expr.span(),
                        ));
                    }
                    None => unknown::logged(undefined(span), self.types)?,
                };
                values.push(value);
                let options = call.args.get(1..2).unwrap_or_default();
                self.engine_call((&CONSOLE, "consoleDir", &[]), values, options, span)
            }
            Method::Table => {
                let data = match call.args.first() {
                    Some(argument) if argument.spread.is_none() => {
                        let value = self.expr(&argument.expr)?;
                        self.logged_cell(value)?
                    }
                    Some(argument) => {
                        return Err(self.source.locate(
                            unsupported("spread argument of console.table"),
                            argument.expr.span(),
                        ));
                    }
                    None => unknown::logged(undefined(span), self.types)?,
                };
                let properties = call.args.get(1..2).unwrap_or_default();
                if properties.is_empty() {
                    let none = self.console_undefined_properties(span)?;
                    return self.engine_apply(
                        (&CONSOLE, "consoleTable", &[]),
                        vec![data, none],
                        span,
                    );
                }
                self.engine_call(
                    (&CONSOLE, "consoleTable", &[]),
                    vec![data],
                    properties,
                    span,
                )
            }
        }
    }

    /// The `string[] | undefined` of an omitted `properties` argument.
    fn console_undefined_properties(&mut self, span: swc_common::Span) -> Result<Expr> {
        let signature = self.engine_signature(&CONSOLE, "consoleTable", &[])?;
        let ty = *signature
            .params
            .get(1)
            .ok_or_else(|| CompilerError::Internal("consoleTable takes two arguments".into()))?;
        unions::widen(undefined(span), Some(ty), self.types)
    }

    /// A counter or timer label: the string argument, or `"default"`.
    fn console_label(
        &mut self,
        call: &ast::CallExpr,
        argument: Option<&ast::ExprOrSpread>,
    ) -> Result<Expr> {
        let Some(argument) = argument else {
            return Ok(Expr {
                span: call.span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(DEFAULT_LABEL)),
            });
        };
        let value = self.expr(&argument.expr)?;
        if argument.spread.is_some() || value.ty.erased() != Type::String {
            return Err(self.source.locate(
                type_error("a console counter or timer label must be a string"),
                argument.expr.span(),
            ));
        }
        Ok(literals::erase(value, self.types))
    }

    /// Call the console prelude's `function` with the argument list `args`.
    fn console_prelude(
        &mut self,
        call: &ast::CallExpr,
        function: &str,
        args: &[ast::ExprOrSpread],
    ) -> Result<Expr> {
        let list = self.console_arguments(call.span, args)?;
        self.engine_apply((&CONSOLE, function, &[]), vec![list], call.span)
    }

    /// A call that writes its formatted arguments directly: to standard
    /// output (`level` `None`) or to standard error.
    fn console_direct(&mut self, call: &ast::CallExpr, level: Option<StderrLevel>) -> Result<Expr> {
        let profile = self.types.profile;
        let span = call.span;
        let text = match call.args.as_slice() {
            // One string prints as it is.
            [argument] if argument.spread.is_none() => {
                let value = self.expr(&argument.expr)?;
                if value.ty.erased() == Type::String {
                    literals::erase(value, self.types)
                } else {
                    let cell = self.logged_cell(value)?;
                    let list = self.argument_list(span, vec![Some(cell)])?;
                    formatted(span, unknown::widen(list, self.types)?)
                }
            }
            args => {
                let list = self.console_arguments(span, args)?;
                formatted(span, unknown::widen(list, self.types)?)
            }
        };
        let kind = match level {
            None => E::Host(profile.host_operation(Host::LogString), vec![text]),
            Some(level) => E::Host(Host::LogStderr16, vec![level_word(span, level), text]),
        };
        Ok(Expr {
            span,
            ty: Type::Void,
            kind,
        })
    }

    /// The `unknown[]` argument list of a console call: each argument
    /// boxed for logging, a spread argument's elements in source order.
    fn console_arguments(
        &mut self,
        span: swc_common::Span,
        args: &[ast::ExprOrSpread],
    ) -> Result<Expr> {
        if args.iter().any(|argument| argument.spread.is_some()) {
            // Spreads take the array literal path: `[a, ...b]` as `unknown[]`.
            let literal = ast::Expr::Array(ast::ArrayLit {
                span,
                elems: args.iter().cloned().map(Some).collect(),
            });
            let ty = arrays::intern(Type::Unknown, self.types)?;
            return self.expr_expected(&literal, Some(ty));
        }
        let mut cells = Vec::with_capacity(args.len());
        for argument in args {
            let value = self.expr(&argument.expr)?;
            cells.push(Some(self.logged_cell(value)?));
        }
        self.argument_list(span, cells)
    }

    /// The `unknown[]` array of `cells`.
    fn argument_list(&mut self, span: swc_common::Span, cells: Vec<Option<Expr>>) -> Result<Expr> {
        Ok(Expr {
            span,
            ty: arrays::intern(Type::Unknown, self.types)?,
            kind: E::Array(cells),
        })
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
            // A void call's value is `undefined`, as Node prints it.
            Type::Void => undefined_of(value),
            _ => value,
        };
        unknown::logged(value, self.types).map_err(|error| self.source.locate(error, span))
    }

    /// A prelude-only intrinsic of the console prelude, if `name` is one.
    pub(super) fn console_intrinsic(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.console_methods()
            || ![FORMAT, OUT, ERROR, WARN, INSPECT, TABLE].contains(&name)
        {
            return Ok(None);
        }
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one argument")));
        };
        if argument.spread.is_some() {
            return Err(invalid(format!("{name} takes a plain argument")));
        }
        let span = call.span;
        let value = self.expr(&argument.expr)?;
        let (ty, kind) = match name {
            OUT | ERROR | WARN => {
                if value.ty != Type::String {
                    return Err(type_error(format!("{name} requires a string")));
                }
                let kind = match name {
                    OUT => E::Host(
                        self.types.profile.host_operation(Host::LogString),
                        vec![value],
                    ),
                    ERROR => E::Host(
                        Host::LogStderr16,
                        vec![level_word(span, StderrLevel::Error), value],
                    ),
                    _ => E::Host(
                        Host::LogStderr16,
                        vec![level_word(span, StderrLevel::Warn), value],
                    ),
                };
                (Type::Void, kind)
            }
            _ => {
                if value.ty != arrays::intern(Type::Unknown, self.types)? {
                    return Err(type_error(format!("{name} requires an unknown[] list")));
                }
                let cell = unknown::widen(value, self.types)?;
                let kind = match name {
                    FORMAT => E::Host(Host::ValueFormatV12, vec![cell]),
                    INSPECT => E::Host(
                        Host::ConsoleRender,
                        vec![render_word(span, RenderMode::Inspect), cell],
                    ),
                    _ => E::Host(
                        Host::ConsoleRender,
                        vec![render_word(span, RenderMode::Table), cell],
                    ),
                };
                (Type::String, kind)
            }
        };
        Ok(Some(Expr { span, ty, kind }))
    }
}

/// `undefined`.
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
/// `undefined` after evaluating the void `value`.
fn undefined_of(value: Expr) -> Expr {
    Expr {
        span: value.span,
        ty: Type::Undefined,
        kind: E::Undefined(Box::new(value)),
    }
}
/// `left, right`: evaluate the argument list, then the call.
fn sequence(left: Expr, right: Expr) -> Expr {
    Expr {
        span: right.span,
        ty: right.ty,
        kind: E::Sequence(Box::new(left), Box::new(right)),
    }
}
/// The linked text of an argument list cell (`value_format_v12`).
fn formatted(span: swc_common::Span, list: Expr) -> Expr {
    Expr {
        span,
        ty: Type::String,
        kind: E::Host(Host::ValueFormatV12, vec![list]),
    }
}
/// The level word of `log_stderr16`.
fn level_word(span: swc_common::Span, level: StderrLevel) -> Expr {
    Expr {
        span,
        ty: Type::Number,
        kind: E::Integer(level as i32),
    }
}
/// The mode word of `console_render`.
fn render_word(span: swc_common::Span, mode: RenderMode) -> Expr {
    Expr {
        span,
        ty: Type::Number,
        kind: E::Integer(mode as i32),
    }
}
