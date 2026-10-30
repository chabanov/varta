//! The draft v0 wire format: batch decoding (structure only, every byte
//! checked) and event record encoding. Semantic rules (ids, tags, values)
//! are checked by `tree` against the live view.
use crate::{ViewError, Violation};
use varta_common::view::{
    self, Attribute, BATCH_HEADER_BYTES, Command, CssProperty, DRAFT_MAX_BATCH_BYTES,
    DRAFT_MAX_BATCH_OPERATIONS, DRAFT_MAX_STYLESHEET_BYTES, DRAFT_MAX_VALUE_BYTES, DRAFT_VERSION,
    EVENT_FIELDS, EVENT_HEADER_BYTES, EventKind, OPERATION_HEADER_BYTES, Opcode, Operand,
    STRING_ALIGNMENT, Tag,
};

/// One decoded operand, in wire order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Id(u32),
    Tag(Tag),
    Attribute(Attribute),
    Property(CssProperty),
    Event(EventKind),
    Command(Command),
    /// A key code (or `ANY_KEY`), checked against the key table.
    Key(u16),
    Bits(u32),
    Number(f64),
    Text(String),
}

/// One decoded operation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Operation {
    pub opcode: Opcode,
    pub operands: Vec<Value>,
}
impl Operation {
    pub(crate) fn id(&self, index: usize) -> u32 {
        match self.operands.get(index) {
            Some(Value::Id(id) | Value::Bits(id)) => *id,
            _ => 0,
        }
    }
    pub(crate) fn text(&self) -> &str {
        self.operands
            .iter()
            .find_map(|operand| match operand {
                Value::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    operation: Option<usize>,
}
impl Reader<'_> {
    fn fail(&self, violation: Violation) -> ViewError {
        ViewError::Invalid {
            operation: self.operation,
            violation,
        }
    }
    fn take(&mut self, count: usize) -> Result<&[u8], ViewError> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| self.fail(Violation::Truncated))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }
    fn u16(&mut self) -> Result<u16, ViewError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn u32(&mut self) -> Result<u32, ViewError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    fn f64(&mut self) -> Result<f64, ViewError> {
        let bytes = self.take(8)?;
        let mut raw = [0; 8];
        raw.copy_from_slice(bytes);
        Ok(f64::from_le_bytes(raw))
    }
    fn zero(&mut self, count: usize) -> Result<(), ViewError> {
        if self.take(count)?.iter().any(|byte| *byte != 0) {
            return Err(self.fail(Violation::Padding));
        }
        Ok(())
    }
    /// A code operand: `u16` plus two zero padding bytes.
    fn code(&mut self) -> Result<u16, ViewError> {
        let code = self.u16()?;
        self.zero(2)?;
        Ok(code)
    }
    fn string(&mut self, limit: usize) -> Result<String, ViewError> {
        let length = self.u32()? as usize;
        if length > limit {
            return Err(self.fail(Violation::StringTooLong));
        }
        if !length.is_multiple_of(2) {
            return Err(self.fail(Violation::OddStringLength));
        }
        let operation = self.operation;
        let fail = |violation| ViewError::Invalid {
            operation,
            violation,
        };
        let bytes = self.take(length)?;
        let units = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair));
        let mut text = String::with_capacity(length / 2);
        for unit in char::decode_utf16(units) {
            let character = unit.map_err(|_| fail(Violation::LoneSurrogate))?;
            if character == '\0' {
                return Err(fail(Violation::NulCharacter));
            }
            text.push(character);
        }
        self.zero(length.next_multiple_of(STRING_ALIGNMENT) - length)?;
        Ok(text)
    }
}

/// Decode a whole batch; any structural violation rejects it before any
/// operation is looked at semantically.
pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<Operation>, ViewError> {
    let mut reader = Reader {
        bytes,
        at: 0,
        operation: None,
    };
    if bytes.len() > DRAFT_MAX_BATCH_BYTES {
        return Err(reader.fail(Violation::BatchTooLarge));
    }
    if bytes.len() < BATCH_HEADER_BYTES {
        return Err(reader.fail(Violation::Truncated));
    }
    if reader.u16()? != DRAFT_VERSION {
        return Err(reader.fail(Violation::Version));
    }
    if reader.u16()? != 0 {
        return Err(reader.fail(Violation::Flags));
    }
    let count = reader.u32()? as usize;
    if count > DRAFT_MAX_BATCH_OPERATIONS {
        return Err(reader.fail(Violation::TooManyOperations));
    }
    let mut operations = Vec::with_capacity(count.min(bytes.len() / OPERATION_HEADER_BYTES));
    for index in 0..count {
        reader.operation = Some(index);
        let opcode = Opcode::from_code(reader.take(1)?[0])
            .ok_or_else(|| reader.fail(Violation::UnknownOpcode))?;
        reader.zero(OPERATION_HEADER_BYTES - 1)?;
        let mut operands = Vec::with_capacity(opcode.operands().len());
        for operand in opcode.operands() {
            operands.push(read_operand(&mut reader, opcode, *operand)?);
        }
        operations.push(Operation { opcode, operands });
    }
    reader.operation = None;
    if reader.at != bytes.len() {
        return Err(reader.fail(Violation::TrailingBytes));
    }
    Ok(operations)
}

