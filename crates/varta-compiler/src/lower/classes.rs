//! Nominal classes (v32 `classes()`). A top-level class owns a private record
//! shape whose slots list inherited fields first, so a subclass instance is
//! layout-compatible with every ancestor. Instances are ordinary managed records
//! traced by their own reference mask; the canonical descriptor of the
//! most-derived class doubles as the runtime class identity used by
//! `instanceof` and by method dispatch. Classes are a closed world: every
//! subclass is known before bodies are lowered, so overridden methods dispatch
//! through one synthesized function per overridden root method.
use super::*;
use std::collections::{BTreeSet, VecDeque};
use std::rc::Rc;
use swc_common::Span;

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
/// A generic method (v37 `class_generics`): it gets no function slot; each
/// distinct type-argument list is one method function of its owner.
pub(super) struct GenericMethod {
    pub owner: u32,
    /// The ancestor that first declared this method name.
    pub root: u32,
    pub access: Access,
    pub is_static: bool,
    /// The declaration without its type parameters, lowered per instance.
    pub function: Rc<generics::Function>,
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
    /// Generic instance and static methods, own and inherited (v37).
    pub generic_methods: BTreeMap<String, Rc<GenericMethod>>,
    pub generic_statics: BTreeMap<String, Rc<GenericMethod>>,
    pub init: Option<Signature>,
    /// Own field initializers in declaration order.
    initializers: Vec<(String, Box<ast::Expr>)>,
    state: State,
    /// Declared by an engine prelude module (v35 Web types).
    pub prelude: bool,
    /// Declared by a v37 builtin prelude (`collections`): its instances never
    /// enter `unknown`, so they never reach a host value reader through it.
    pub opaque: bool,
    /// Type parameters of a generic class template (v37 `class_generics`).
    /// A template has only static members; each instantiation is a class of
    /// its own with `template` set.
    params: Option<Rc<ast::TsTypeParamDecl>>,
    /// The template of a generic class instance.
    pub template: Option<u32>,
    /// The type arguments of a generic class instance, by parameter name.
    pub frame: generics::Frame,
    /// Own members of an instance whose function slots are not allocated yet.
    unallocated: Vec<Member>,
    /// V38 (`web_bytes`): the same-named interface merged with the class.
    merged: Option<merged_interfaces::MergedInterface>,
}
impl Class {
    /// A generic class declaration (not one of its instances).
    pub(super) fn is_template(&self) -> bool {
        self.params.is_some()
    }
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
    /// Generic method instances by (owner, name, static, type arguments).
    method_instances: BTreeMap<generic_methods::InstanceKey, Signature>,
    /// Dispatchers of overridden generic methods by (root, name, arguments).
    generic_dispatchers: BTreeMap<(u32, String, Vec<Type>), usize>,
    /// Generic method instances whose bodies are not lowered yet.
    pending_methods: VecDeque<generic_methods::Pending>,
    /// Generic class instances by (template, type arguments).
    instances: BTreeMap<(u32, Vec<Type>), u32>,
    /// Instances created before every declared class was laid out.
    deferred: Vec<u32>,
    /// Every declared class is laid out: instances lay out when created.
    ready: bool,
    /// Instance member bodies to lower: (function index, class, member).
    pending_members: VecDeque<generic_classes::PendingMember>,
}
/// The type name under which the merged interface of class `class` is
/// declared (v38 `web_bytes`): no identifier can spell it, and diagnostics
/// read `interface C`.
pub(super) fn merged_type_name(class: &str) -> String {
    format!("interface {class}")
}

