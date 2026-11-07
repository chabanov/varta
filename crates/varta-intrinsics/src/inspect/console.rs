//! The console methods that inspect with their own options (v38
//! `console_methods`): `console.dir` (`util.inspect` with the caller's
//! options) and `console.table` (Node's `Console.prototype.table` over
//! `internal/cli_table`).
//!
//! Both take the `unknown[]` cell of a request the console prelude builds:
//! `[value, depth, showHidden, breakLength, compact, maxArrayLength,
//! maxStringLength, sorted, numericSeparator]` for [`RenderMode::Inspect`]
//! (an `undefined` option keeps its default, `null` is unbounded), and
//! `[data, properties]` for [`RenderMode::Table`] (`properties` a string
//! array or `undefined`; `data` an object, since the prelude logs a
//! primitive as `console.log` does).
use super::*;
use varta_common::console::RenderMode;

/// `cli_table`'s box drawing.
const TOP_LEFT: &str = "┌";
const TOP_MIDDLE: &str = "┬";
const TOP_RIGHT: &str = "┐";
const LEFT_MIDDLE: &str = "├";
const ROW_MIDDLE: &str = "┼";
const RIGHT_MIDDLE: &str = "┤";
const BOTTOM_LEFT: &str = "└";
const BOTTOM_MIDDLE: &str = "┴";
const BOTTOM_RIGHT: &str = "┘";
const MIDDLE_MIDDLE: &str = "─";
const LEFT: &str = "│ ";
const RIGHT: &str = " │";
const MIDDLE: &str = " │ ";
/// The column headings of `Console.prototype.table`.
const INDEX_KEY: &str = "(index)";
const ITERATION_KEY: &str = "(iteration index)";
const KEY_KEY: &str = "Key";
const VALUES_KEY: &str = "Values";
/// The positions of the options in an inspect request, after the value.
const OPTION_COUNT: usize = 8;

/// The text of a console render request held by the `unknown[]` cell
/// `cell`.
pub fn render<M: Memory + ?Sized>(
    memory: &M,
    mode: RenderMode,
    cell: u64,
    bounds: ModuleBounds,
    budget: &mut Budget,
) -> Result<Text, EncodeError> {
    let array = ReferenceBox::read_envelope(memory, cell, VERSION).map_err(layout)?;
    if array.kind != Kind::Array {
        return Err(EncodeError::Layout);
    }
    let vector = Vector::read_in(memory, array.payload, VERSION, bounds).map_err(layout)?;
    let mut inspector = Inspector::new(memory, bounds, budget);
    let mut args = Vec::with_capacity(vector.length);
    for at in 0..vector.length {
        args.push(inspector.element(&vector, at)?.ok_or(EncodeError::Layout)?);
    }
    match mode {
        RenderMode::Inspect => {
            let [value, options @ ..] = args.as_slice() else {
                return Err(EncodeError::Layout);
            };
            if options.len() != OPTION_COUNT {
                return Err(EncodeError::Layout);
            }
            let options = inspector.options_of(options)?;
            inspector.inspect_with(*value, options)
        }
        RenderMode::Table => {
            let [data, properties] = args.as_slice() else {
                return Err(EncodeError::Layout);
            };
            let properties = match properties.kind {
                Kind::Undefined => None,
                _ => Some(inspector.strings(*properties)?),
            };
            inspector.table(*data, properties.as_deref())
        }
    }
}

/// A Number option as a count: `None` for `undefined`, the type's maximum
/// for `null`, `Infinity` or NaN; a negative count is 0.
fn count(value: f64) -> usize {
    if value.is_nan() || value == f64::INFINITY {
        usize::MAX
    } else if value <= 0.0 {
        0
    } else {
        libm::floor(value) as usize
    }
}

