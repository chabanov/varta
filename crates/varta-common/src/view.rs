//! L0 view protocol, **unstable draft v0**: data only.
//!
//! These tables give the browser applier, the headless renderer and a future
//! native renderer one source for every numeric code of the accepted L0 v1
//! subset (development plan, "View layer"). Nothing here is a registered host
//! operation: no compiler emits `view_commit`, no runtime links it and no
//! profile admits it. Codes may change until the protocol is frozen by the
//! freeze criterion; a frozen protocol gets its own version and host ABI.
use crate::HostScalar;

/// Draft protocol version carried in every batch and event record header.
pub const DRAFT_VERSION: u16 = 0;

/// The planned patch import: `view_commit(ptr, len) -> status`. Not registered.
pub const COMMIT_IMPORT: &str = "view_commit";
pub const COMMIT_PARAMS: [HostScalar; 2] = [HostScalar::I32, HostScalar::I32];
pub const COMMIT_RESULTS: [HostScalar; 1] = [HostScalar::I32];
/// The planned guest export that receives one event record written at
/// `abi::RESULT_BASE`: `(len: i32) -> ()`.
pub const EVENT_EXPORT: &str = "varta.view_abi.event";
pub const EVENT_EXPORT_PARAMS: [HostScalar; 1] = [HostScalar::I32];

/// Node id of the root container that the host creates; guest ids are nonzero.
pub const ROOT_NODE: u32 = 1;
/// "No node" in `insert`'s `before` operand (append).
pub const NO_NODE: u32 = 0;

/// Batch header: `version: u16`, `flags: u16`, `op_count: u32`.
pub const BATCH_HEADER_BYTES: usize = 8;
/// Operation header: `opcode: u8` and three reserved zero bytes.
pub const OPERATION_HEADER_BYTES: usize = 4;
/// Strings are a `u32` byte length followed by UTF-16LE code units, padded to this.
pub const STRING_ALIGNMENT: usize = 4;

/// Placeholder budgets, to be replaced by measured named limits before freezing.
pub const DRAFT_MAX_BATCH_BYTES: usize = 256 * 1024;
pub const DRAFT_MAX_BATCH_OPERATIONS: usize = 16 * 1024;
pub const DRAFT_MAX_LIVE_NODES: usize = 64 * 1024;
pub const DRAFT_MAX_LIVE_LISTENERS: usize = 8 * 1024;
pub const DRAFT_MAX_VALUE_BYTES: usize = 64 * 1024;
pub const DRAFT_MAX_STYLESHEET_BYTES: usize = 256 * 1024;
pub const DRAFT_MAX_PENDING_EVENTS: usize = 64;

/// Declares a `u16`/`u8`-coded enum with its wire name and an `ALL` table in code order.
macro_rules! coded {
    ($(#[$meta:meta])* $name:ident: $repr:ty { $($variant:ident = $code:literal => $text:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[repr($repr)]
        pub enum $name {
            $($variant = $code,)+
        }
        impl $name {
            /// Every variant, in ascending code order.
            pub const ALL: &[Self] = &[$(Self::$variant,)+];
            #[must_use]
            pub fn code(self) -> $repr {
                self as $repr
            }
            /// The HTML/CSS/DOM spelling, or the protocol operation name.
            #[must_use]
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }
            #[must_use]
            pub fn from_code(code: $repr) -> Option<Self> {
                Self::ALL.iter().copied().find(|value| value.code() == code)
            }
        }
    };
}

coded! {
    /// Patch operations; operands follow [`Opcode::operands`].
    Opcode: u8 {
        CreateElement = 1 => "create_element",
        CreateText = 2 => "create_text",
        SetText = 3 => "set_text",
        SetAttribute = 4 => "set_attribute",
        RemoveAttribute = 5 => "remove_attribute",
        SetClass = 6 => "set_class",
        SetStyle = 7 => "set_style",
        RemoveStyle = 8 => "remove_style",
        Insert = 9 => "insert",
        Remove = 10 => "remove",
        Listen = 11 => "listen",
        Unlisten = 12 => "unlisten",
        SetValue = 13 => "set_value",
        SetChecked = 14 => "set_checked",
        Command = 15 => "command",
        Stylesheet = 16 => "stylesheet",
        SetTitle = 17 => "set_title",
        CopyText = 18 => "copy_text",
    }
}

coded! {
    /// Elements of the L0 v1 subset (HTML namespace only).
    Tag: u16 {
        Div = 1 => "div",
        Span = 2 => "span",
        P = 3 => "p",
        H1 = 4 => "h1",
        H2 = 5 => "h2",
        H3 = 6 => "h3",
        Ul = 7 => "ul",
        Ol = 8 => "ol",
        Li = 9 => "li",
        Header = 10 => "header",
        Nav = 11 => "nav",
        Main = 12 => "main",
        Section = 13 => "section",
        Footer = 14 => "footer",
        Strong = 15 => "strong",
        Em = 16 => "em",
        Code = 17 => "code",
        Pre = 18 => "pre",
        Br = 19 => "br",
        Button = 20 => "button",
        A = 21 => "a",
        Form = 22 => "form",
        Label = 23 => "label",
        Input = 24 => "input",
        Textarea = 25 => "textarea",
        Select = 26 => "select",
        Option = 27 => "option",
        Dialog = 28 => "dialog",
        Progress = 29 => "progress",
        Img = 30 => "img",
    }
}