impl Classes {
    /// Merge `interface` into the same-named class `id` (v38 `web_bytes`):
    /// its members become instance fields that the constructor provides
    /// with `Object.assign(this, …)`. Generic classes and interfaces merge
    /// no members.
    pub(super) fn merge_interface(
        &mut self,
        id: u32,
        interface: interfaces::Interface,
    ) -> Result<()> {
        let class = &mut self.list[id as usize];
        if class.is_template() || interface.params().is_some() || class.merged.is_some() {
            return Err(interface.source().locate(
                unsupported(&format!(
                    "declaration merging of a generic interface or class {}",
                    class.name
                )),
                interface.span(),
            ));
        }
        class.merged = Some(merged_interfaces::MergedInterface {
            interface: Box::new(interface),
            members: Vec::new(),
        });
        Ok(())
    }
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
    /// Whether the program loaded any engine prelude class that may enter
    /// `unknown` (v37 builtin classes never do).
    pub(super) fn has_prelude(&self) -> bool {
        self.list.iter().any(|class| class.prelude && !class.opaque)
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
    pub(crate) fn family_shapes(&self, ancestor: u32) -> Vec<u32> {
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
    fn allocate(
        &mut self,
        (next, limit): (&mut usize, usize),
        class: u32,
        member: Member,
    ) -> Result<usize> {
        if *next >= limit {
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
    if name == ASYNC_ITERATOR_METHOD {
        return Err(unsupported(&format!("class member named {name}")));
    }
    Ok(name)
}

/// The name a `[Symbol.asyncIterator]()` method is registered under (v37
/// `net_request`). A string key cannot register it (`member_name` rejects
/// the spelling), so only the well-known symbol reaches it.
pub(super) const ASYNC_ITERATOR_METHOD: &str = "[Symbol.asyncIterator]";

/// Whether `key` is the computed key `[Symbol.asyncIterator]`.
fn async_iterator_key(key: &ast::PropName) -> bool {
    let ast::PropName::Computed(computed) = key else {
        return false;
    };
    matches!(
        &*computed.expr,
        ast::Expr::Member(ast::MemberExpr { obj, prop: ast::MemberProp::Ident(property), .. })
            if matches!(&**obj, ast::Expr::Ident(object) if object.sym == "Symbol")
                && property.sym == "asyncIterator"
    )
}

/// An instance method's registered name: `member_name`, or the async
/// iterator method under `net_request` profiles. Engine preludes of those
/// profiles may also declare `toString` (`URL`, `URLSearchParams`).
fn method_name(
    key: &ast::PropName,
    profile: varta_common::language::LanguageProfile,
    prelude: bool,
) -> Result<String> {
    if profile.net_request() && async_iterator_key(key) {
        return Ok(ASYNC_ITERATOR_METHOD.to_owned());
    }
    if profile.net_request()
        && prelude
        && matches!(key, ast::PropName::Ident(id) if id.sym == error_classes::TO_STRING)
    {
        return Ok(error_classes::TO_STRING.to_owned());
    }
    member_name(key)
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
    if types.classes.list.len() >= types.profile.limits().record_shapes {
        return Err(source.locate(invalid("class count exceeds limit"), declaration.ident.span));
    }
    let params = match &declaration.class.type_params {
        Some(params) if types.profile.class_generics() => {
            if matches!(&declaration.class.super_class, Some(parent)
                if matches!(&**parent, ast::Expr::Ident(id)
                    if varta_common::value::ErrorClass::from_name(id.sym.as_ref()).is_some()))
            {
                return Err(source.locate(
                    unsupported("generic Error subclass"),
                    declaration.ident.span,
                ));
            }
            Some(Rc::new((**params).clone()))
        }
        _ => None,
    };
    let id = types.classes.list.len() as u32;
    types.classes.list.push(Class {
        name: name.clone(),
        module,
        source: source.clone(),
        span: declaration.ident.span,
        // `#x` members and references become `private` members named `#x` (v37).
        declaration: {
            let class = if types.profile.class_generics() {
                private_names::rewrite(&declaration.class, source)?
            } else {
                declaration.class.clone()
            };
            // `constructor(private readonly x: T)` (v38).
            if types.profile.structural_subtyping() {
                parameter_properties::rewrite(class, source)?
            } else {
                class
            }
        },
        parent: None,
        shape: 0,
        fields: Vec::new(),
        methods: BTreeMap::new(),
        statics: BTreeMap::new(),
        generic_methods: BTreeMap::new(),
        generic_statics: BTreeMap::new(),
        init: None,
        initializers: Vec::new(),
        state: State::Pending,
        prelude: crate::modules::engine::is_prelude_source(source),
        params,
        template: None,
        frame: generics::Frame::new(),
        unallocated: Vec::new(),
        opaque: crate::modules::engine::is_builtin_source(source),
        merged: None,
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
mod generic_classes;
mod generic_methods;
mod implements;
mod layout;
mod members;
mod merged_interfaces;
mod parameter_properties;
mod private_names;
pub(super) use dispatch::{bodies, dispatch_bodies, dispatchers, late_dispatch_bodies};
pub(super) use error_classes::{
    MESSAGE, NAME, join_error_classes, new_target_name, reaches_instance, reaches_json_form,
    reaches_prelude, root as error_family_root,
};
pub(super) use generic_classes::{UNALLOCATED, instantiate_class, member_body, next_member};
pub(super) use generic_methods::{
    generic_dispatch_bodies, generic_method_body, next_generic_method,
};
pub(super) use layout::layout_module;
pub(super) use private_names::{
    PREFIX as PRIVATE_PREFIX, hide_prelude_state, is_private_name, reaches_private,
};