impl<M: Memory + ?Sized> Inspector<'_, '_, M> {
    fn option_number(&self, value: Value) -> Result<Option<f64>, EncodeError> {
        Ok(match value.kind {
            Kind::Undefined => None,
            Kind::Null => Some(f64::INFINITY),
            Kind::Float64 => Some(f64::from_bits(value.payload)),
            Kind::I32 => Some(f64::from(value.payload as i64 as i32)),
            _ => return Err(EncodeError::Layout),
        })
    }
    fn option_flag(value: Value, default: bool) -> Result<bool, EncodeError> {
        match value.kind {
            Kind::Undefined => Ok(default),
            Kind::Boolean => Ok(value.payload != 0),
            _ => Err(EncodeError::Layout),
        }
    }
    /// The options of an inspect request.
    fn options_of(&self, values: &[Value]) -> Result<Options, EncodeError> {
        let [
            depth,
            show_hidden,
            break_length,
            compact,
            max_array_length,
            max_string_length,
            sorted,
            numeric_separator,
        ] = values
        else {
            return Err(EncodeError::Layout);
        };
        let mut options = Options::DEFAULT;
        if let Some(depth) = self.option_number(*depth)? {
            // `recurseTimes > depth` of integer levels: the floor compares
            // alike; NaN never compares greater.
            options.depth = if depth.is_nan() || depth >= i64::MAX as f64 {
                i64::MAX
            } else if depth <= i64::MIN as f64 {
                i64::MIN
            } else {
                libm::floor(depth) as i64
            };
        }
        options.show_hidden = Self::option_flag(*show_hidden, false)?;
        if let Some(length) = self.option_number(*break_length)? {
            options.break_length = count(length);
        }
        options.compact = match compact.kind {
            Kind::Undefined => options.compact,
            Kind::Boolean if compact.payload != 0 => Compact::Single,
            Kind::Boolean => Compact::Levels(0),
            _ => Compact::Levels(count(
                self.option_number(*compact)?.ok_or(EncodeError::Layout)?,
            )),
        };
        if let Some(length) = self.option_number(*max_array_length)? {
            options.max_array_length = count(length);
        }
        if let Some(length) = self.option_number(*max_string_length)? {
            options.max_string_length = count(length);
        }
        options.sorted = Self::option_flag(*sorted, false)?;
        options.numeric_separator = Self::option_flag(*numeric_separator, false)?;
        Ok(options)
    }
    /// The strings of a `string[]` value.
    fn strings(&mut self, value: Value) -> Result<Vec<Text>, EncodeError> {
        if value.kind != Kind::Array {
            return Err(EncodeError::Layout);
        }
        let vector =
            Vector::read_in(self.memory, value.payload, VERSION, self.bounds).map_err(layout)?;
        let mut strings = Vec::with_capacity(vector.length);
        for at in 0..vector.length {
            let element = self.element(&vector, at)?.ok_or(EncodeError::Layout)?;
            if element.kind != Kind::String {
                return Err(EncodeError::Layout);
            }
            strings.push(self.string(element.payload)?);
        }
        Ok(strings)
    }