coded! {
    /// Attributes of the L0 v1 subset. `class`, `style`, the live input value and
    /// `checked` have their own operations.
    Attribute: u16 {
        Id = 1 => "id",
        Type = 2 => "type",
        Name = 3 => "name",
        Value = 4 => "value",
        Placeholder = 5 => "placeholder",
        Disabled = 6 => "disabled",
        Readonly = 7 => "readonly",
        Maxlength = 8 => "maxlength",
        Autofocus = 9 => "autofocus",
        Selected = 10 => "selected",
        For = 11 => "for",
        Href = 12 => "href",
        Src = 13 => "src",
        Alt = 14 => "alt",
        Tabindex = 15 => "tabindex",
        Max = 16 => "max",
        Role = 17 => "role",
        AriaLabel = 18 => "aria-label",
        AriaLabelledby = 19 => "aria-labelledby",
        AriaDescribedby = 20 => "aria-describedby",
        AriaLive = 21 => "aria-live",
        AriaExpanded = 22 => "aria-expanded",
        AriaSelected = 23 => "aria-selected",
        AriaCurrent = 24 => "aria-current",
        AriaBusy = 25 => "aria-busy",
        AriaInvalid = 26 => "aria-invalid",
        AriaHidden = 27 => "aria-hidden",
        Rows = 28 => "rows",
    }
}

coded! {
    /// CSS properties of the L0 v1 subset; values follow the subset grammar.
    CssProperty: u16 {
        Display = 1 => "display",
        Width = 2 => "width",
        Height = 3 => "height",
        MinWidth = 4 => "min-width",
        MinHeight = 5 => "min-height",
        MaxWidth = 6 => "max-width",
        MaxHeight = 7 => "max-height",
        Margin = 8 => "margin",
        MarginTop = 9 => "margin-top",
        MarginRight = 10 => "margin-right",
        MarginBottom = 11 => "margin-bottom",
        MarginLeft = 12 => "margin-left",
        Padding = 13 => "padding",
        PaddingTop = 14 => "padding-top",
        PaddingRight = 15 => "padding-right",
        PaddingBottom = 16 => "padding-bottom",
        PaddingLeft = 17 => "padding-left",
        Gap = 18 => "gap",
        RowGap = 19 => "row-gap",
        ColumnGap = 20 => "column-gap",
        FlexDirection = 21 => "flex-direction",
        FlexWrap = 22 => "flex-wrap",
        JustifyContent = 23 => "justify-content",
        AlignItems = 24 => "align-items",
        AlignSelf = 25 => "align-self",
        FlexGrow = 26 => "flex-grow",
        FlexShrink = 27 => "flex-shrink",
        FlexBasis = 28 => "flex-basis",
        GridTemplateColumns = 29 => "grid-template-columns",
        GridTemplateRows = 30 => "grid-template-rows",
        GridColumn = 31 => "grid-column",
        GridRow = 32 => "grid-row",
        Position = 33 => "position",
        Top = 34 => "top",
        Right = 35 => "right",
        Bottom = 36 => "bottom",
        Left = 37 => "left",
        ZIndex = 38 => "z-index",
        Overflow = 39 => "overflow",
        Border = 40 => "border",
        BorderWidth = 41 => "border-width",
        BorderStyle = 42 => "border-style",
        BorderColor = 43 => "border-color",
        BorderRadius = 44 => "border-radius",
        BackgroundColor = 45 => "background-color",
        Color = 46 => "color",
        Opacity = 47 => "opacity",
        Outline = 48 => "outline",
        OutlineWidth = 49 => "outline-width",
        OutlineStyle = 50 => "outline-style",
        OutlineColor = 51 => "outline-color",
        OutlineOffset = 52 => "outline-offset",
        FontFamily = 53 => "font-family",
        FontSize = 54 => "font-size",
        FontWeight = 55 => "font-weight",
        FontStyle = 56 => "font-style",
        LineHeight = 57 => "line-height",
        TextAlign = 58 => "text-align",
        TextDecoration = 59 => "text-decoration",
        WhiteSpace = 60 => "white-space",
        TextOverflow = 61 => "text-overflow",
        OverflowWrap = 62 => "overflow-wrap",
        Cursor = 63 => "cursor",
        Transform = 64 => "transform",
        TransitionProperty = 65 => "transition-property",
        TransitionDuration = 66 => "transition-duration",
        TransitionTimingFunction = 67 => "transition-timing-function",
        TransitionDelay = 68 => "transition-delay",
        FieldSizing = 69 => "field-sizing",
    }
}

coded! {
    /// Event kinds a `listen` may name and an event record may carry.
    EventKind: u16 {
        Click = 1 => "click",
        Input = 2 => "input",
        Change = 3 => "change",
        Submit = 4 => "submit",
        Keydown = 5 => "keydown",
        Focusin = 6 => "focusin",
        Focusout = 7 => "focusout",
        Scroll = 8 => "scroll",
        Cancel = 9 => "cancel",
        Close = 10 => "close",
    }
}

