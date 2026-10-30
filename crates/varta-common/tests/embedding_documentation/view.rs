//! The L0 view protocol draft v0 tables in `docs/embedding.md` are generated
//! from `varta_common::view`, so an applier written from the document uses the
//! same codes as the registry.
use super::DOCUMENT;
use crate::support::{assert_block, table};
use varta_common::view::{
    self, Attribute, Command, CommandStatus, CssProperty, CssValue, DraftCapability, DraftGrant,
    EventFlag, EventKind, Key, ListenOption, Opcode, ResultOperation, Tag, Tags,
};

fn codes(rows: impl Iterator<Item = (u32, &'static str)>) -> String {
    table(
        &["Code", "Name"],
        rows.map(|(code, name)| vec![code.to_string(), format!("`{name}`")]),
    )
}

fn opcodes() -> String {
    table(
        &["Opcode", "Operation", "Operands (wire order)"],
        Opcode::ALL.iter().map(|&opcode| {
            let operands: Vec<String> = opcode
                .operands()
                .iter()
                .map(|operand| format!("{} ({})", operand.name(), operand.wire()))
                .collect();
            vec![
                opcode.code().to_string(),
                format!("`{}`", opcode.name()),
                operands.join(", "),
            ]
        }),
    )
}

fn layout() -> String {
    let scalars = |values: &[varta_common::HostScalar]| {
        values
            .iter()
            .map(|value| format!("{value:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut rows: Vec<(String, String)> = vec![
        ("Draft version".into(), view::DRAFT_VERSION.to_string()),
        (
            "Patch import (not registered)".into(),
            format!(
                "`varta.{}` ({}) → ({})",
                view::COMMIT_IMPORT,
                scalars(&view::COMMIT_PARAMS),
                scalars(&view::COMMIT_RESULTS)
            ),
        ),
        (
            "Event export (not registered)".into(),
            format!(
                "`{}` ({}) → ()",
                view::EVENT_EXPORT,
                scalars(&view::EVENT_EXPORT_PARAMS)
            ),
        ),
        ("Root node id".into(), view::ROOT_NODE.to_string()),
        ("\"No node\" id".into(), view::NO_NODE.to_string()),
        (
            "Batch header bytes".into(),
            view::BATCH_HEADER_BYTES.to_string(),
        ),
        (
            "Operation header bytes".into(),
            view::OPERATION_HEADER_BYTES.to_string(),
        ),
        (
            "String alignment bytes".into(),
            view::STRING_ALIGNMENT.to_string(),
        ),
        (
            "Event header bytes".into(),
            view::EVENT_HEADER_BYTES.to_string(),
        ),
        (
            "Event strings after the header".into(),
            view::EVENT_STRINGS.join(", "),
        ),
        (
            "Placeholder: batch bytes / operations".into(),
            format!(
                "{} / {}",
                view::DRAFT_MAX_BATCH_BYTES,
                view::DRAFT_MAX_BATCH_OPERATIONS
            ),
        ),
        (
            "Placeholder: live nodes / listeners".into(),
            format!(
                "{} / {}",
                view::DRAFT_MAX_LIVE_NODES,
                view::DRAFT_MAX_LIVE_LISTENERS
            ),
        ),
        (
            "Placeholder: value bytes / stylesheet bytes / pending events".into(),
            format!(
                "{} / {} / {}",
                view::DRAFT_MAX_VALUE_BYTES,
                view::DRAFT_MAX_STYLESHEET_BYTES,
                view::DRAFT_MAX_PENDING_EVENTS
            ),
        ),
        (
            "Placeholder: pending `copy_text` requests".into(),
            view::DRAFT_MAX_PENDING_COMMANDS.to_string(),
        ),
        (
            "Key filter: any key".into(),
            format!("key code {}", view::ANY_KEY),
        ),
        (
            "Key filter: modifier mask".into(),
            format!(
                "{} (shift, ctrl, alt, meta event flags)",
                view::MODIFIER_MASK
            ),
        ),
    ];
    for &option in ListenOption::ALL {
        rows.push((
            format!("`listen` option `{}`", option.name()),
            format!("bit value {}", option as u32),
        ));
    }
    for &flag in EventFlag::ALL {
        rows.push((
            format!("Event flag `{}`", flag.name()),
            format!("bit value {}", flag as u32),
        ));
    }
    for field in view::EVENT_FIELDS {
        rows.push((
            format!("Event field `{}`", field.name),
            format!("offset {}, {}", field.offset, field.ty.name()),
        ));
    }
    table(
        &["Item", "Value"],
        rows.into_iter().map(|(item, value)| vec![item, value]),
    )
}

/// Markdown table cells cannot contain a bare `|`.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn tags(tags: Tags) -> String {
    match tags {
        Tags::All => "all".to_owned(),
        Tags::Focusable => "focusable elements".to_owned(),
        Tags::Only(tags) => tags
            .iter()
            .map(|tag| format!("`{}`", tag.name()))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn attributes() -> String {
    table(
        &["Code", "Attribute", "Elements", "Value"],
        Attribute::ALL.iter().map(|&attribute| {
            vec![
                attribute.code().to_string(),
                format!("`{}`", attribute.name()),
                tags(attribute.tags()),
                cell(&attribute.value().describe()),
            ]
        }),
    )
}

fn css_properties() -> String {
    table(
        &["Code", "Property", "Values", "Note"],
        CssProperty::ALL.iter().map(|&property| {
            vec![
                property.code().to_string(),
                format!("`{}`", property.name()),
                format!("`{}`", cell(property.grammar())),
                property.note().map_or_else(|| "—".to_owned(), cell),
            ]
        }),
    )
}

fn css_values() -> String {
    table(
        &["Type", "Definition"],
        CssValue::ALL
            .iter()
            .map(|&value| vec![format!("`{}`", value.name()), cell(value.definition())]),
    )
}

fn stylesheet_tokens() -> String {
    let quoted = |items: &[&str]| {
        items
            .iter()
            .map(|item| format!("`{}`", cell(item)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    table(
        &["Item", "Values"],
        [
            ("Pseudo-classes", quoted(view::PSEUDO_CLASSES)),
            ("Media features", quoted(view::MEDIA_FEATURES)),
            ("Forbidden anywhere", quoted(view::FORBIDDEN_CSS)),
            (
                "Forbidden in `set_style` values",
                quoted(view::FORBIDDEN_STYLE_VALUE),
            ),
        ]
        .into_iter()
        .map(|(item, values)| vec![item.to_owned(), values]),
    )
}

fn events() -> String {
    let flag = |set: bool| if set { "yes" } else { "—" }.to_owned();
    table(
        &["Code", "Event", "Coalescible", "Increments seq"],
        EventKind::ALL.iter().map(|&kind| {
            vec![
                kind.code().to_string(),
                format!("`{}`", kind.name()),
                flag(kind.coalescible()),
                flag(kind.edits()),
            ]
        }),
    )
}

fn commands() -> String {
    table(
        &["Code", "Command", "Elements", "Argument"],
        Command::ALL.iter().map(|&command| {
            vec![
                command.code().to_string(),
                format!("`{}`", command.name()),
                tags(command.tags()),
                if command.takes_argument() {
                    "finite nonnegative top offset (CSS px)"
                } else {
                    "must be 0"
                }
                .to_owned(),
            ]
        }),
    )
}

fn keys() -> String {
    table(
        &["Code", "Key", "DOM `KeyboardEvent.key`"],
        Key::ALL.iter().map(|&key| {
            vec![
                key.code().to_string(),
                format!("`{}`", key.name()),
                format!("`{}`", key.dom_key()),
            ]
        }),
    )
}

fn capabilities() -> String {
    table(
        &["Capability", "Grant", "Authorizes (user gesture only)"],
        DraftCapability::ALL.iter().map(|&capability| {
            let grant = match capability.grant() {
                DraftGrant::Boolean => "boolean `enabled`",
                DraftGrant::Urls => "URL `patterns` (`https://` admitted)",
            };
            vec![
                format!("`{}`", capability.id()),
                grant.to_owned(),
                capability.use_site().to_owned(),
            ]
        }),
    )
}

fn defaults() -> String {
    let schemes = |values: [&str; 2]| format!("`{}` / `{}`", values[0], values[1]);
    table(
        &["Default", "Light / dark"],
        [
            ("`dialog` backdrop colour", schemes(view::DIALOG_BACKDROP)),
            (
                "`dialog` background colour",
                schemes(view::DIALOG_BACKGROUND),
            ),
            ("`dialog` text colour", schemes(view::DIALOG_TEXT)),
            (
                "Browser selector substituted for `:root`",
                format!("`{}` (both schemes)", view::ROOT_SELECTOR_IN_BROWSER),
            ),
        ]
        .into_iter()
        .map(|(item, value)| vec![item.to_owned(), value]),
    )
}

fn command_results() -> String {
    let mut rows: Vec<Vec<String>> = CommandStatus::ALL
        .iter()
        .map(|&status| {
            let operations: Vec<String> = ResultOperation::ALL
                .iter()
                .filter(|operation| operation.statuses().contains(&status))
                .map(|operation| format!("`{}`", operation.name()))
                .collect();
            vec![
                status.code().to_string(),
                format!("`{}`", status.name()),
                operations.join(", "),
                status.meaning().to_owned(),
                status
                    .dom_exception()
                    .map_or_else(|| "resolves".to_owned(), |name| format!("`{name}`")),
            ]
        })
        .collect();
    for &operation in ResultOperation::ALL {
        rows.push(vec![
            "—".to_owned(),
            format!("identity of `{}`", operation.name()),
            format!("`{}`", operation.name()),
            operation.identity().to_owned(),
            "—".to_owned(),
        ]);
    }
    table(
        &["Status", "Name", "Operations", "Meaning", "Guest API"],
        rows,
    )
}

#[test]
fn view_draft_tables_match_registry() {
    assert_block(DOCUMENT, "view-layout", &layout());
    assert_block(DOCUMENT, "view-opcodes", &opcodes());
    assert_block(
        DOCUMENT,
        "view-tags",
        &codes(Tag::ALL.iter().map(|v| (u32::from(v.code()), v.name()))),
    );
    assert_block(DOCUMENT, "view-attributes", &attributes());
    assert_block(DOCUMENT, "view-css-properties", &css_properties());
    assert_block(DOCUMENT, "view-css-values", &css_values());
    assert_block(DOCUMENT, "view-stylesheet-tokens", &stylesheet_tokens());
    assert_block(DOCUMENT, "view-events", &events());
    assert_block(DOCUMENT, "view-commands", &commands());
    assert_block(DOCUMENT, "view-keys", &keys());
    assert_block(DOCUMENT, "view-capabilities", &capabilities());
    assert_block(DOCUMENT, "view-defaults", &defaults());
    assert_block(DOCUMENT, "view-command-results", &command_results());
}