    /// `ObjectKeys(value)` with the property values: array indices without
    /// holes, dictionary keys, the public fields of an object, class
    /// instance or Error object, and nothing for other values.
    fn own_entries(&mut self, value: Value) -> Result<Vec<(Text, Value)>, EncodeError> {
        let mut entries = Vec::new();
        match value.kind {
            Kind::Array => {
                let vector = Vector::read_in(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                for at in 0..vector.length {
                    if let Some(element) = self.element(&vector, at)? {
                        entries.push((digits(at as u64, 1), element));
                    }
                }
            }
            Kind::Bytes => {
                let vector = byte_vector(self.memory, value.payload, VERSION, self.bounds)
                    .map_err(layout)?;
                for at in 0..vector.length {
                    let byte = byte_at(self.memory, &vector, at).map_err(layout)?;
                    let number = Value {
                        kind: Kind::Float64,
                        payload: f64::from(byte).to_bits(),
                    };
                    entries.push((digits(at as u64, 1), number));
                }
            }
            Kind::Dictionary => {
                let dictionary =
                    Dictionary::read_in(self.memory, value.payload, VERSION, self.bounds)
                        .map_err(layout)?;
                for at in 0..dictionary.keys.length {
                    let Some((key, entry)) = dictionary.entry(self.memory, at).map_err(layout)?
                    else {
                        continue;
                    };
                    let entry = self.resolve(dictionary.values.kind as u32, entry)?;
                    entries.push((self.string(key)?, entry));
                }
            }
            Kind::Record => {
                let record = self.record(value.payload, None)?;
                if matches!(
                    record.display,
                    RecordDisplay::Object
                        | RecordDisplay::Class
                        | RecordDisplay::Error
                        | RecordDisplay::AggregateError
                ) {
                    entries.extend(
                        Self::public(&record).map(|field| (utf16(&field.key), field.value)),
                    );
                }
            }
            _ => {}
        }
        Ok(entries)
    }
    /// `_inspect` of a table cell.
    fn cell(&mut self, value: Value) -> Result<Text, EncodeError> {
        let object = matches!(
            value.kind,
            Kind::Record | Kind::Dictionary | Kind::Promise | Kind::Error | Kind::Function
        );
        let elided = object && self.own_entries(value)?.len() > 2;
        self.inspect_with(value, Options::table_cell(elided))
    }
    /// Whether a table row item is a primitive (`null` or neither a
    /// function nor an object).
    fn primitive_kind(value: Value) -> bool {
        matches!(
            value.kind,
            Kind::String
                | Kind::Float64
                | Kind::I32
                | Kind::BigInt
                | Kind::Boolean
                | Kind::Undefined
                | Kind::Null
        )
    }
    /// The collection display of a record value, if it is a Map or a Set.
    fn collection_entries(
        &mut self,
        value: Value,
    ) -> Result<Option<(bool, Vec<CollectionEntry>)>, EncodeError> {
        if value.kind != Kind::Record {
            return Ok(None);
        }
        let record = self.record(value.payload, None)?;
        let map = match record.display {
            RecordDisplay::Map => true,
            RecordDisplay::Set => false,
            _ => return Ok(None),
        };
        let vector = |inspector: &Self, name: &[u8]| -> Result<Vector, EncodeError> {
            let field = record.field(name).ok_or(EncodeError::Layout)?;
            if field.value.kind != Kind::Array {
                return Err(EncodeError::Layout);
            }
            Vector::read_in(
                inspector.memory,
                field.value.payload,
                VERSION,
                inspector.bounds,
            )
            .map_err(layout)
        };
        let keys = vector(self, STORE_KEYS)?;
        let values = if map {
            Some(vector(self, STORE_VALUES)?)
        } else {
            None
        };
        let mut entries = Vec::new();
        for at in 0..keys.length {
            let Some(key) = self.element(&keys, at)? else {
                continue;
            };
            let value = match &values {
                Some(values) => Some(self.element(values, at)?.ok_or(EncodeError::Layout)?),
                None => None,
            };
            entries.push((key, value));
        }
        Ok(Some((map, entries)))
    }

    /// `Console.prototype.table(data, properties)` of an object `data`.
    fn table(&mut self, data: Value, properties: Option<&[Text]>) -> Result<Text, EncodeError> {
        if let Some((map, entries)) = self.collection_entries(data)? {
            let mut index = Vec::with_capacity(entries.len());
            let mut keys = Vec::with_capacity(entries.len());
            let mut values = Vec::with_capacity(entries.len());
            for (at, (key, value)) in entries.iter().enumerate() {
                let position = Value {
                    kind: Kind::Float64,
                    payload: (at as f64).to_bits(),
                };
                index.push(Some(self.cell(position)?));
                if let Some(value) = value {
                    keys.push(Some(self.cell(*key)?));
                    values.push(Some(self.cell(*value)?));
                } else {
                    values.push(Some(self.cell(*key)?));
                }
            }
            return Ok(if map {
                render_table(
                    &[ascii(ITERATION_KEY), ascii(KEY_KEY), ascii(VALUES_KEY)],
                    &[index, keys, values],
                )
            } else {
                render_table(&[ascii(ITERATION_KEY), ascii(VALUES_KEY)], &[index, values])
            });
        }
        let rows = self.own_entries(data)?;
        // `map`: the columns by key, in `ObjectKeys` order of a null-prototype
        // object (canonical array indices ascending, then insertion order).
        let mut columns: Vec<(Text, Vec<Option<Text>>)> = Vec::new();
        let mut has_primitives = false;
        let mut primitives: Vec<Option<Text>> = Vec::new();
        for (at, (_, item)) in rows.iter().enumerate() {
            let primitive = Self::primitive_kind(*item);
            if properties.is_none() && primitive {
                has_primitives = true;
                set(&mut primitives, at, self.cell(*item)?);
                continue;
            }
            let own = self.own_entries(*item)?;
            let keys: Vec<Text> = match properties {
                Some(properties) => properties.to_vec(),
                None => own.iter().map(|(key, _)| key.clone()).collect(),
            };
            for key in keys {
                let column = match columns.iter().position(|(name, _)| *name == key) {
                    Some(column) => column,
                    None => {
                        columns.push((key.clone(), Vec::new()));
                        columns.len() - 1
                    }
                };
                let found = own
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| *value);
                let text = match found {
                    Some(value) if !(primitive && properties.is_some()) => self.cell(value)?,
                    _ => Text::new(),
                };
                set(&mut columns[column].1, at, text);
            }
        }
        columns.sort_by_key(|(name, _)| {
            let index = array_index(name);
            (index.is_none(), index.unwrap_or(0))
        });
        let mut head = alloc::vec![ascii(INDEX_KEY)];
        let mut values = alloc::vec![rows.iter().map(|(key, _)| Some(key.clone())).collect()];
        for (name, column) in columns {
            head.push(name);
            values.push(column);
        }
        if has_primitives {
            head.push(ascii(VALUES_KEY));
            values.push(primitives);
        }
        Ok(render_table(&head, &values))
    }
}

