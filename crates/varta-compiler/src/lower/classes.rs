//! Nominal classes (v32 `classes()`). A top-level class owns a private record
//! shape whose slots list inherited fields first, so a subclass instance is
//! layout-compatible with every ancestor. Instances are ordinary managed records
//! traced by their own reference mask; the canonical descriptor of the
//! most-derived class doubles as the runtime class identity used by
//! `instanceof` and by method dispatch. Classes are a closed world: every
//! subclass is known before bodies are lowered, so overridden methods dispatch
//! through one synthesized function per overridden root method.
use super::*;
use std::collections::BTreeSet;
use swc_common::Span;
use varta_common::value::MAX_RECORD_SHAPES;

const MAX_FUNCTIONS: usize = varta_common::abi::MAX_USER_FUNCTIONS;
/// Synthesized parameter prefix of an implicit derived constructor.
const IMPLICIT_PARAMETER: &str = "$super";
fn implicit_parameter(index: usize) -> String {
    format!("{IMPLICIT_PARAMETER}{index}")
}
const CONSTRUCTOR_NAME: &str = "constructor";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Access {
    Public,
    Protected,
    Private,
}
impl Access {
    fn from_ast(access: Option<ast::Accessibility>) -> Self {
        match access {
            None | Some(ast::Accessibility::Public) => Self::Public,
            Some(ast::Accessibility::Protected) => Self::Protected,
            Some(ast::Accessibility::Private) => Self::Private,
        }
    }
}

