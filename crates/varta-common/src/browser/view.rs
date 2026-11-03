//! Structured view contract. No Markdown extraction or browser-owned wire codes.
use crate::view::{self as v, Operand as O};
use serde_json::{Value, json};

fn tags(tags: v::Tags) -> Value {
    match tags {
        v::Tags::All => json!("all"),
        v::Tags::Focusable => json!("focusable"),
        v::Tags::Only(tags) => json!(tags.iter().map(|tag| tag.name()).collect::<Vec<_>>()),
    }
}

fn attribute(grammar: v::AttributeValue) -> Value {
    use v::AttributeValue as A;
    let (kind, values) = match grammar {
        A::Ident => ("ident", None),
        A::IdentList => ("identList", None),
        A::Text => ("text", None),
        A::Boolean => ("boolean", None),
        A::Count => ("count", None),
        A::Number => ("number", None),
        A::PositiveCount => ("positiveCount", None),
        A::Keywords(values) => ("keywords", Some(values)),
        A::ElementType => ("elementType", None),
        A::Href => ("href", None),
        A::Resource => ("resource", None),
    };
    json!({"kind":kind,"values":values})
}

pub(super) fn registry() -> Value {
    let limits = json!({
        "batchBytes":v::DRAFT_MAX_BATCH_BYTES,"operations":v::DRAFT_MAX_BATCH_OPERATIONS,
        "treeDepth":v::DRAFT_MAX_TREE_DEPTH,
        "nodes":v::DRAFT_MAX_LIVE_NODES,"listeners":v::DRAFT_MAX_LIVE_LISTENERS,
        "valueBytes":v::DRAFT_MAX_VALUE_BYTES,"stylesheetBytes":v::DRAFT_MAX_STYLESHEET_BYTES,
        "retainedTextBytes":v::DRAFT_MAX_RETAINED_TEXT_BYTES,"fieldValueReservationBytes":v::DRAFT_FIELD_VALUE_RESERVATION_BYTES,
        "selectWork":v::DRAFT_MAX_SELECT_WORK,"selectNodeWork":v::DRAFT_SELECT_NODE_WORK,
        "radioWork":v::DRAFT_MAX_RADIO_WORK,"radioNodeWork":v::DRAFT_RADIO_NODE_WORK,
        "cascadeWork":v::DRAFT_MAX_CASCADE_WORK,
        "stylesheetWork":v::DRAFT_MAX_STYLESHEET_WORK,"stylesheetNodeWork":v::DRAFT_STYLESHEET_NODE_WORK,
        "events":v::DRAFT_MAX_PENDING_EVENTS,"commands":v::DRAFT_MAX_PENDING_COMMANDS,
        "identChars":v::attributes::MAX_IDENT_CHARS,"cssTokens":v::css::MAX_VALUE_TOKENS,"hexDigits":v::css::HEX_COLOR_DIGITS,
    });
    let wire = json!({
        "version":v::DRAFT_VERSION,"root":v::ROOT_NODE,"noNode":v::NO_NODE,
        "batchHeaderBytes":v::BATCH_HEADER_BYTES,"operationHeaderBytes":v::OPERATION_HEADER_BYTES,
        "alignment":v::STRING_ALIGNMENT,"eventHeaderBytes":v::EVENT_HEADER_BYTES,"eventStrings":v::EVENT_STRINGS,
        "eventFields":v::EVENT_FIELDS.iter().map(|field|json!({"name":field.name,"offset":field.offset,"type":field.ty.name()})).collect::<Vec<_>>(),
        "eventExport":v::EVENT_EXPORT,"commitImport":v::COMMIT_IMPORT,
    });
    let filters = json!({"anyKey":v::ANY_KEY,"modifierMask":v::MODIFIER_MASK,
        "options":v::ListenOption::ALL.iter().map(|option|(option.name(),*option as u32)).collect::<std::collections::BTreeMap<_,_>>(),
        "flags":v::EventFlag::ALL.iter().map(|flag|(flag.name(),*flag as u32)).collect::<std::collections::BTreeMap<_,_>>()});
    let defaults = json!({"rootSelector":v::ROOT_SELECTOR_IN_BROWSER,"rootStyle":v::ROOT_BASE_STYLE,
        "dialogBackdrop":v::DIALOG_BACKDROP,"dialogBackground":v::DIALOG_BACKGROUND,"dialogText":v::DIALOG_TEXT});
    json!({
        "limits":limits,"wire":wire,"filters":filters,"defaults":defaults,
        "capabilities":{"openUrl":v::OPEN_URL_CAPABILITY,"clipboard":v::CLIPBOARD_WRITE_CAPABILITY,"anyHttps":v::ANY_HTTPS_PATTERN},
        "opcodes":v::Opcode::ALL.iter().map(|op|json!({"code":op.code(),"name":op.name(),"operands":op.operands().iter().map(|&operand|{
            let kind=match operand {O::Node|O::Parent|O::Before|O::Handler|O::Sheet|O::Request=>"id",
                O::Tag|O::Attribute|O::Property|O::Event|O::Command|O::Key=>"code",O::Argument=>"f64",O::Text=>"string",_=>"u32"};
            json!({"name":operand.name(),"kind":kind})}).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "tags":v::Tag::ALL.iter().map(|tag|json!({"code":tag.code(),"name":tag.name(),"textChild":tag.admits_child(None),"children":v::Tag::ALL.iter().filter(|child|tag.admits_child(Some(**child))).map(|child|child.name()).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "attributes":v::Attribute::ALL.iter().map(|attr|json!({"code":attr.code(),"name":attr.name(),"tags":tags(attr.tags()),"grammar":attribute(attr.value())})).collect::<Vec<_>>(),
        "css":v::CssProperty::ALL.iter().map(|p|json!({"code":p.code(),"name":p.name(),"grammar":p.grammar(),"note":p.note()})).collect::<Vec<_>>(),
        "commands":v::Command::ALL.iter().map(|c|json!({"code":c.code(),"name":c.name(),"tags":tags(c.tags()),"argument":c.takes_argument()})).collect::<Vec<_>>(),
        "events":v::EventKind::ALL.iter().map(|kind|json!({"code":kind.code(),"name":kind.name(),"coalescible":kind.coalescible(),"edits":kind.edits(),"gesture":kind.user_gesture()})).collect::<Vec<_>>(),
        "keys":v::Key::ALL.iter().map(|key|json!({"code":key.code(),"name":key.name(),"domKey":key.dom_key()})).collect::<Vec<_>>(),
        "statuses":v::CommandStatus::ALL.iter().map(|status|(status.name(),status.code())).collect::<std::collections::BTreeMap<_,_>>(),
        "cssTypes":v::CssValue::ALL.iter().filter_map(|value|v::css::composite(value.name()).map(|grammar|(value.name(),grammar))).collect::<std::collections::BTreeMap<_,_>>(),
        "inputTypes":v::INPUT_TYPES,"buttonTypes":v::BUTTON_TYPES,
        "stylesheet":{"pseudo":v::PSEUDO_CLASSES,"media":v::MEDIA_FEATURES,"forbidden":v::FORBIDDEN_CSS,"forbiddenInline":v::FORBIDDEN_STYLE_VALUE},
    })
}