fn read_operand(
    reader: &mut Reader<'_>,
    opcode: Opcode,
    operand: Operand,
) -> Result<Value, ViewError> {
    use Operand as O;
    Ok(match operand {
        O::Node | O::Parent | O::Before | O::Handler | O::Sheet | O::Request => {
            Value::Id(reader.u32()?)
        }
        O::Options | O::BaseSeq | O::Checked | O::RequiredModifiers | O::ForbiddenModifiers => {
            Value::Bits(reader.u32()?)
        }
        O::Tag => Value::Tag(
            Tag::from_code(reader.code()?).ok_or_else(|| reader.fail(Violation::UnknownTag))?,
        ),
        O::Attribute => Value::Attribute(
            Attribute::from_code(reader.code()?)
                .ok_or_else(|| reader.fail(Violation::UnknownAttribute))?,
        ),
        O::Property => Value::Property(
            CssProperty::from_code(reader.code()?)
                .ok_or_else(|| reader.fail(Violation::UnknownProperty))?,
        ),
        O::Event => Value::Event(
            EventKind::from_code(reader.code()?)
                .ok_or_else(|| reader.fail(Violation::UnknownEvent))?,
        ),
        O::Command => Value::Command(
            Command::from_code(reader.code()?)
                .ok_or_else(|| reader.fail(Violation::UnknownCommand))?,
        ),
        O::Key => {
            let code = reader.code()?;
            if code != view::ANY_KEY && view::Key::from_code(code).is_none() {
                return Err(reader.fail(Violation::UnknownKey));
            }
            Value::Key(code)
        }
        O::Argument => Value::Number(reader.f64()?),
        O::Text => Value::Text(reader.string(if opcode == Opcode::Stylesheet {
            DRAFT_MAX_STYLESHEET_BYTES
        } else {
            DRAFT_MAX_VALUE_BYTES
        })?),
    })
}

/// One event record, as the host delivers it to the guest export.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub kind: EventKind,
    pub handler: u32,
    pub target: u32,
    pub seq: u32,
    pub flags: u32,
    pub status: u32,
    pub scroll_top: f64,
    pub scroll_height: f64,
    pub client_height: f64,
    pub key: String,
    pub value: String,
}
impl EventRecord {
    pub(crate) fn new(kind: EventKind, handler: u32, target: u32) -> Self {
        Self {
            kind,
            handler,
            target,
            seq: 0,
            flags: 0,
            status: 0,
            scroll_top: 0.0,
            scroll_height: 0.0,
            client_height: 0.0,
            key: String::new(),
            value: String::new(),
        }
    }
    /// The wire bytes: the fixed header at the [`EVENT_FIELDS`] offsets, then
    /// the `key` and `value` strings (UTF-16LE, padded).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![0; EVENT_HEADER_BYTES];
        for field in EVENT_FIELDS {
            let at = field.offset;
            match field.name {
                "version" => put(&mut bytes, at, &DRAFT_VERSION.to_le_bytes()),
                "kind" => put(&mut bytes, at, &self.kind.code().to_le_bytes()),
                "handler" => put(&mut bytes, at, &self.handler.to_le_bytes()),
                "target" => put(&mut bytes, at, &self.target.to_le_bytes()),
                "seq" => put(&mut bytes, at, &self.seq.to_le_bytes()),
                "flags" => put(&mut bytes, at, &self.flags.to_le_bytes()),
                "status" => put(&mut bytes, at, &self.status.to_le_bytes()),
                "scroll_top" => put(&mut bytes, at, &self.scroll_top.to_le_bytes()),
                "scroll_height" => put(&mut bytes, at, &self.scroll_height.to_le_bytes()),
                "client_height" => put(&mut bytes, at, &self.client_height.to_le_bytes()),
                _ => {}
            }
        }
        for text in [&self.key, &self.value] {
            push_string(&mut bytes, text);
        }
        bytes
    }
}

fn put(bytes: &mut [u8], at: usize, value: &[u8]) {
    bytes[at..at + value.len()].copy_from_slice(value);
}

