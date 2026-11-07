//! V38 character classes of the `v` flag (ES2024 ClassSetExpression):
//! nested classes, intersection `&&`, subtraction `--`, string disjunctions
//! `\q{…}` and the properties of strings, with the early errors and V8's
//! diagnostics; and Unicode property escapes in every Unicode mode.
//!
//! Under `v` with `i` every operand is folded (MaybeSimpleCaseFolding) before
//! the set operations and a complement is taken among the characters that
//! are their own folding, so the result is a set of canonical characters;
//! the compiler then widens it to whole case classes, which matches exactly
//! the characters whose Canonicalize value is in the set.
use super::RegExpError;
use super::case;
use super::parse::{Node, Parser};
use super::property;
use super::set::{MAX_CODE_POINT, Set};
use alloc::{boxed::Box, collections::BTreeSet, string::String, vec::Vec};

/// A class value: single characters, strings of any other length, and
/// whether the class may contain strings (MayContainStrings, a syntactic
/// property that decides whether it can be negated).
#[derive(Clone, Debug, Default)]
pub struct ClassValue {
    pub set: Set,
    pub strings: BTreeSet<Vec<u32>>,
    pub may_strings: bool,
}

impl ClassValue {
    fn chars(set: Set) -> Self {
        Self {
            set,
            ..Self::default()
        }
    }
    fn union(mut self, other: Self) -> Self {
        self.set.union(&other.set);
        self.strings.extend(other.strings);
        self.may_strings |= other.may_strings;
        self
    }
    fn intersection(self, other: &Self) -> Self {
        Self {
            set: self.set.intersection(&other.set),
            strings: self.strings.intersection(&other.strings).cloned().collect(),
            may_strings: self.may_strings && other.may_strings,
        }
    }
    fn difference(self, other: &Self) -> Self {
        Self {
            set: self.set.difference(&other.set),
            strings: self.strings.difference(&other.strings).cloned().collect(),
            may_strings: self.may_strings,
        }
    }
    /// The class as a pattern node (CompileAtom with strings): its strings
    /// longest first, then its single characters, then the empty string.
    pub fn into_node(self) -> Node {
        if self.strings.is_empty() {
            return Node::Class {
                set: self.set,
                negated: false,
            };
        }
        let mut strings: Vec<Vec<u32>> = self.strings.into_iter().collect();
        strings.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        let empty = strings.last().is_some_and(Vec::is_empty);
        let mut alternatives: Vec<Node> = strings
            .into_iter()
            .filter(|string| !string.is_empty())
            .map(|string| Node::Concat(string.into_iter().map(Node::Char).collect()))
            .collect();
        if !self.set.ranges().is_empty() {
            alternatives.push(Node::Class {
                set: self.set,
                negated: false,
            });
        }
        if empty {
            alternatives.push(Node::Empty);
        }
        Node::Group(Box::new(Node::Alternation(alternatives)), None)
    }
}

/// ClassSetSyntaxCharacter.
const SYNTAX: &str = "()[]{}/-\\|";
/// ClassSetReservedPunctuator (escapable as themselves).
const RESERVED: &str = "&-!#%,:;<=>@`~";
/// The characters of ClassSetReservedDoublePunctuator.
const DOUBLED: &str = "&!#$%*+,.:;<=>?@^`~";
const INVALID_CHARACTER: &str = "Invalid character in character class";
const INVALID_OPERATION: &str = "Invalid set operation in character class";
const BACKSPACE: u32 = 0x08;

fn is(ch: Option<u32>, text: char) -> bool {
    ch == Some(u32::from(text))
}

