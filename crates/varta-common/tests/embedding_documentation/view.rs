//! The L0 view protocol draft v0 tables in `docs/embedding.md` are generated
//! from `varta_common::view`, so an applier written from the document uses the
//! same codes as the registry.
use super::DOCUMENT;
use crate::support::{assert_block, table};
use varta_common::view::{
    self, Attribute, Command, CssProperty, EventFlag, EventKind, ListenOption, Opcode, Tag,
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

#[test]
fn view_draft_tables_match_registry() {
    assert_block(DOCUMENT, "view-layout", &layout());
    assert_block(DOCUMENT, "view-opcodes", &opcodes());
    assert_block(
        DOCUMENT,
        "view-tags",
        &codes(Tag::ALL.iter().map(|v| (u32::from(v.code()), v.name()))),
    );
    assert_block(
        DOCUMENT,
        "view-attributes",
        &codes(
            Attribute::ALL
                .iter()
                .map(|v| (u32::from(v.code()), v.name())),
        ),
    );
    assert_block(
        DOCUMENT,
        "view-css-properties",
        &codes(
            CssProperty::ALL
                .iter()
                .map(|v| (u32::from(v.code()), v.name())),
        ),
    );
    assert_block(
        DOCUMENT,
        "view-events",
        &codes(
            EventKind::ALL
                .iter()
                .map(|v| (u32::from(v.code()), v.name())),
        ),
    );
    assert_block(
        DOCUMENT,
        "view-commands",
        &codes(Command::ALL.iter().map(|v| (u32::from(v.code()), v.name()))),
    );
}