coded! {
    /// Imperative commands of the `command` operation.
    Command: u16 {
        Focus = 1 => "focus",
        Blur = 2 => "blur",
        Select = 3 => "select",
        ScrollIntoView = 4 => "scroll_into_view",
        ScrollTo = 5 => "scroll_to",
        ShowModal = 6 => "show_modal",
        Close = 7 => "close",
    }
}

coded! {
    /// Keys a `keydown` listener filter may name; the name is the DOM
    /// `KeyboardEvent.key` value (`Space` is the key `" "`).
    Key: u16 {
        Enter = 1 => "Enter",
        Escape = 2 => "Escape",
        Tab = 3 => "Tab",
        Space = 4 => "Space",
        Backspace = 5 => "Backspace",
        Delete = 6 => "Delete",
        ArrowUp = 7 => "ArrowUp",
        ArrowDown = 8 => "ArrowDown",
        ArrowLeft = 9 => "ArrowLeft",
        ArrowRight = 10 => "ArrowRight",
        Home = 11 => "Home",
        End = 12 => "End",
        PageUp = 13 => "PageUp",
        PageDown = 14 => "PageDown",
    }
}
impl Key {
    /// The DOM `KeyboardEvent.key` value the filter compares with.
    #[must_use]
    pub fn dom_key(self) -> &'static str {
        match self {
            Self::Space => " ",
            other => other.name(),
        }
    }
}
/// Filter key code that matches every key (a filter on modifiers only, or none).
pub const ANY_KEY: u16 = 0;
/// Modifier bits a filter may require or forbid: the shift, ctrl, alt and meta
/// bits of [`EventFlag`], with the same values.
pub const MODIFIER_MASK: u32 = EventFlag::Shift as u32
    | EventFlag::Ctrl as u32
    | EventFlag::Alt as u32
    | EventFlag::Meta as u32;

/// One operand of a patch operation, in wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    /// `u32` node id.
    Node,
    /// `u32` parent node id.
    Parent,
    /// `u32` node id to insert before, or [`NO_NODE`] to append.
    Before,
    /// `u16` [`Tag`] code, padded to 4 bytes.
    Tag,
    /// `u16` [`Attribute`] code, padded to 4 bytes.
    Attribute,
    /// `u16` [`CssProperty`] code, padded to 4 bytes.
    Property,
    /// `u16` [`EventKind`] code, padded to 4 bytes.
    Event,
    /// `u16` [`Command`] code, padded to 4 bytes.
    Command,
    /// `u32` guest-chosen nonzero handler id.
    Handler,
    /// `u32` [`ListenOption`] bit set.
    Options,
    /// `u32` field edit counter the guest last saw.
    BaseSeq,
    /// `u32` 0 or 1.
    Checked,
    /// `u32` stylesheet id.
    Sheet,
    /// `f64` command argument (`scroll_to` top; otherwise 0).
    Argument,
    /// UTF-16LE string.
    Text,
    /// `u16` [`Key`] code or [`ANY_KEY`], padded to 4 bytes.
    Key,
    /// `u32` modifier bits (within [`MODIFIER_MASK`]) that must all be held.
    RequiredModifiers,
    /// `u32` modifier bits (within [`MODIFIER_MASK`]) that must all be released.
    ForbiddenModifiers,
}
impl Operand {
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::Node | Self::Parent | Self::Before | Self::Handler | Self::Sheet => "u32 id",
            Self::Tag
            | Self::Attribute
            | Self::Property
            | Self::Event
            | Self::Command
            | Self::Key => "u16 code + 2 pad",
            Self::Options
            | Self::BaseSeq
            | Self::Checked
            | Self::RequiredModifiers
            | Self::ForbiddenModifiers => "u32",
            Self::Argument => "f64",
            Self::Text => "string",
        }
    }
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Parent => "parent",
            Self::Before => "before",
            Self::Tag => "tag",
            Self::Attribute => "attribute",
            Self::Property => "property",
            Self::Event => "event",
            Self::Command => "command",
            Self::Handler => "handler",
            Self::Options => "options",
            Self::BaseSeq => "base_seq",
            Self::Checked => "checked",
            Self::Sheet => "sheet",
            Self::Argument => "argument",
            Self::Text => "text",
            Self::Key => "key",
            Self::RequiredModifiers => "required_modifiers",
            Self::ForbiddenModifiers => "forbidden_modifiers",
        }
    }
}
impl Opcode {
    /// Operands in wire order after the operation header.
    #[must_use]
    pub fn operands(self) -> &'static [Operand] {
        use Operand as O;
        match self {
            Self::CreateElement => &[O::Node, O::Tag],
            Self::CreateText | Self::SetText | Self::SetClass => &[O::Node, O::Text],
            Self::SetAttribute => &[O::Node, O::Attribute, O::Text],
            Self::RemoveAttribute => &[O::Node, O::Attribute],
            Self::SetStyle => &[O::Node, O::Property, O::Text],
            Self::RemoveStyle => &[O::Node, O::Property],
            Self::Insert => &[O::Parent, O::Node, O::Before],
            Self::Remove => &[O::Node],
            Self::Listen => &[
                O::Node,
                O::Event,
                O::Handler,
                O::Options,
                O::Key,
                O::RequiredModifiers,
                O::ForbiddenModifiers,
            ],
            Self::Unlisten => &[O::Node, O::Event, O::Handler],
            Self::SetValue => &[O::Node, O::Text, O::BaseSeq],
            Self::SetChecked => &[O::Node, O::Checked, O::BaseSeq],
            Self::Command => &[O::Node, O::Command, O::Argument],
            Self::Stylesheet => &[O::Sheet, O::Text],
            Self::SetTitle | Self::CopyText => &[O::Text],
        }
    }
}