/// `column[at] = text` of a sparse JavaScript array.
fn set(column: &mut Vec<Option<Text>>, at: usize, text: Text) {
    if column.len() <= at {
        column.resize(at + 1, None);
    }
    column[at] = Some(text);
}

/// A Map entry (key and value) or a Set entry (the value as its key).
type CollectionEntry = (Value, Option<Value>);

/// The canonical array index a property key names, if any.
fn array_index(key: &[u16]) -> Option<u32> {
    if key.is_empty() || key.len() > 10 || (key.len() > 1 && key[0] == u16::from(b'0')) {
        return None;
    }
    let mut index: u64 = 0;
    for unit in key {
        let digit = u8::try_from(*unit).ok().filter(u8::is_ascii_digit)?;
        index = index * 10 + u64::from(digit - b'0');
    }
    u32::try_from(index).ok().filter(|index| *index != u32::MAX)
}

/// `internal/cli_table`'s `renderRow`.
fn render_row(row: &[Text], widths: &[usize]) -> Text {
    let mut out = ascii(LEFT);
    for (at, cell) in row.iter().enumerate() {
        out.extend_from_slice(cell);
        out.extend(spaces(widths[at].saturating_sub(width(cell))));
        if at + 1 != row.len() {
            out.extend(ascii(MIDDLE));
        }
    }
    out.extend(ascii(RIGHT));
    out
}

/// `internal/cli_table`'s `table(head, columns)`: a missing cell of a
/// sparse column is empty.
fn render_table(head: &[Text], columns: &[Vec<Option<Text>>]) -> Text {
    let longest = columns.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths: Vec<usize> = head.iter().map(|heading| width(heading)).collect();
    let mut rows: Vec<Vec<Text>> = alloc::vec![Vec::with_capacity(head.len()); longest];
    for (column_at, column) in columns.iter().enumerate() {
        for (row_at, row) in rows.iter_mut().enumerate() {
            let value = column.get(row_at).cloned().flatten().unwrap_or_default();
            widths[column_at] = widths[column_at].max(width(&value));
            row.push(value);
        }
    }
    let divider: Vec<Text> = widths
        .iter()
        .map(|column| ascii(MIDDLE_MIDDLE).repeat(column + 2))
        .collect();
    let line = |left: &str, middle: &str, right: &str| -> Text {
        let mut text = ascii(left);
        text.extend(join(&divider, &ascii(middle)));
        text.extend(ascii(right));
        text
    };
    let newline = ascii("\n");
    let mut result = line(TOP_LEFT, TOP_MIDDLE, TOP_RIGHT);
    result.extend_from_slice(&newline);
    result.extend(render_row(head, &widths));
    result.extend_from_slice(&newline);
    result.extend(line(LEFT_MIDDLE, ROW_MIDDLE, RIGHT_MIDDLE));
    result.extend_from_slice(&newline);
    for row in &rows {
        result.extend(render_row(row, &widths));
        result.extend_from_slice(&newline);
    }
    result.extend(line(BOTTOM_LEFT, BOTTOM_MIDDLE, BOTTOM_RIGHT));
    result
}