impl Parser {
    /// `\p{…}` or `\P{…}` with the cursor on the backslash.
    pub(super) fn property(&mut self) -> Result<ClassValue, RegExpError> {
        let negated = is(self.peek_at(1), 'P');
        self.at += 2;
        let invalid = |parser: &Self| parser.error("Invalid property name");
        if !self.eat(b'{') {
            return Err(invalid(self));
        }
        let mut text = String::new();
        loop {
            let Some(ch) = self.peek() else {
                return Err(invalid(self));
            };
            self.at += 1;
            if ch == u32::from('}') {
                break;
            }
            match char::from_u32(ch) {
                Some(ch) if ch.is_ascii_alphanumeric() || ch == '_' || ch == '=' => text.push(ch),
                _ => return Err(invalid(self)),
            }
        }
        let (name, value) = match text.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (text.as_str(), None),
        };
        let sets = self.flags.unicode_sets;
        let Some(found) = property::lookup(name, value, sets && !negated) else {
            return Err(invalid(self));
        };
        let value = ClassValue {
            may_strings: !found.strings.is_empty(),
            set: found.set,
            strings: found.strings,
        };
        if !sets {
            return Ok(if negated {
                ClassValue::chars(value.set.complement(MAX_CODE_POINT))
            } else {
                value
            });
        }
        let value = self.fold(value);
        Ok(if negated {
            ClassValue::chars(self.complement(&value.set))
        } else {
            value
        })
    }
    /// MaybeSimpleCaseFolding under `i`.
    fn fold(&self, value: ClassValue) -> ClassValue {
        if !self.flags.ignore_case {
            return value;
        }
        let mut folded = ClassValue {
            set: case::fold(&value.set),
            strings: BTreeSet::new(),
            may_strings: value.may_strings,
        };
        for string in value.strings {
            folded.strings.insert(
                string
                    .into_iter()
                    .map(|ch| case::canonicalize(ch, true))
                    .collect(),
            );
        }
        folded
    }
    /// CharacterComplement: among every code point, or under `i` among the
    /// code points that are their own folding.
    fn complement(&self, set: &Set) -> Set {
        if self.flags.ignore_case {
            case::universe().difference(set)
        } else {
            set.complement(MAX_CODE_POINT)
        }
    }

    /// A `v` class at the cursor (on `[`).
    pub(super) fn class_set(&mut self) -> Result<Node, RegExpError> {
        Ok(self.nested_class()?.into_node())
    }
    /// NestedClass `[…]` or `[^…]` with the cursor on `[`.
    fn nested_class(&mut self) -> Result<ClassValue, RegExpError> {
        self.at += 1;
        let negated = self.eat(b'^');
        let value = self.class_contents()?;
        if !negated {
            return Ok(value);
        }
        if value.may_strings {
            return Err(self.error("Negated character class may contain strings"));
        }
        Ok(ClassValue::chars(self.complement(&value.set)))
    }
    fn operator(&self, text: &str) -> bool {
        let mut chars = text.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        is(self.peek(), first) && is(self.peek_at(1), second)
    }
    /// ClassSetExpression up to and including the closing `]`.
    fn class_contents(&mut self) -> Result<ClassValue, RegExpError> {
        if self.eat(b']') {
            return Ok(ClassValue::default());
        }
        let (first, range) = self.union_item()?;
        for (text, intersect) in [("&&", true), ("--", false)] {
            if !self.operator(text) {
                continue;
            }
            if range {
                return Err(self.error(INVALID_OPERATION));
            }
            let mut value = first;
            while self.operator(text) {
                self.at += 2;
                if intersect && is(self.peek(), '&') {
                    return Err(self.error(INVALID_CHARACTER));
                }
                let (operand, _) = self.operand()?;
                value = if intersect {
                    value.intersection(&operand)
                } else {
                    value.difference(&operand)
                };
            }
            if self.eat(b']') {
                return Ok(value);
            }
            if self.peek().is_none() {
                return Err(self.error("Unterminated character class"));
            }
            return Err(self.error(INVALID_OPERATION));
        }
        let mut value = first;
        loop {
            if self.eat(b']') {
                return Ok(value);
            }
            if self.operator("&&") || self.operator("--") {
                return Err(self.error(INVALID_OPERATION));
            }
            let (item, _) = self.union_item()?;
            value = value.union(item);
        }
    }
    /// A ClassSetOperand or ClassSetRange of a union, and whether it was a
    /// range.
    fn union_item(&mut self) -> Result<(ClassValue, bool), RegExpError> {
        let (operand, single) = self.operand()?;
        let dash = is(self.peek(), '-') && !is(self.peek_at(1), '-');
        if !dash {
            return Ok((operand, false));
        }
        let Some(low) = single else {
            return Err(self.error("Invalid character class"));
        };
        self.at += 1;
        let (_, high) = self.operand()?;
        let Some(high) = high else {
            return Err(self.error("Invalid character class"));
        };
        if low > high {
            return Err(self.error("Range out of order in character class"));
        }
        Ok((
            self.fold(ClassValue::chars(Set::from_ranges([(low, high)]))),
            true,
        ))
    }
    /// A ClassSetOperand, with its character when it is a single
    /// ClassSetCharacter (before folding, for ranges).
    fn operand(&mut self) -> Result<(ClassValue, Option<u32>), RegExpError> {
        let Some(ch) = self.peek() else {
            return Err(self.error("Unterminated character class"));
        };
        if ch == u32::from('[') {
            return Ok((self.nested_class()?, None));
        }
        if ch == u32::from('\\') {
            let next = self.peek_at(1);
            if is(next, 'q') {
                if !is(self.peek_at(2), '{') {
                    return Err(self.error("Invalid escape"));
                }
                return Ok((self.string_disjunction()?, None));
            }
            if is(next, 'p') || is(next, 'P') {
                return Ok((self.property()?, None));
            }
            if let Some(next) = next
                && let Some(set) = self.class_escape(next)?
            {
                self.at += 2;
                return Ok((self.fold(ClassValue::chars(set)), None));
            }
        }
        let ch = self.set_character()?;
        Ok((self.fold(ClassValue::chars(Set::single(ch))), Some(ch)))
    }
    /// A ClassSetCharacter at the cursor.
    fn set_character(&mut self) -> Result<u32, RegExpError> {
        let Some(ch) = self.peek() else {
            return Err(self.error("Unterminated character class"));
        };
        let character = char::from_u32(ch);
        if ch == u32::from('\\') {
            let Some(next) = self.peek_at(1) else {
                return Err(self.error("\\ at end of pattern"));
            };
            if char::from_u32(next).is_some_and(|next| RESERVED.contains(next)) {
                self.at += 2;
                return Ok(next);
            }
            if next == u32::from('b') {
                self.at += 2;
                return Ok(BACKSPACE);
            }
            self.at += 1;
            return self.character_escape(true);
        }
        if character.is_some_and(|ch| SYNTAX.contains(ch)) {
            return Err(self.error(INVALID_CHARACTER));
        }
        if character.is_some_and(|ch| DOUBLED.contains(ch)) && self.peek_at(1) == Some(ch) {
            return Err(self.error(INVALID_OPERATION));
        }
        self.at += 1;
        Ok(ch)
    }
    /// `\q{…}` with the cursor on the backslash.
    fn string_disjunction(&mut self) -> Result<ClassValue, RegExpError> {
        self.at += 3;
        let mut value = ClassValue::default();
        let mut string = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.error("Unterminated character class")),
                Some(ch) if ch == u32::from('}') || ch == u32::from('|') => {
                    self.at += 1;
                    if let [single] = string[..] {
                        value.set.add(single, single);
                    } else {
                        value.may_strings = true;
                        value.strings.insert(string.clone());
                    }
                    string.clear();
                    if ch == u32::from('}') {
                        return Ok(self.fold(value));
                    }
                }
                Some(_) => string.push(self.set_character()?),
            }
        }
    }
}