/// Bits of the `listen` options operand, executed synchronously by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ListenOption {
    PreventDefault = 1,
    StopPropagation = 1 << 1,
    /// Key filter: never match while an IME composition is active.
    NotComposing = 1 << 2,
}
impl ListenOption {
    pub const ALL: &[Self] = &[
        Self::PreventDefault,
        Self::StopPropagation,
        Self::NotComposing,
    ];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PreventDefault => "preventDefault",
            Self::StopPropagation => "stopPropagation",
            Self::NotComposing => "notComposing",
        }
    }
}

/// Bits of an event record's `flags` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum EventFlag {
    Shift = 1,
    Ctrl = 1 << 1,
    Alt = 1 << 2,
    Meta = 1 << 3,
    Composing = 1 << 4,
    Checked = 1 << 5,
}
impl EventFlag {
    pub const ALL: &[Self] = &[
        Self::Shift,
        Self::Ctrl,
        Self::Alt,
        Self::Meta,
        Self::Composing,
        Self::Checked,
    ];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Shift => "shiftKey",
            Self::Ctrl => "ctrlKey",
            Self::Alt => "altKey",
            Self::Meta => "metaKey",
            Self::Composing => "isComposing",
            Self::Checked => "checked",
        }
    }
}

/// Wire type of a fixed event record field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    U16,
    U32,
    F64,
}
impl FieldType {
    #[must_use]
    pub fn bytes(self) -> usize {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
            Self::F64 => 8,
        }
    }
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::F64 => "f64",
        }
    }
}

/// A fixed-offset field of the event record header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventField {
    pub name: &'static str,
    pub offset: usize,
    pub ty: FieldType,
}

/// The event record header; the `key` and `value` strings follow it, in that order.
pub const EVENT_FIELDS: &[EventField] = &[
    EventField {
        name: "version",
        offset: 0,
        ty: FieldType::U16,
    },
    EventField {
        name: "kind",
        offset: 2,
        ty: FieldType::U16,
    },
    EventField {
        name: "handler",
        offset: 4,
        ty: FieldType::U32,
    },
    EventField {
        name: "target",
        offset: 8,
        ty: FieldType::U32,
    },
    EventField {
        name: "seq",
        offset: 12,
        ty: FieldType::U32,
    },
    EventField {
        name: "flags",
        offset: 16,
        ty: FieldType::U32,
    },
    EventField {
        name: "reserved",
        offset: 20,
        ty: FieldType::U32,
    },
    EventField {
        name: "scroll_top",
        offset: 24,
        ty: FieldType::F64,
    },
    EventField {
        name: "scroll_height",
        offset: 32,
        ty: FieldType::F64,
    },
    EventField {
        name: "client_height",
        offset: 40,
        ty: FieldType::F64,
    },
];
/// Header bytes before the `key` string.
pub const EVENT_HEADER_BYTES: usize = 48;
/// Strings following the event header, in wire order.
pub const EVENT_STRINGS: &[&str] = &["key", "value"];

/// The elements an attribute or command applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tags {
    /// Every element of the subset.
    All,
    /// Elements that take focus: `button`, `input`, `select`, `textarea`, `a`
    /// and any element with a `tabindex`.
    Focusable,
    Only(&'static [Tag]),
}

/// `type` values of `input` in the subset.
pub const INPUT_TYPES: &[&str] = &["text", "password", "number", "search", "checkbox", "radio"];
/// `type` values of `button`; the default is `submit`, which matters only inside a `form`.
pub const BUTTON_TYPES: &[&str] = &["button", "submit"];
/// Values of the `role` attribute in the subset.
pub const ROLES: &[&str] = &[
    "table",
    "row",
    "cell",
    "columnheader",
    "dialog",
    "alert",
    "status",
    "navigation",
    "list",
    "listitem",
    "tab",
    "tablist",
    "tabpanel",
];