/// Append a wire string: `u32` byte length, UTF-16LE units, zero padding.
/// Host strings are well formed (a Rust `str` has no lone surrogate).
pub(crate) fn push_string(bytes: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let length = units.len() * 2;
    bytes.extend_from_slice(&(length as u32).to_le_bytes());
    for unit in units {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes.resize(
        bytes.len() + length.next_multiple_of(STRING_ALIGNMENT) - length,
        0,
    );
}

/// Encodes batches in the draft v0 wire format, for tests and host tools
/// (the guest encodes its own batches with the engine's view prelude).
#[derive(Debug, Default, Clone)]
pub struct BatchWriter {
    body: Vec<u8>,
    count: u32,
}
impl BatchWriter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    fn op(&mut self, opcode: Opcode) -> &mut Self {
        self.count += 1;
        self.body.extend_from_slice(&[opcode.code(), 0, 0, 0]);
        self
    }
    fn u32(&mut self, value: u32) -> &mut Self {
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn code(&mut self, value: u16) -> &mut Self {
        self.body.extend_from_slice(&value.to_le_bytes());
        self.body.extend_from_slice(&[0, 0]);
        self
    }
    fn text(&mut self, text: &str) -> &mut Self {
        push_string(&mut self.body, text);
        self
    }
    /// Append raw UTF-16 code units as a string operand (lone surrogates and
    /// NUL included), for negative tests.
    pub fn raw_text(&mut self, units: &[u16]) -> &mut Self {
        self.body
            .extend_from_slice(&((units.len() * 2) as u32).to_le_bytes());
        for unit in units {
            self.body.extend_from_slice(&unit.to_le_bytes());
        }
        let length = units.len() * 2;
        self.body.resize(
            self.body.len() + length.next_multiple_of(STRING_ALIGNMENT) - length,
            0,
        );
        self
    }
    pub fn create_element(&mut self, node: u32, tag: Tag) -> &mut Self {
        self.op(Opcode::CreateElement).u32(node).code(tag.code())
    }
    pub fn create_text(&mut self, node: u32, text: &str) -> &mut Self {
        self.op(Opcode::CreateText).u32(node).text(text)
    }
    pub fn set_text(&mut self, node: u32, text: &str) -> &mut Self {
        self.op(Opcode::SetText).u32(node).text(text)
    }
    pub fn set_attribute(&mut self, node: u32, attribute: Attribute, value: &str) -> &mut Self {
        self.op(Opcode::SetAttribute)
            .u32(node)
            .code(attribute.code())
            .text(value)
    }
    pub fn remove_attribute(&mut self, node: u32, attribute: Attribute) -> &mut Self {
        self.op(Opcode::RemoveAttribute)
            .u32(node)
            .code(attribute.code())
    }
    pub fn set_class(&mut self, node: u32, class: &str) -> &mut Self {
        self.op(Opcode::SetClass).u32(node).text(class)
    }
    pub fn set_style(&mut self, node: u32, property: CssProperty, value: &str) -> &mut Self {
        self.op(Opcode::SetStyle)
            .u32(node)
            .code(property.code())
            .text(value)
    }
    pub fn remove_style(&mut self, node: u32, property: CssProperty) -> &mut Self {
        self.op(Opcode::RemoveStyle).u32(node).code(property.code())
    }
    pub fn insert(&mut self, parent: u32, node: u32, before: u32) -> &mut Self {
        self.op(Opcode::Insert).u32(parent).u32(node).u32(before)
    }
    pub fn remove(&mut self, node: u32) -> &mut Self {
        self.op(Opcode::Remove).u32(node)
    }
    pub fn listen(
        &mut self,
        node: u32,
        event: EventKind,
        handler: u32,
        filter: crate::Filter,
    ) -> &mut Self {
        self.op(Opcode::Listen)
            .u32(node)
            .code(event.code())
            .u32(handler)
            .u32(filter.options)
            .code(filter.key)
            .u32(filter.required)
            .u32(filter.forbidden)
    }
    pub fn unlisten(&mut self, node: u32, event: EventKind, handler: u32) -> &mut Self {
        self.op(Opcode::Unlisten)
            .u32(node)
            .code(event.code())
            .u32(handler)
    }
    pub fn set_value(&mut self, node: u32, value: &str, base_seq: u32) -> &mut Self {
        self.op(Opcode::SetValue)
            .u32(node)
            .text(value)
            .u32(base_seq)
    }
    pub fn set_checked(&mut self, node: u32, checked: u32, base_seq: u32) -> &mut Self {
        self.op(Opcode::SetChecked)
            .u32(node)
            .u32(checked)
            .u32(base_seq)
    }
    pub fn command(&mut self, node: u32, command: Command, argument: f64) -> &mut Self {
        self.op(Opcode::Command).u32(node).code(command.code());
        self.body.extend_from_slice(&argument.to_le_bytes());
        self
    }
    pub fn stylesheet(&mut self, sheet: u32, text: &str) -> &mut Self {
        self.op(Opcode::Stylesheet).u32(sheet).text(text)
    }
    pub fn set_title(&mut self, text: &str) -> &mut Self {
        self.op(Opcode::SetTitle).text(text)
    }
    pub fn copy_text(&mut self, request: u32, text: &str) -> &mut Self {
        self.op(Opcode::CopyText).u32(request).text(text)
    }
    /// Append raw bytes to the body (for malformed-batch tests).
    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.body.extend_from_slice(bytes);
        self
    }
    /// The batch: header and every operation so far.
    #[must_use]
    pub fn finish(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(BATCH_HEADER_BYTES + self.body.len());
        bytes.extend_from_slice(&DRAFT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&self.count.to_le_bytes());
        bytes.extend_from_slice(&self.body);
        bytes
    }
}