#[derive(Clone)]
pub(super) struct Field {
    pub name: String,
    pub ty: Type,
    pub owner: u32,
    pub access: Access,
    pub readonly: bool,
    /// The most-derived class that redeclared this inherited field (v35); its
    /// constructor may assign the field like the declaring class's.
    pub redeclared: Option<u32>,
}
#[derive(Clone)]
pub(super) struct Method {
    pub owner: u32,
    /// The ancestor that first declared this method name.
    pub root: u32,
    /// Implementation signature; `index` is the implementation function.
    pub signature: Signature,
    pub access: Access,
    /// Synthesized dispatcher when a descendant of `root` overrides the method.
    pub dispatch: Option<usize>,
    /// A `get` accessor: invoked by a property read with no arguments.
    pub getter: bool,
}
impl Method {
    /// The called function and its receiver class.
    pub(super) fn target(&self) -> (usize, u32) {
        match self.dispatch {
            Some(dispatch) => (dispatch, self.root),
            None => (self.signature.index, self.owner),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Member {
    Init,
    Method(String),
    Static(String),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Pending,
    Laying,
    Ready,
}

pub(super) struct Class {
    pub name: String,
    pub module: usize,
    pub source: Source,
    pub span: Span,
    declaration: Box<ast::Class>,
    pub parent: Option<u32>,
    pub shape: u32,
    pub fields: Vec<Field>,
    pub methods: BTreeMap<String, Method>,
    /// Static methods, own and inherited: receiver-free direct calls `C.m()`.
    pub statics: BTreeMap<String, Method>,
    pub init: Option<Signature>,
    /// Own field initializers in declaration order.
    initializers: Vec<(String, Box<ast::Expr>)>,
    state: State,
    /// Declared by an engine prelude module (v35 Web types).
    pub prelude: bool,
}

/// The class whose member bodies are being lowered; `init` is the function
/// index of its constructor initializer while that body is lowered.
#[derive(Clone, Copy)]
pub(super) struct Context {
    pub class: u32,
    pub init: Option<usize>,
}

struct Dispatcher {
    root: u32,
    name: String,
    index: usize,
}

#[derive(Default)]
pub(crate) struct Classes {
    list: Vec<Class>,
    pub(super) context: Option<Context>,
    /// Class function bodies in function-index order.
    order: Vec<(usize, u32, Member)>,
    dispatchers: Vec<Dispatcher>,
    method_names: BTreeSet<String>,
    /// Synthetic native-error root classes (v32 `error_subclasses`).
    error_roots: BTreeMap<varta_common::value::ErrorClass, u32>,
}
impl Classes {
    pub(crate) fn shape(&self, id: u32) -> u32 {
        self.list[id as usize].shape
    }
    pub(super) fn span(&self, id: u32) -> Span {
        self.list[id as usize].span
    }
    pub(super) fn get(&self, id: u32) -> &Class {
        &self.list[id as usize]
    }
    /// Whether some class declares a method of this name. Native intrinsic
    /// member calls of the same name are then no longer provably user-code free.
    pub(super) fn has_method(&self, name: &str) -> bool {
        self.method_names.contains(name)
    }
    /// The engine prelude class `name` (v35 Web types), when the program loaded it.
    pub(super) fn prelude_class(&self, name: &str) -> Option<u32> {
        self.list
            .iter()
            .position(|class| class.prelude && class.name == name)
            .map(|id| id as u32)
    }
    /// Whether the program loaded any engine prelude class.
    pub(super) fn has_prelude(&self) -> bool {
        self.list.iter().any(|class| class.prelude)
    }
    /// The explanation for a member of an engine prelude class (or of a
    /// subclass) that the profile names but does not implement.
    pub(super) fn pending_member(
        &self,
        mut class: u32,
        name: &str,
        profile: varta_common::language::LanguageProfile,
    ) -> Option<(&'static str, &'static str)> {
        loop {
            let info = &self.list[class as usize];
            if info.prelude
                && let Some(reason) = crate::modules::engine::pending(&info.name, name, profile)
            {
                return Some((reason, crate::modules::engine::pending_detail(&info.name)));
            }
            class = info.parent?;
        }
    }
    /// `descendant` equals `ancestor` or inherits from it.
    pub(super) fn descends(&self, mut descendant: u32, ancestor: u32) -> bool {
        loop {
            if descendant == ancestor {
                return true;
            }
            match self.list[descendant as usize].parent {
                Some(parent) => descendant = parent,
                None => return false,
            }
        }
    }
    /// Record shapes of `ancestor` and all of its descendants.
    pub(super) fn family_shapes(&self, ancestor: u32) -> Vec<u32> {
        (0..self.list.len() as u32)
            .filter(|id| self.descends(*id, ancestor))
            .map(|id| self.shape(id))
            .collect()
    }
    pub(super) fn field(&self, class: u32, name: &str) -> Option<(u32, &Field)> {
        self.list[class as usize]
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name)
            .map(|(slot, field)| (slot as u32, field))
    }
    fn allocate(&mut self, next: &mut usize, class: u32, member: Member) -> Result<usize> {
        if *next >= MAX_FUNCTIONS {
            return Err(invalid("too many functions across modules"));
        }
        let index = *next;
        *next += 1;
        self.order.push((index, class, member));
        Ok(index)
    }
}

/// `upcast` is exact or an ancestor class of `actual`.
pub(super) fn upcast(types: &ir::ValueTypes, actual: Type, expected: Type) -> bool {
    matches!((actual, expected), (Type::Class(a), Type::Class(e)) if types.classes.descends(a, e))
}

/// Retype a class value (or a union of them) whose class members widen to
/// ancestor class members of the target union. Union cells carry the record
/// tag for every class, so the payload is unchanged.
pub(super) fn union_upcast(value: Expr, target: Type, types: &ir::ValueTypes) -> Expr {
    let wanted = unions::members(target, types);
    let widened = |member: Type| {
        if wanted.contains(&member) {
            Some(member)
        } else {
            wanted.iter().copied().find(|t| upcast(types, member, *t))
        }
    };
    let actual = unions::members(value.ty, types);
    if !actual.iter().any(|m| matches!(m, Type::Class(_)))
        || actual.iter().all(|m| wanted.contains(m))
    {
        return value;
    }
    if !value.ty.is_union() {
        return match widened(value.ty) {
            Some(ty) => Expr { ty, ..value },
            None => value,
        };
    }
    if actual.iter().all(|m| widened(*m).is_some()) {
        Expr {
            ty: target,
            ..value
        }
    } else {
        value
    }
}

/// Retype the descendant operand of two related class values to their common
/// ancestor, so identity comparison accepts nominal subtypes.
pub(super) fn common_operands(left: Expr, right: Expr, types: &ir::ValueTypes) -> (Expr, Expr) {
    match (left.ty, right.ty) {
        (Type::Class(a), Type::Class(b)) if types.classes.descends(a, b) => (
            Expr {
                ty: right.ty,
                ..left
            },
            right,
        ),
        (Type::Class(a), Type::Class(b)) if types.classes.descends(b, a) => (
            left,
            Expr {
                ty: Type::Class(a),
                ..right
            },
        ),
        _ => (left, right),
    }
}

fn reserved(name: &str) -> bool {
    matches!(
        name,
        "VartaResult"
            | "Array"
            | "VartaHttpRequest"
            | "VartaHttpResponse"
            | "VartaHttpHeader"
            | "varta"
            | "console"
            | "log"
            | "JSON"
            | "Object"
            | "Math"
            | "Number"
            | "String"
            | "Boolean"
            | "BigInt"
    ) || varta_common::value::ErrorClass::from_name(name).is_some()
}

fn member_name(key: &ast::PropName) -> Result<String> {
    let name = match key {
        ast::PropName::Ident(id) => id.sym.to_string(),
        ast::PropName::Str(text) => text
            .value
            .as_str()
            .ok_or_else(|| unsupported("non-UTF-8 class member name"))?
            .to_owned(),
        _ => return Err(unsupported("computed or numeric class member name")),
    };
    if name == CONSTRUCTOR_NAME || records::inherited(&name) {
        return Err(unsupported(&format!(
            "class member named like the inherited {name}"
        )));
    }
    Ok(name)
}

fn receiver_width(signature: &Signature) -> Result<()> {
    if signature
        .receiver
        .into_iter()
        .chain(signature.params.iter().copied())
        .map(Type::width)
        .sum::<u32>() as usize
        > varta_common::abi::MAX_WASM_PARAMETERS
    {
        return Err(invalid("function parameters exceed 32 Wasm value slots"));
    }
    Ok(())
}

fn function_declaration(name: &str, span: Span, function: Box<ast::Function>) -> ast::FnDecl {
    ast::FnDecl {
        ident: ast::Ident::new(name.into(), span, Default::default()),
        declare: false,
        function,
    }
}

fn constructor_function(ctor: &ast::Constructor, source: &Source) -> Result<Box<ast::Function>> {
    let params = ctor
        .params
        .iter()
        .map(|param| match param {
            ast::ParamOrTsParamProp::Param(param) => Ok(param.clone()),
            ast::ParamOrTsParamProp::TsParamProp(prop) => {
                Err(source.locate(unsupported("constructor parameter property"), prop.span))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Box::new(ast::Function {
        params,
        decorators: Vec::new(),
        span: ctor.span,
        ctxt: ctor.ctxt,
        body: ctor.body.clone(),
        is_generator: false,
        is_async: false,
        type_params: None,
        return_type: None,
        this_param: None,
    }))
}

/// Register a top-level class declaration of `module` in the type namespace.
/// Returns its name and id; layout happens later. `export class` is scanned as
/// a module export, so classes join the type export tables and re-exports.
pub(super) fn register(
    item: &ast::ModuleItem,
    module: usize,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<Option<(String, u32)>> {
    if !types.profile.classes() {
        return Ok(None);
    }
    let declaration = match item {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Class(class))) => class,
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
            let ast::Decl::Class(class) = &export.decl else {
                return Ok(None);
            };
            class
        }
        _ => return Ok(None),
    };
    let name = declaration.ident.sym.to_string();
    if declaration.declare {
        return Err(source.locate(
            unsupported("ambient class declaration"),
            declaration.ident.span,
        ));
    }
    if reserved(&name) {
        return Err(source.locate(
            unsupported(&format!("class named like the built-in {name}")),
            declaration.ident.span,
        ));
    }
    if types.classes.list.len() >= MAX_RECORD_SHAPES {
        return Err(source.locate(invalid("class count exceeds limit"), declaration.ident.span));
    }
    let id = types.classes.list.len() as u32;
    types.classes.list.push(Class {
        name: name.clone(),
        module,
        source: source.clone(),
        span: declaration.ident.span,
        declaration: declaration.class.clone(),
        parent: None,
        shape: 0,
        fields: Vec::new(),
        methods: BTreeMap::new(),
        statics: BTreeMap::new(),
        init: None,
        initializers: Vec::new(),
        state: State::Pending,
        prelude: crate::modules::engine::is_prelude_source(source),
    });
    Ok(Some((name, id)))
}

impl Lower<'_> {
    /// The class callee of `new C` / `instanceof C`, unless shadowed by a value.
    pub(super) fn class_callee(&self, expr: &ast::Expr) -> Option<u32> {
        if !self.types.profile.classes() {
            return None;
        }
        let ast::Expr::Ident(id) = stmt::unparen(expr) else {
            return None;
        };
        let name = id.sym.as_ref();
        if self.binding(name).is_ok() || self.functions.contains_key(name) {
            return None;
        }
        named::class(self.types, name)
    }
}

mod constructors;
mod dispatch;
mod error_classes;
mod layout;
mod members;
pub(super) use dispatch::{bodies, dispatch_bodies, dispatchers};
pub(super) use error_classes::{
    MESSAGE, NAME, join_error_classes, new_target_name, reaches_instance, reaches_prelude,
};
pub(super) use layout::layout_module;