/// The value grammar of an attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeValue {
    /// `[A-Za-z][A-Za-z0-9_-]*`, at most 64 characters, unique in the tree.
    Ident,
    /// Space-separated `Ident` references.
    IdentList,
    /// Any well-formed text.
    Text,
    /// Present means true; the value must be empty. `remove_attribute` means false.
    Boolean,
    /// `0` or a positive decimal integer without sign or leading zeros.
    Count,
    /// A finite nonnegative decimal number.
    Number,
    /// A positive decimal integer without sign or leading zeros.
    PositiveCount,
    /// One of the listed keywords, exactly.
    Keywords(&'static [&'static str]),
    /// `input`: [`INPUT_TYPES`]; `button`: [`BUTTON_TYPES`].
    ElementType,
    /// Internal route: `#fragment`, or `/path` (not `//`) with optional `?query`
    /// and `#fragment`; or an absolute `https://`/`http://` URL that matches a
    /// [`OPEN_URL_CAPABILITY`] grant (an external link).
    Href,
    /// A path of a bundled artifact resource, resolved against the asset base.
    Resource,
}
impl AttributeValue {
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Ident => "ident: `[A-Za-z][A-Za-z0-9_-]*`, at most 64 characters".into(),
            Self::IdentList => "space-separated ident list".into(),
            Self::Text => "text".into(),
            Self::Boolean => "boolean: present with an empty value; remove to make false".into(),
            Self::Count => "count: `0` or a positive integer without sign or leading zeros".into(),
            Self::Number => "finite nonnegative number".into(),
            Self::PositiveCount => "positive integer without sign or leading zeros".into(),
            Self::Keywords(words) => words
                .iter()
                .map(|word| format!("`{word}`"))
                .collect::<Vec<_>>()
                .join(" | "),
            Self::ElementType => format!(
                "input: {}; button: {} (default `submit`)",
                INPUT_TYPES
                    .iter()
                    .map(|word| format!("`{word}`"))
                    .collect::<Vec<_>>()
                    .join(" | "),
                BUTTON_TYPES
                    .iter()
                    .map(|word| format!("`{word}`"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            Self::Href => format!(
                "internal route: `#fragment` or `/path[?query][#fragment]`, never `//`; or an absolute `https://`/`http://` URL matching a `{OPEN_URL_CAPABILITY}` grant"
            ),
            Self::Resource => "bundled resource path".into(),
        }
    }
}

const BOOLEAN_STATE: &[&str] = &["true", "false"];
const LIVE_REGIONS: &[&str] = &["off", "polite", "assertive"];
const CURRENT: &[&str] = &["page", "step", "location", "date", "time", "true", "false"];
const TABINDEX: &[&str] = &["0", "-1"];

impl Attribute {
    /// Elements that may carry the attribute; any other element invalidates the batch.
    #[must_use]
    pub fn tags(self) -> Tags {
        use Tag as T;
        match self {
            Self::Id
            | Self::Tabindex
            | Self::Role
            | Self::AriaLabel
            | Self::AriaLabelledby
            | Self::AriaDescribedby
            | Self::AriaLive
            | Self::AriaExpanded
            | Self::AriaSelected
            | Self::AriaCurrent
            | Self::AriaBusy
            | Self::AriaInvalid
            | Self::AriaHidden => Tags::All,
            Self::Type => Tags::Only(&[T::Input, T::Button]),
            Self::Name => Tags::Only(&[T::Input, T::Select, T::Textarea]),
            Self::Value => Tags::Only(&[T::Option, T::Progress]),
            Self::Placeholder | Self::Readonly | Self::Maxlength => {
                Tags::Only(&[T::Input, T::Textarea])
            }
            Self::Disabled => Tags::Only(&[T::Button, T::Input, T::Select, T::Textarea, T::Option]),
            Self::Autofocus => Tags::Only(&[T::Button, T::Input, T::Select, T::Textarea]),
            Self::Selected => Tags::Only(&[T::Option]),
            Self::For => Tags::Only(&[T::Label]),
            Self::Href => Tags::Only(&[T::A]),
            Self::Src | Self::Alt => Tags::Only(&[T::Img]),
            Self::Max => Tags::Only(&[T::Progress]),
            Self::Rows => Tags::Only(&[T::Textarea]),
        }
    }
    #[must_use]
    pub fn value(self) -> AttributeValue {
        use AttributeValue as V;
        match self {
            Self::Id | Self::For => V::Ident,
            Self::AriaLabelledby | Self::AriaDescribedby => V::IdentList,
            Self::Type => V::ElementType,
            Self::Name | Self::Placeholder | Self::Alt | Self::AriaLabel | Self::Value => V::Text,
            Self::Disabled | Self::Readonly | Self::Autofocus | Self::Selected => V::Boolean,
            Self::Maxlength => V::Count,
            Self::Max => V::Number,
            Self::Rows => V::PositiveCount,
            Self::Href => V::Href,
            Self::Src => V::Resource,
            Self::Tabindex => V::Keywords(TABINDEX),
            Self::Role => V::Keywords(ROLES),
            Self::AriaLive => V::Keywords(LIVE_REGIONS),
            Self::AriaCurrent => V::Keywords(CURRENT),
            Self::AriaExpanded
            | Self::AriaSelected
            | Self::AriaBusy
            | Self::AriaInvalid
            | Self::AriaHidden => V::Keywords(BOOLEAN_STATE),
        }
    }
}

impl Command {
    #[must_use]
    pub fn tags(self) -> Tags {
        match self {
            Self::Focus | Self::Blur => Tags::Focusable,
            Self::Select => Tags::Only(&[Tag::Input, Tag::Textarea]),
            Self::ScrollIntoView | Self::ScrollTo => Tags::All,
            Self::ShowModal | Self::Close => Tags::Only(&[Tag::Dialog]),
        }
    }
    /// Only `scroll_to` takes its argument (a finite nonnegative top offset in
    /// CSS pixels); every other command requires the argument 0.
    #[must_use]
    pub fn takes_argument(self) -> bool {
        self == Self::ScrollTo
    }
}

impl EventKind {
    /// Pending records of a coalescible kind are replaced by a newer record for
    /// the same listener; other kinds are never merged.
    #[must_use]
    pub fn coalescible(self) -> bool {
        matches!(self, Self::Input | Self::Scroll)
    }
    /// Whether the event increments the target field's edit counter.
    #[must_use]
    pub fn edits(self) -> bool {
        self == Self::Input
    }
}

/// Primitive CSS value types used by [`CssProperty::grammar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssValue {
    Number,
    Integer,
    Length,
    Percentage,
    LengthPercentage,
    Color,
    Time,
    Track,
    Var,
}
impl CssValue {
    pub const ALL: &[Self] = &[
        Self::Number,
        Self::Integer,
        Self::Length,
        Self::Percentage,
        Self::LengthPercentage,
        Self::Color,
        Self::Time,
        Self::Track,
        Self::Var,
    ];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "<number>",
            Self::Integer => "<integer>",
            Self::Length => "<length>",
            Self::Percentage => "<percentage>",
            Self::LengthPercentage => "<length-percentage>",
            Self::Color => "<color>",
            Self::Time => "<time>",
            Self::Track => "<track>",
            Self::Var => "<var>",
        }
    }
    #[must_use]
    pub fn definition(self) -> &'static str {
        match self {
            Self::Number => "`-`? digits [`.` digits], or `.` digits; no exponent",
            Self::Integer => "`-`? digits",
            Self::Length => "<number>`px` | <number>`rem` | `0`",
            Self::Percentage => "<number>`%`",
            Self::LengthPercentage => "<length> | <percentage>",
            Self::Color => {
                "`#rgb` | `#rrggbb` | `#rrggbbaa` | `rgb(` <integer>`,` <integer>`,` <integer> `)` | `rgba(` <integer>`,` <integer>`,` <integer>`,` <number> `)` | `transparent` | `currentcolor`"
            }
            Self::Time => "<number>`ms` | <number>`s`",
            Self::Track => {
                "<length-percentage> | <number>`fr` | `auto` | `minmax(` <track>`,` <track> `)` | `repeat(` <integer>`,` <track>+ `)`"
            }
            Self::Var => {
                "`var(--`name`)` or `var(--`name`,` fallback`)`, allowed wherever a whole component value is expected"
            }
        }
    }
}

