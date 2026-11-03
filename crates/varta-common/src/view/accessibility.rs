//! Role and naming relations shared by the native and browser view hosts.
use super::Tag;

pub const NAME_FROM_CONTENT: &[&str] = &[
    "button",
    "link",
    "heading",
    "option",
    "cell",
    "columnheader",
    "row",
    "tab",
    "checkbox",
    "radio",
];

pub const LABELABLE: &[Tag] = &[Tag::Input, Tag::Textarea, Tag::Select, Tag::Progress];

#[must_use]
pub fn implicit_role(tag: Tag, input_type: &str, named_section: bool) -> Option<&'static str> {
    Some(match tag {
        Tag::Button => "button",
        Tag::A => "link",
        Tag::H1 | Tag::H2 | Tag::H3 => "heading",
        Tag::Ul | Tag::Ol => "list",
        Tag::Li => "listitem",
        Tag::Nav => "navigation",
        Tag::Main => "main",
        Tag::Header => "banner",
        Tag::Footer => "contentinfo",
        Tag::Aside => "complementary",
        Tag::Form => "form",
        Tag::P => "paragraph",
        Tag::Dialog => "dialog",
        Tag::Progress => "progressbar",
        Tag::Img => "image",
        Tag::Textarea => "textbox",
        Tag::Select => "combobox",
        Tag::Option => "option",
        Tag::Input => match input_type {
            "checkbox" => "checkbox",
            "radio" => "radio",
            "search" => "searchbox",
            "number" => "spinbutton",
            _ => "textbox",
        },
        Tag::Section if named_section => "region",
        _ => return None,
    })
}