impl CssProperty {
    /// Value definition in CSS value-definition syntax over [`CssValue`] types.
    /// `a | b` is a choice, `a{1,4}` repetition, `a || b` any order, `a#` a comma list.
    #[must_use]
    pub fn grammar(self) -> &'static str {
        match self {
            Self::Display => "block | flex | grid | none | inline",
            Self::Width | Self::Height | Self::MinWidth | Self::MinHeight => {
                "auto | <length-percentage>"
            }
            Self::MaxWidth | Self::MaxHeight => "none | <length-percentage>",
            Self::Margin => "[auto | <length-percentage>]{1,4}",
            Self::MarginTop | Self::MarginRight | Self::MarginBottom | Self::MarginLeft => {
                "auto | <length-percentage>"
            }
            Self::Padding => "<length-percentage>{1,4} (nonnegative)",
            Self::PaddingTop | Self::PaddingRight | Self::PaddingBottom | Self::PaddingLeft => {
                "<length-percentage> (nonnegative)"
            }
            Self::Gap => "<length-percentage>{1,2}",
            Self::RowGap | Self::ColumnGap => "<length-percentage>",
            Self::FlexDirection => "row | row-reverse | column | column-reverse",
            Self::FlexWrap => "nowrap | wrap | wrap-reverse",
            Self::JustifyContent => {
                "start | end | flex-start | flex-end | center | space-between | space-around | space-evenly"
            }
            Self::AlignItems => "stretch | start | end | flex-start | flex-end | center | baseline",
            Self::AlignSelf => {
                "auto | stretch | start | end | flex-start | flex-end | center | baseline"
            }
            Self::FlexGrow | Self::FlexShrink => "<number> (nonnegative)",
            Self::FlexBasis => "auto | <length-percentage>",
            Self::GridTemplateColumns | Self::GridTemplateRows => "none | <track>+",
            Self::GridColumn | Self::GridRow => {
                "<line> [/ <line>]? where <line> = auto | <integer> | span <integer>"
            }
            Self::Position => "static | relative | absolute | fixed",
            Self::Top | Self::Right | Self::Bottom | Self::Left => "auto | <length-percentage>",
            Self::ZIndex => "auto | <integer>",
            Self::Overflow => "visible | hidden | auto",
            Self::Border | Self::Outline => "none | [<length> || solid || <color>]",
            Self::BorderWidth | Self::OutlineWidth | Self::OutlineOffset => "<length>",
            Self::BorderStyle | Self::OutlineStyle => "none | solid",
            Self::BorderColor | Self::OutlineColor | Self::BackgroundColor | Self::Color => {
                "<color>"
            }
            Self::BorderRadius => "<length-percentage>{1,4}",
            Self::Opacity => "<number> (0 to 1)",
            Self::FontFamily => {
                "[<string> | <ident> | sans-serif | serif | monospace | system-ui]# (artifact or generic families)"
            }
            Self::FontSize => "<length-percentage>",
            Self::FontWeight => "normal | bold | 400 | 500 | 600 | 700",
            Self::FontStyle => "normal | italic",
            Self::LineHeight => "normal | <number> | <length-percentage>",
            Self::TextAlign => "left | center | right",
            Self::TextDecoration => "none | underline",
            Self::WhiteSpace => "normal | nowrap | pre-wrap",
            Self::TextOverflow => "clip | ellipsis",
            Self::OverflowWrap => "normal | anywhere",
            Self::Cursor => "default | pointer | text | not-allowed",
            Self::Transform => {
                "none | [translate(<length-percentage>[, <length-percentage>]) | translateX(<length-percentage>) | translateY(<length-percentage>) | scale(<number>[, <number>])]+"
            }
            Self::TransitionProperty => {
                "none | all | [opacity | color | background-color | transform]#"
            }
            Self::TransitionDuration | Self::TransitionDelay => "<time>#",
            Self::TransitionTimingFunction => "[linear | ease | ease-in | ease-out | ease-in-out]#",
            Self::FieldSizing => "fixed | content (textarea only)",
        }
    }
}

/// Pseudo-classes a stylesheet selector may use.
pub const PSEUDO_CLASSES: &[&str] = &[
    ":hover",
    ":active",
    ":focus-visible",
    ":disabled",
    ":checked",
];
/// Media features of `@media` conditions.
pub const MEDIA_FEATURES: &[&str] = &["prefers-color-scheme", "min-width", "max-width"];
/// Text that invalidates a stylesheet or a `set_style` value wherever it occurs
/// (ASCII case-insensitive), besides any function the grammar does not name.
pub const FORBIDDEN_CSS: &[&str] = &[
    "\\",
    "<",
    ">",
    "/*",
    "@import",
    "!important",
    "url(",
    "image-set(",
    "expression(",
];
/// Characters that are structural in stylesheets and invalid in `set_style` values.
pub const FORBIDDEN_STYLE_VALUE: &[&str] = &[";", "{", "}"];

/// The capability an external link needs (draft v0, not registered).
pub const OPEN_URL_CAPABILITY: &str = "ui.open_url";
/// The capability `copy_text` needs (draft v0, not registered).
pub const CLIPBOARD_WRITE_CAPABILITY: &str = "clipboard.write";

/// The grant shape of a draft view capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftGrant {
    /// `"enabled": true | false`.
    Boolean,
    /// `"patterns": [...]` with the URL pattern rules, `https://` admitted.
    Urls,
}

/// View capabilities of draft v0. None is in the capability registry: the
/// runtime and every manifest validator reject these ids until a profile
/// registers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftCapability {
    OpenUrl,
    ClipboardWrite,
}
impl DraftCapability {
    pub const ALL: &[Self] = &[Self::OpenUrl, Self::ClipboardWrite];
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenUrl => OPEN_URL_CAPABILITY,
            Self::ClipboardWrite => CLIPBOARD_WRITE_CAPABILITY,
        }
    }
    #[must_use]
    pub fn grant(self) -> DraftGrant {
        match self {
            Self::OpenUrl => DraftGrant::Urls,
            Self::ClipboardWrite => DraftGrant::Boolean,
        }
    }
    /// What the capability authorizes, always only during a user gesture.
    #[must_use]
    pub fn use_site(self) -> &'static str {
        match self {
            Self::OpenUrl => "activating an `a` whose `href` is an external URL",
            Self::ClipboardWrite => "a `copy_text` operation in the batch of a user-gesture turn",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn contiguous<T: Copy>(all: &[T], code: impl Fn(T) -> u32) {
        for (index, &value) in all.iter().enumerate() {
            assert_eq!(
                code(value) as usize,
                index + 1,
                "codes start at 1 without gaps"
            );
        }
    }
    fn unique_names(names: impl IntoIterator<Item = &'static str>) {
        let mut seen = BTreeSet::new();
        for name in names {
            assert!(seen.insert(name), "duplicate name {name}");
        }
    }

    #[test]
    fn codes_are_contiguous_unique_and_round_trip() {
        contiguous(Opcode::ALL, |v| u32::from(v.code()));
        contiguous(Tag::ALL, |v| u32::from(v.code()));
        contiguous(Attribute::ALL, |v| u32::from(v.code()));
        contiguous(CssProperty::ALL, |v| u32::from(v.code()));
        contiguous(EventKind::ALL, |v| u32::from(v.code()));
        contiguous(Command::ALL, |v| u32::from(v.code()));
        unique_names(Opcode::ALL.iter().map(|v| v.name()));
        unique_names(Tag::ALL.iter().map(|v| v.name()));
        unique_names(Attribute::ALL.iter().map(|v| v.name()));
        unique_names(CssProperty::ALL.iter().map(|v| v.name()));
        unique_names(EventKind::ALL.iter().map(|v| v.name()));
        unique_names(Command::ALL.iter().map(|v| v.name()));
        for &tag in Tag::ALL {
            assert_eq!(Tag::from_code(tag.code()), Some(tag));
        }
        assert_eq!(Opcode::from_code(0), None);
        assert_eq!(Tag::from_code(0), None);
    }

    #[test]
    fn bit_sets_are_single_distinct_bits() {
        let mut seen = 0;
        for &option in ListenOption::ALL {
            let bit = option as u32;
            assert!(bit.is_power_of_two() && seen & bit == 0);
            seen |= bit;
        }
        let mut seen = 0;
        for &flag in EventFlag::ALL {
            let bit = flag as u32;
            assert!(bit.is_power_of_two() && seen & bit == 0);
            seen |= bit;
        }
    }

    #[test]
    fn event_header_is_packed_and_aligned() {
        let mut end = 0;
        for field in EVENT_FIELDS {
            assert_eq!(
                field.offset, end,
                "{} follows the previous field",
                field.name
            );
            assert_eq!(
                field.offset % field.ty.bytes(),
                0,
                "{} is aligned",
                field.name
            );
            end = field.offset + field.ty.bytes();
        }
        assert_eq!(end, EVENT_HEADER_BYTES);
        assert_eq!(EVENT_HEADER_BYTES % STRING_ALIGNMENT, 0);
    }

    #[test]
    fn operations_have_operands_and_at_most_one_string() {
        for &opcode in Opcode::ALL {
            let operands = opcode.operands();
            assert!(!operands.is_empty(), "{}", opcode.name());
            assert!(
                operands
                    .iter()
                    .filter(|operand| **operand == Operand::Text)
                    .count()
                    <= 1,
                "{} carries at most one string",
                opcode.name()
            );
        }
        assert_eq!(ROOT_NODE, NO_NODE + 1);
    }

    #[test]
    fn attribute_and_command_rules_name_subset_elements() {
        for &attribute in Attribute::ALL {
            if let Tags::Only(tags) = attribute.tags() {
                assert!(!tags.is_empty(), "{}", attribute.name());
            }
            if let AttributeValue::Keywords(words) = attribute.value() {
                assert!(!words.is_empty(), "{}", attribute.name());
                unique_names(words.iter().copied());
            }
            assert!(!attribute.value().describe().is_empty());
        }
        unique_names(ROLES.iter().copied());
        unique_names(INPUT_TYPES.iter().copied());
        assert_eq!(
            Command::ALL
                .iter()
                .filter(|command| command.takes_argument())
                .count(),
            1
        );
        assert_eq!(Command::ShowModal.tags(), Tags::Only(&[Tag::Dialog]));
    }

    #[test]
    fn css_grammars_and_value_types_are_defined() {
        for &property in CssProperty::ALL {
            assert!(!property.grammar().is_empty(), "{}", property.name());
        }
        unique_names(CssValue::ALL.iter().map(|value| value.name()));
        for &value in CssValue::ALL {
            assert!(!value.definition().is_empty());
        }
        for forbidden in FORBIDDEN_CSS.iter().chain(FORBIDDEN_STYLE_VALUE) {
            assert!(!forbidden.is_empty());
        }
    }

    #[test]
    fn key_filters_and_draft_capabilities_are_consistent() {
        contiguous(Key::ALL, |v| u32::from(v.code()));
        unique_names(Key::ALL.iter().map(|key| key.dom_key()));
        assert!(Key::from_code(ANY_KEY).is_none());
        assert_eq!(MODIFIER_MASK & (EventFlag::Composing as u32), 0);
        assert_eq!(MODIFIER_MASK.count_ones(), 4);
        let listen = Opcode::Listen.operands();
        assert!(listen.contains(&Operand::Key) && listen.contains(&Operand::ForbiddenModifiers));
        assert!(Opcode::Unlisten.operands().contains(&Operand::Handler));
        unique_names(
            DraftCapability::ALL
                .iter()
                .map(|capability| capability.id()),
        );
        for &capability in DraftCapability::ALL {
            assert!(
                crate::capability_kind(capability.id()).is_none(),
                "draft ids stay unregistered"
            );
        }
        assert_eq!(Attribute::Rows.tags(), Tags::Only(&[Tag::Textarea]));
    }

    #[test]
    fn only_input_edits_and_only_input_and_scroll_coalesce() {
        for &kind in EventKind::ALL {
            assert_eq!(kind.edits(), kind == EventKind::Input);
            assert_eq!(
                kind.coalescible(),
                matches!(kind, EventKind::Input | EventKind::Scroll)
            );
        }
    }
}
