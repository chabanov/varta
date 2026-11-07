//! ECMAScript 2025 RegExp pattern syntax (§22.2.1), parsed ahead of time.
//! Patterns without `u` follow the web-compatibility grammar of Annex B.1.2
//! (literal `]`/`{`/`}`, identity escapes, legacy octal escapes and `\c`).
//! Under v37 constructs the linear-time matcher cannot run are rejected by
//! name; v38 (`full`) admits the whole grammar: backreferences, lookaround,
//! pattern modifiers, duplicate named groups, Unicode property escapes and
//! the `v` flag's class set expressions (`classes`).
use super::RegExpError;
use super::set::{self, Set};

/// The parsed flags of a pattern.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
    pub dot_all: bool,
    pub unicode: bool,
    pub sticky: bool,
    /// V38: `d` (`hasIndices`).
    pub has_indices: bool,
    /// V38: `v` (`unicodeSets`).
    pub unicode_sets: bool,
}

impl Flags {
    /// The flags of `text`; `full` (v38) admits `d` and `v`.
    pub(crate) fn parse(text: &str, full: bool) -> Result<Self, RegExpError> {
        let mut flags = Self::default();
        let mut seen = String::new();
        for flag in text.chars() {
            if seen.contains(flag) {
                return Err(RegExpError::Syntax(format!(
                    "Invalid flags supplied to RegExp constructor '{text}'"
                )));
            }
            seen.push(flag);
            let slot = match flag {
                'g' => &mut flags.global,
                'i' => &mut flags.ignore_case,
                'm' => &mut flags.multiline,
                's' => &mut flags.dot_all,
                'u' => &mut flags.unicode,
                'y' => &mut flags.sticky,
                'd' if full => &mut flags.has_indices,
                'v' if full => &mut flags.unicode_sets,
                'd' => {
                    return Err(RegExpError::Unsupported(
                        "RegExp flag d (hasIndices match indices)".into(),
                    ));
                }
                'v' => {
                    return Err(RegExpError::Unsupported(
                        "RegExp flag v (unicodeSets class set notation)".into(),
                    ));
                }
                _ => {
                    return Err(RegExpError::Syntax(format!(
                        "Invalid flags supplied to RegExp constructor '{text}'"
                    )));
                }
            };
            *slot = true;
        }
        if flags.unicode && flags.unicode_sets {
            return Err(RegExpError::Syntax(format!(
                "Invalid flags supplied to RegExp constructor '{text}'"
            )));
        }
        Ok(flags)
    }
    /// The canonical `flags` text (`dgimsuvy` order).
    pub(crate) fn text(self) -> String {
        [
            (self.has_indices, 'd'),
            (self.global, 'g'),
            (self.ignore_case, 'i'),
            (self.multiline, 'm'),
            (self.dot_all, 's'),
            (self.unicode, 'u'),
            (self.unicode_sets, 'v'),
            (self.sticky, 'y'),
        ]
        .into_iter()
        .filter_map(|(set, flag)| set.then_some(flag))
        .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Assertion {
    Start,
    End,
    WordBoundary,
    NotWordBoundary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Node {
    Empty,
    /// One pattern character (a code unit, or a code point under `u`).
    Char(u32),
    /// A character class; `negated` (`[^…]`) applies after case closure.
    Class {
        set: Set,
        negated: bool,
    },
    /// `.`
    Dot,
    Assert(Assertion),
    /// A group; capturing groups carry their one-based index.
    Group(Box<Node>, Option<usize>),
    Concat(Vec<Node>),
    Alternation(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
    /// V38: a backreference to the first of `groups` that took part
    /// (several for a name shared by groups in different alternatives).
    Backref(Vec<usize>),
    /// V38: a lookahead or lookbehind assertion.
    Look {
        behind: bool,
        negative: bool,
        node: Box<Node>,
    },
    /// V38: a pattern modifier group `(?ims-ims:…)`: the flags it sets.
    Modified(Modifiers, Box<Node>),
}

/// The flags a modifier group turns on (`Some(true)`) or off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Modifiers {
    pub ignore_case: Option<bool>,
    pub multiline: Option<bool>,
    pub dot_all: Option<bool>,
}
impl Modifiers {
    pub(crate) fn apply(self, mut flags: Flags) -> Flags {
        flags.ignore_case = self.ignore_case.unwrap_or(flags.ignore_case);
        flags.multiline = self.multiline.unwrap_or(flags.multiline);
        flags.dot_all = self.dot_all.unwrap_or(flags.dot_all);
        flags
    }
}

impl Node {
    /// Whether the node can match without consuming input.
    pub(crate) fn nullable(&self) -> bool {
        match self {
            Self::Empty | Self::Assert(_) | Self::Backref(_) | Self::Look { .. } => true,
            Self::Char(_) | Self::Class { .. } | Self::Dot => false,
            Self::Group(node, _) | Self::Modified(_, node) => node.nullable(),
            Self::Concat(nodes) => nodes.iter().all(Self::nullable),
            Self::Alternation(nodes) => nodes.iter().any(Self::nullable),
            Self::Repeat { node, min, .. } => *min == 0 || node.nullable(),
        }
    }
    /// The capture indices inside the node, as an inclusive range.
    pub(crate) fn groups(&self) -> Option<(usize, usize)> {
        fn join(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> Option<(usize, usize)> {
            match (a, b) {
                (Some((a0, a1)), Some((b0, b1))) => Some((a0.min(b0), a1.max(b1))),
                (a, None) => a,
                (None, b) => b,
            }
        }
        match self {
            Self::Empty
            | Self::Char(_)
            | Self::Class { .. }
            | Self::Dot
            | Self::Assert(_)
            | Self::Backref(_) => None,
            Self::Group(node, index) => join(index.map(|i| (i, i)), node.groups()),
            Self::Concat(nodes) | Self::Alternation(nodes) => nodes
                .iter()
                .fold(None, |acc, node| join(acc, node.groups())),
            Self::Repeat { node, .. } | Self::Look { node, .. } | Self::Modified(_, node) => {
                node.groups()
            }
        }
    }
}

/// A parsed pattern.
pub(crate) struct Pattern {
    pub node: Node,
    /// Number of capturing groups.
    pub groups: usize,
    /// Group names by one-based index (`names[0]` is unused).
    pub names: Vec<Option<String>>,
}

const SYNTAX: &[u32] = &[
    b'^' as u32,
    b'$' as u32,
    b'\\' as u32,
    b'.' as u32,
    b'*' as u32,
    b'+' as u32,
    b'?' as u32,
    b'(' as u32,
    b')' as u32,
    b'[' as u32,
    b']' as u32,
    b'{' as u32,
    b'}' as u32,
    b'|' as u32,
];
const fn c(byte: u8) -> u32 {
    byte as u32
}
/// The largest value of a legacy octal escape (`\377`).
const MAX_OCTAL: u32 = 0o377;
const CONTROL_MODULUS: u32 = 32;
const HEX_RADIX: u32 = 16;
const OCTAL_RADIX: u32 = 8;
const DECIMAL_RADIX: u32 = 10;

/// Pattern characters: UTF-16 code units, or code points under `u`.
fn characters(pattern: &str, unicode: bool) -> Vec<u32> {
    if unicode {
        pattern.chars().map(u32::from).collect()
    } else {
        pattern.encode_utf16().map(u32::from).collect()
    }
}

pub(crate) fn parse(pattern: &str, flags: Flags, full: bool) -> Result<Pattern, RegExpError> {
    // Under `v` the pattern grammar is the Unicode one (with class sets).
    let mut grammar = flags;
    grammar.unicode = flags.unicode || flags.unicode_sets;
    let chars = characters(pattern, grammar.unicode);
    let (total, named) = prescan(&chars, flags.unicode_sets);
    let mut parser = Parser {
        chars,
        at: 0,
        flags: grammar,
        original: flags,
        full,
        named,
        total,
        names: vec![None],
        pattern,
        paths: Vec::new(),
        path: Vec::new(),
        disjunctions: 0,
        references: Vec::new(),
    };
    let mut node = parser.disjunction()?;
    if parser.at < parser.chars.len() {
        // Only an unmatched `)` stops a top-level disjunction early.
        return Err(parser.error("Unmatched ')'"));
    }
    if full {
        parser.resolve(&mut node)?;
    }
    Ok(Pattern {
        node,
        groups: parser.names.len() - 1,
        names: parser.names,
    })
}

/// CountLeftCapturingParensWithin, and whether a group name occurs (which
/// selects the `NamedCaptureGroups` grammar without `u`). Under `v` classes
/// nest.
fn prescan(chars: &[u32], sets: bool) -> (usize, bool) {
    let (mut count, mut named, mut class, mut at) = (0, false, 0usize, 0);
    while at < chars.len() {
        match chars[at] {
            ch if ch == c(b'\\') => at += 1,
            ch if ch == c(b'[') => class = if sets { class + 1 } else { 1 },
            ch if ch == c(b']') => class = class.saturating_sub(1),
            ch if ch == c(b'(') && class == 0 => {
                if chars.get(at + 1) != Some(&c(b'?')) {
                    count += 1;
                } else if chars.get(at + 2) == Some(&c(b'<'))
                    && !matches!(chars.get(at + 3), Some(&ch) if ch == c(b'=') || ch == c(b'!'))
                {
                    count += 1;
                    named = true;
                }
            }
            _ => {}
        }
        at += 1;
    }
    (count, named)
}

/// One class atom: a character or a class escape set.
enum ClassAtom {
    Char(u32),
    Set(Set),
}

pub(super) struct Parser<'a> {
    pub(super) chars: Vec<u32>,
    pub(super) at: usize,
    /// The flags in effect (pattern modifiers change `i`, `m` and `s`);
    /// `unicode` is set under `v` too.
    pub(super) flags: Flags,
    /// The pattern's flags, for diagnostics.
    original: Flags,
    /// V38: the full ECMAScript 2025 grammar.
    full: bool,
    named: bool,
    total: usize,
    names: Vec<Option<String>>,
    pattern: &'a str,
    /// V38: each named group's name and alternative path (MightBothParticipate).
    paths: Vec<(String, Vec<(usize, usize)>)>,
    /// V38: the (disjunction, alternative) path to the cursor.
    path: Vec<(usize, usize)>,
    disjunctions: usize,
    /// V38: the names of named backreferences, resolved after parsing.
    references: Vec<String>,
}

/// The marker of an unresolved named backreference: `Backref([NAMED, i])`
/// names `references[i]`.
const NAMED: usize = usize::MAX;

impl Parser<'_> {
    pub(super) fn error(&self, message: &str) -> RegExpError {
        RegExpError::Syntax(format!(
            "Invalid regular expression: /{}/{}: {message}",
            self.pattern,
            self.original.text()
        ))
    }
    /// V38: named backreferences become the groups of their name.
    fn resolve(&self, node: &mut Node) -> Result<(), RegExpError> {
        match node {
            Node::Backref(groups) if groups.first() == Some(&NAMED) => {
                let index = groups.get(1).copied().unwrap_or(0);
                let name = &self.references[index];
                let found: Vec<usize> = self
                    .names
                    .iter()
                    .enumerate()
                    .filter(|(_, group)| group.as_deref() == Some(name.as_str()))
                    .map(|(group, _)| group)
                    .collect();
                if found.is_empty() {
                    return Err(self.error("Invalid named capture referenced"));
                }
                *groups = found;
            }
            Node::Group(inner, _)
            | Node::Repeat { node: inner, .. }
            | Node::Look { node: inner, .. }
            | Node::Modified(_, inner) => self.resolve(inner)?,
            Node::Concat(nodes) | Node::Alternation(nodes) => {
                for inner in nodes {
                    self.resolve(inner)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn peek(&self) -> Option<u32> {
        self.chars.get(self.at).copied()
    }
    pub(super) fn peek_at(&self, offset: usize) -> Option<u32> {
        self.chars.get(self.at + offset).copied()
    }
    pub(super) fn eat(&mut self, ch: u8) -> bool {
        if self.peek() == Some(c(ch)) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    pub(super) fn max(&self) -> u32 {
        if self.flags.unicode {
            set::MAX_CODE_POINT
        } else {
            set::MAX_UNIT
        }
    }

    fn disjunction(&mut self) -> Result<Node, RegExpError> {
        let id = self.disjunctions;
        self.disjunctions += 1;
        self.path.push((id, 0));
        let mut alternatives = vec![self.alternative()?];
        while self.eat(b'|') {
            if let Some(last) = self.path.last_mut() {
                last.1 += 1;
            }
            alternatives.push(self.alternative()?);
        }
        self.path.pop();
        Ok(if alternatives.len() == 1 {
            alternatives.pop().unwrap_or(Node::Empty)
        } else {
            Node::Alternation(alternatives)
        })
    }
    fn alternative(&mut self) -> Result<Node, RegExpError> {
        let mut terms = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == c(b'|') || ch == c(b')') {
                break;
            }
            terms.push(self.term()?);
        }
        Ok(match terms.len() {
            0 => Node::Empty,
            1 => terms.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(terms),
        })
    }
    fn term(&mut self) -> Result<Node, RegExpError> {
        let assertion = match self.peek() {
            Some(ch) if ch == c(b'^') => Some(Assertion::Start),
            Some(ch) if ch == c(b'$') => Some(Assertion::End),
            Some(ch) if ch == c(b'\\') && self.peek_at(1) == Some(c(b'b')) => {
                Some(Assertion::WordBoundary)
            }
            Some(ch) if ch == c(b'\\') && self.peek_at(1) == Some(c(b'B')) => {
                Some(Assertion::NotWordBoundary)
            }
            Some(ch) if ch == c(b'(') && self.peek_at(1) == Some(c(b'?')) => {
                if self.full {
                    if let Some(node) = self.lookaround_full()? {
                        return Ok(node);
                    }
                } else {
                    self.lookaround()?;
                }
                None
            }
            _ => None,
        };
        if let Some(assertion) = assertion {
            self.at += if matches!(assertion, Assertion::Start | Assertion::End) {
                1
            } else {
                2
            };
            if self.quantifier()?.is_some() {
                return Err(self.error("Nothing to repeat"));
            }
            return Ok(Node::Assert(assertion));
        }
        let atom = self.atom()?;
        match self.quantifier()? {
            Some((min, max, greedy)) => Ok(Node::Repeat {
                node: Box::new(atom),
                min,
                max,
                greedy,
            }),
            None => Ok(atom),
        }
    }
    /// V38: a lookaround assertion at the cursor (`(?=`, `(?!`, `(?<=`,
    /// `(?<!`), with the Annex B quantifier of a lookahead without `u`.
    fn lookaround_full(&mut self) -> Result<Option<Node>, RegExpError> {
        let (behind, negative, header) = match (self.peek_at(2), self.peek_at(3)) {
            (Some(ch), _) if ch == c(b'=') => (false, false, 3),
            (Some(ch), _) if ch == c(b'!') => (false, true, 3),
            (Some(lt), Some(ch)) if lt == c(b'<') && ch == c(b'=') => (true, false, 4),
            (Some(lt), Some(ch)) if lt == c(b'<') && ch == c(b'!') => (true, true, 4),
            _ => return Ok(None),
        };
        self.at += header;
        let node = self.disjunction()?;
        if !self.eat(b')') {
            return Err(self.error("Unterminated group"));
        }
        let look = Node::Look {
            behind,
            negative,
            node: Box::new(node),
        };
        let start = self.at;
        match self.quantifier()? {
            None => Ok(Some(look)),
            Some(_) if behind || self.flags.unicode => {
                self.at = start;
                Err(self.error("Invalid quantifier"))
            }
            Some((min, max, greedy)) => Ok(Some(Node::Repeat {
                node: Box::new(look),
                min,
                max,
                greedy,
            })),
        }
    }
    /// Lookaround groups are not linear-time; name them precisely.
    fn lookaround(&self) -> Result<(), RegExpError> {
        let kind = match (self.peek_at(2), self.peek_at(3)) {
            (Some(ch), _) if ch == c(b'=') => "RegExp lookahead assertion (?=…)",
            (Some(ch), _) if ch == c(b'!') => "RegExp negative lookahead assertion (?!…)",
            (Some(lt), Some(ch)) if lt == c(b'<') && ch == c(b'=') => {
                "RegExp lookbehind assertion (?<=…)"
            }
            (Some(lt), Some(ch)) if lt == c(b'<') && ch == c(b'!') => {
                "RegExp negative lookbehind assertion (?<!…)"
            }
            _ => return Ok(()),
        };
        Err(RegExpError::Unsupported(kind.into()))
    }
    /// A quantifier after an atom: `(min, max, greedy)`.
    fn quantifier(&mut self) -> Result<Option<(u32, Option<u32>, bool)>, RegExpError> {
        let (min, max) = match self.peek() {
            Some(ch) if ch == c(b'*') => {
                self.at += 1;
                (0, None)
            }
            Some(ch) if ch == c(b'+') => {
                self.at += 1;
                (1, None)
            }
            Some(ch) if ch == c(b'?') => {
                self.at += 1;
                (0, Some(1))
            }
            Some(ch) if ch == c(b'{') => match self.braced() {
                Some((min, max, end)) => {
                    self.at = end;
                    (min, max)
                }
                None if self.flags.unicode => return Err(self.error("Incomplete quantifier")),
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        if max.is_some_and(|max| max < min) {
            return Err(self.error("numbers out of order in {} quantifier"));
        }
        let greedy = !self.eat(b'?');
        Ok(Some((min, max, greedy)))
    }
    /// `{n}`, `{n,}` or `{n,m}` at the cursor: `(min, max, end)`.
    fn braced(&self) -> Option<(u32, Option<u32>, usize)> {
        let mut at = self.at + 1;
        let min = self.decimal(&mut at)?;
        if self.chars.get(at) == Some(&c(b'}')) {
            return Some((min, Some(min), at + 1));
        }
        if self.chars.get(at) != Some(&c(b',')) {
            return None;
        }
        at += 1;
        if self.chars.get(at) == Some(&c(b'}')) {
            return Some((min, None, at + 1));
        }
        let max = self.decimal(&mut at)?;
        (self.chars.get(at) == Some(&c(b'}'))).then_some((min, Some(max), at + 1))
    }
    /// Decimal digits at `at`, saturating; `None` without a digit.
    pub(super) fn decimal(&self, at: &mut usize) -> Option<u32> {
        let start = *at;
        let mut value: u32 = 0;
        while let Some(digit) = self.chars.get(*at).and_then(|&ch| digit(ch, DECIMAL_RADIX)) {
            value = value.saturating_mul(DECIMAL_RADIX).saturating_add(digit);
            *at += 1;
        }
        (*at > start).then_some(value)
    }

    fn atom(&mut self) -> Result<Node, RegExpError> {
        let Some(ch) = self.peek() else {
            return Ok(Node::Empty);
        };
        match ch {
            ch if ch == c(b'.') => {
                self.at += 1;
                Ok(Node::Dot)
            }
            ch if ch == c(b'(') => self.group(),
            ch if ch == c(b'[') => self.class(),
            ch if ch == c(b'\\') => self.atom_escape(),
            ch if ch == c(b'*') || ch == c(b'+') || ch == c(b'?') => {
                Err(self.error("Nothing to repeat"))
            }
            ch if ch == c(b'{') => {
                if self.flags.unicode {
                    return Err(self.error("Lone quantifier brackets"));
                }
                if self.braced().is_some() {
                    return Err(self.error("Nothing to repeat"));
                }
                self.at += 1;
                Ok(Node::Char(ch))
            }
            ch if (ch == c(b'}') || ch == c(b']')) && self.flags.unicode => {
                Err(self.error("Lone quantifier brackets"))
            }
            ch => {
                self.at += 1;
                Ok(Node::Char(ch))
            }
        }
    }
    fn group(&mut self) -> Result<Node, RegExpError> {
        self.at += 1;
        let index = if self.eat(b'?') {
            if self.eat(b':') {
                None
            } else if self.peek() == Some(c(b'<')) {
                self.at += 1;
                let name = self.group_name()?;
                if self.full {
                    self.declare(&name)?;
                } else if self
                    .names
                    .iter()
                    .flatten()
                    .any(|existing| *existing == name)
                {
                    return Err(RegExpError::Unsupported(
                        "RegExp duplicate named capture groups".into(),
                    ));
                }
                self.names.push(Some(name));
                Some(self.names.len() - 1)
            } else if self.full {
                return self.modified();
            } else if matches!(self.peek(), Some(ch) if [c(b'i'), c(b'm'), c(b's'), c(b'-')].contains(&ch))
            {
                return Err(RegExpError::Unsupported(
                    "RegExp pattern modifiers (?ims-ims:…)".into(),
                ));
            } else {
                return Err(self.error("Invalid group"));
            }
        } else {
            self.names.push(None);
            Some(self.names.len() - 1)
        };
        let node = self.disjunction()?;
        if !self.eat(b')') {
            return Err(self.error("Unterminated group"));
        }
        Ok(Node::Group(Box::new(node), index))
    }
    /// V38: a group name may repeat only where the groups cannot both take
    /// part: in different alternatives of a disjunction (ES2025
    /// MightBothParticipate).
    fn declare(&mut self, name: &str) -> Result<(), RegExpError> {
        let path = &self.path;
        let clash = self.paths.iter().any(|(other, other_path)| {
            other == name
                && !path
                    .iter()
                    .zip(other_path)
                    .find(|(a, b)| a != b)
                    .is_some_and(|((a, x), (b, y))| a == b && x != y)
        });
        if clash {
            return Err(self.error("Duplicate capture group name"));
        }
        self.paths.push((name.to_owned(), path.clone()));
        Ok(())
    }
    /// V38: a pattern modifier group `(?ims-ims:…)` after `(?`.
    fn modified(&mut self) -> Result<Node, RegExpError> {
        let mut modifiers = Modifiers::default();
        let mut seen = String::new();
        let mut removing = false;
        loop {
            let Some(ch) = self.peek().and_then(char::from_u32) else {
                return Err(self.error("Invalid group"));
            };
            let slot = match ch {
                ':' => break,
                '-' if removing => return Err(self.error("Multiple dashes in flag group")),
                '-' => {
                    removing = true;
                    self.at += 1;
                    continue;
                }
                'i' => &mut modifiers.ignore_case,
                'm' => &mut modifiers.multiline,
                's' => &mut modifiers.dot_all,
                _ => return Err(self.error("Invalid group")),
            };
            if seen.contains(ch) {
                return Err(self.error("Repeated flag in flag group"));
            }
            seen.push(ch);
            *slot = Some(!removing);
            self.at += 1;
        }
        if removing && seen.is_empty() {
            return Err(self.error("Invalid flag group"));
        }
        self.at += 1;
        let outer = self.flags;
        self.flags = modifiers.apply(outer);
        let node = self.disjunction()?;
        self.flags = outer;
        if !self.eat(b')') {
            return Err(self.error("Unterminated group"));
        }
        Ok(Node::Modified(modifiers, Box::new(node)))
    }
    /// RegExpIdentifierName `>` after `<`.
    fn group_name(&mut self) -> Result<String, RegExpError> {
        let mut name = String::new();
        loop {
            let Some(mut ch) = self.peek() else {
                return Err(self.error("Invalid capture group name"));
            };
            self.at += 1;
            if ch == c(b'>') {
                break;
            }
            if ch == c(b'\\') && self.full {
                ch = self.name_escape()?;
            } else if ch == c(b'\\') {
                return Err(RegExpError::Unsupported(
                    "RegExp escape sequence in a capture group name".into(),
                ));
            }
            // Without `u`, an astral name character is a surrogate pair.
            if is_high(ch)
                && let Some(low) = self.peek().filter(|&low| is_low(low))
            {
                self.at += 1;
                ch = combine(ch, low);
            }
            let Some(character) = char::from_u32(ch) else {
                return Err(self.error("Invalid capture group name"));
            };
            let valid = if name.is_empty() {
                character == '$' || character == '_' || character.is_alphabetic()
            } else {
                character == '$'
                    || character == '_'
                    || character == '\u{200C}'
                    || character == '\u{200D}'
                    || character.is_alphanumeric()
            };
            if !valid {
                return Err(self.error("Invalid capture group name"));
            }
            name.push(character);
        }
        if name.is_empty() {
            return Err(self.error("Invalid capture group name"));
        }
        Ok(name)
    }

    fn atom_escape(&mut self) -> Result<Node, RegExpError> {
        let Some(next) = self.peek_at(1) else {
            return Err(self.error("\\ at end of pattern"));
        };
        if self.full
            && let Some(node) = self.atom_escape_full(next)?
        {
            return Ok(node);
        }
        if let Some(set) = self.class_escape(next)? {
            self.at += 2;
            return Ok(Node::Class {
                set,
                negated: false,
            });
        }
        if next == c(b'k') && (self.flags.unicode || self.named) {
            self.at += 2;
            if !self.eat(b'<') {
                return Err(self.error("Invalid named reference"));
            }
            self.group_name()?;
            return Err(RegExpError::Unsupported(
                "RegExp named backreference \\k<name> (backreferences need backtracking; v37 matches in linear time)".into(),
            ));
        }
        if (c(b'1')..=c(b'9')).contains(&next) {
            let mut at = self.at + 1;
            let number = self.decimal(&mut at).unwrap_or(0);
            if number as usize <= self.total {
                return Err(RegExpError::Unsupported(format!(
                    "RegExp backreference \\{number} (backreferences need backtracking; v37 matches in linear time)"
                )));
            }
            if self.flags.unicode {
                return Err(self.error("Invalid escape"));
            }
        }
        self.at += 1;
        Ok(Node::Char(self.character_escape(false)?))
    }
    /// V38 escapes of an atom with the cursor on the backslash:
    /// backreferences, named backreferences and Unicode property escapes
    /// (whose strings under `v` become alternatives).
    fn atom_escape_full(&mut self, next: u32) -> Result<Option<Node>, RegExpError> {
        if self.flags.unicode && (next == c(b'p') || next == c(b'P')) {
            let value = self.property()?;
            return Ok(Some(value.into_node()));
        }
        if next == c(b'k') && (self.flags.unicode || self.named) {
            self.at += 2;
            if !self.eat(b'<') {
                return Err(self.error("Invalid named reference"));
            }
            let name = self.group_name()?;
            self.references.push(name);
            return Ok(Some(Node::Backref(vec![NAMED, self.references.len() - 1])));
        }
        if (c(b'1')..=c(b'9')).contains(&next) {
            let mut at = self.at + 1;
            let number = self.decimal(&mut at).unwrap_or(0);
            if number as usize <= self.total {
                self.at = at;
                return Ok(Some(Node::Backref(vec![number as usize])));
            }
        }
        Ok(None)
    }
    /// The code point of a `\u` escape in a group name (after the
    /// backslash): `\uXXXX` (a surrogate pair of two escapes is one code
    /// point) or `\u{…}`.
    fn name_escape(&mut self) -> Result<u32, RegExpError> {
        const HEX4: usize = 4;
        let invalid = |parser: &Self| parser.error("Invalid capture group name");
        if self.peek() != Some(c(b'u')) {
            return Err(invalid(self));
        }
        if self.peek_at(1) == Some(c(b'{')) {
            let mut at = self.at + 2;
            let mut value: u32 = 0;
            while let Some(d) = self.chars.get(at).and_then(|&ch| digit(ch, HEX_RADIX)) {
                value = value.saturating_mul(HEX_RADIX).saturating_add(d);
                at += 1;
            }
            if at == self.at + 2
                || self.chars.get(at) != Some(&c(b'}'))
                || value > set::MAX_CODE_POINT
            {
                return Err(invalid(self));
            }
            self.at = at + 1;
            return Ok(value);
        }
        let Some(value) = self.hex(1, HEX4) else {
            return Err(invalid(self));
        };
        self.at += 1 + HEX4;
        if is_high(value)
            && self.peek() == Some(c(b'\\'))
            && self.peek_at(1) == Some(c(b'u'))
            && let Some(low) = self.hex(2, HEX4).filter(|&low| is_low(low))
        {
            self.at += 2 + HEX4;
            return Ok(combine(value, low));
        }
        Ok(value)
    }
    /// `\d \D \s \S \w \W` (and `\p`, `\P` under `u`) after the backslash.
    pub(super) fn class_escape(&self, next: u32) -> Result<Option<Set>, RegExpError> {
        let max = self.max();
        let word = || {
            let mut word = set::word();
            if self.flags.unicode && self.flags.ignore_case {
                for extra in set::EXTRA_WORD {
                    word.add(extra, extra);
                }
            }
            word
        };
        let set = match char::from_u32(next) {
            Some('d') => set::digits(),
            Some('D') => set::digits().complement(max),
            Some('s') => set::space(),
            Some('S') => set::space().complement(max),
            Some('w') => word(),
            Some('W') => word().complement(max),
            Some('p' | 'P') if self.flags.unicode => {
                return Err(RegExpError::Unsupported(
                    "RegExp Unicode property escape \\p{…}".into(),
                ));
            }
            _ => return Ok(None),
        };
        Ok(Some(set))
    }
    /// A CharacterEscape (or Annex B identity/octal escape) with the cursor
    /// on the character after the backslash.
    pub(super) fn character_escape(&mut self, class: bool) -> Result<u32, RegExpError> {
        let Some(ch) = self.peek() else {
            return Err(self.error("\\ at end of pattern"));
        };
        let unicode = self.flags.unicode;
        let simple = match char::from_u32(ch) {
            Some('f') => Some(0x0C),
            Some('n') => Some(0x0A),
            Some('r') => Some(0x0D),
            Some('t') => Some(0x09),
            Some('v') => Some(0x0B),
            Some('b') if class => Some(0x08),
            Some('-') if class && unicode => Some(ch),
            _ => None,
        };
        if let Some(value) = simple {
            self.at += 1;
            return Ok(value);
        }
        if ch == c(b'c') {
            return self.control(class);
        }
        if ch == c(b'x') {
            if let Some(value) = self.hex(1, 2) {
                self.at += 3;
                return Ok(value);
            }
            if unicode {
                return Err(self.error("Invalid escape"));
            }
            self.at += 1;
            return Ok(ch);
        }
        if ch == c(b'u') {
            return self.unicode_escape();
        }
        if ch == c(b'0')
            && !self
                .peek_at(1)
                .is_some_and(|d| digit(d, DECIMAL_RADIX).is_some())
        {
            self.at += 1;
            return Ok(0);
        }
        if digit(ch, DECIMAL_RADIX).is_some() {
            if unicode {
                return Err(self.error(if class {
                    "Invalid class escape"
                } else {
                    "Invalid decimal escape"
                }));
            }
            return Ok(self.legacy_octal());
        }
        if unicode {
            if SYNTAX.contains(&ch) || ch == c(b'/') {
                self.at += 1;
                return Ok(ch);
            }
            return Err(self.error("Invalid escape"));
        }
        if ch == c(b'k') && self.named {
            return Err(self.error("Invalid named reference"));
        }
        self.at += 1;
        Ok(ch)
    }
    /// `\c` with the cursor on `c`.
    fn control(&mut self, class: bool) -> Result<u32, RegExpError> {
        if let Some(letter) = self.peek_at(1)
            && char::from_u32(letter).is_some_and(|l| l.is_ascii_alphabetic())
        {
            self.at += 2;
            return Ok(letter % CONTROL_MODULUS);
        }
        if self.flags.unicode {
            return Err(self.error("Invalid unicode escape"));
        }
        // Annex B ClassControlLetter: a digit or `_` inside a class.
        if class
            && let Some(letter) = self.peek_at(1)
            && (digit(letter, DECIMAL_RADIX).is_some() || letter == c(b'_'))
        {
            self.at += 2;
            return Ok(letter % CONTROL_MODULUS);
        }
        // `\c` otherwise matches the backslash; `c` is read next.
        Ok(c(b'\\'))
    }
    /// LegacyOctalEscapeSequence or an identity escape of `8`/`9`.
    fn legacy_octal(&mut self) -> u32 {
        let Some(first) = self.peek().and_then(|ch| digit(ch, OCTAL_RADIX)) else {
            let ch = self.peek().unwrap_or(0);
            self.at += 1;
            return ch;
        };
        self.at += 1;
        let mut value = first;
        for _ in 0..2 {
            match self.peek().and_then(|ch| digit(ch, OCTAL_RADIX)) {
                Some(next) if value * OCTAL_RADIX + next <= MAX_OCTAL => {
                    value = value * OCTAL_RADIX + next;
                    self.at += 1;
                }
                _ => break,
            }
        }
        value
    }
    /// `count` hex digits starting `offset` characters after the cursor.
    fn hex(&self, offset: usize, count: usize) -> Option<u32> {
        (0..count).try_fold(0, |value, index| {
            Some(value * HEX_RADIX + digit(self.peek_at(offset + index)?, HEX_RADIX)?)
        })
    }
    /// `\u` escapes with the cursor on `u`.
    fn unicode_escape(&mut self) -> Result<u32, RegExpError> {
        const HEX4: usize = 4;
        if self.flags.unicode {
            if self.peek_at(1) == Some(c(b'{')) {
                let mut at = self.at + 2;
                let mut value: u32 = 0;
                while let Some(d) = self.chars.get(at).and_then(|&ch| digit(ch, HEX_RADIX)) {
                    value = value.saturating_mul(HEX_RADIX).saturating_add(d);
                    at += 1;
                }
                if at == self.at + 2
                    || self.chars.get(at) != Some(&c(b'}'))
                    || value > set::MAX_CODE_POINT
                {
                    return Err(self.error("Invalid Unicode escape"));
                }
                self.at = at + 1;
                return Ok(value);
            }
            let Some(value) = self.hex(1, HEX4) else {
                return Err(self.error("Invalid Unicode escape"));
            };
            self.at += 1 + HEX4;
            // `\uLead\uTrail` is one code point under `u`.
            if is_high(value)
                && self.peek() == Some(c(b'\\'))
                && self.peek_at(1) == Some(c(b'u'))
                && let Some(low) = self.hex(2, HEX4).filter(|&low| is_low(low))
            {
                self.at += 2 + HEX4;
                return Ok(combine(value, low));
            }
            return Ok(value);
        }
        match self.hex(1, HEX4) {
            Some(value) => {
                self.at += 1 + HEX4;
                Ok(value)
            }
            None => {
                self.at += 1;
                Ok(c(b'u'))
            }
        }
    }

    fn class(&mut self) -> Result<Node, RegExpError> {
        if self.full && self.flags.unicode_sets {
            return self.class_set();
        }
        self.at += 1;
        let negated = self.eat(b'^');
        let mut members = Set::default();
        loop {
            match self.peek() {
                None => return Err(self.error("Unterminated character class")),
                Some(ch) if ch == c(b']') => {
                    self.at += 1;
                    break;
                }
                _ => {}
            }
            let first = self.class_atom()?;
            let range =
                self.peek() == Some(c(b'-')) && self.peek_at(1).is_some_and(|ch| ch != c(b']'));
            if !range {
                add(&mut members, first);
                continue;
            }
            self.at += 1;
            let second = self.class_atom()?;
            match (first, second) {
                (ClassAtom::Char(low), ClassAtom::Char(high)) => {
                    if low > high {
                        return Err(self.error("Range out of order in character class"));
                    }
                    members.add(low, high);
                }
                (first, second) => {
                    if self.flags.unicode {
                        return Err(self.error("Invalid character class"));
                    }
                    add(&mut members, first);
                    members.add(c(b'-'), c(b'-'));
                    add(&mut members, second);
                }
            }
        }
        Ok(Node::Class {
            set: members,
            negated,
        })
    }
    fn class_atom(&mut self) -> Result<ClassAtom, RegExpError> {
        let Some(ch) = self.peek() else {
            return Err(self.error("Unterminated character class"));
        };
        if ch != c(b'\\') {
            self.at += 1;
            return Ok(ClassAtom::Char(ch));
        }
        let Some(next) = self.peek_at(1) else {
            return Err(self.error("\\ at end of pattern"));
        };
        if self.full && self.flags.unicode && (next == c(b'p') || next == c(b'P')) {
            return Ok(ClassAtom::Set(self.property()?.set));
        }
        if let Some(set) = self.class_escape(next)? {
            self.at += 2;
            return Ok(ClassAtom::Set(set));
        }
        if next == c(b'B') && self.flags.unicode {
            return Err(self.error("Invalid class escape"));
        }
        if next == c(b'c')
            && !self.flags.unicode
            && !self.peek_at(2).is_some_and(|letter| {
                char::from_u32(letter).is_some_and(|l| l.is_ascii_alphanumeric())
                    || letter == c(b'_')
            })
        {
            // ClassAtomNoDash `\` [lookahead = c]: the backslash itself.
            self.at += 1;
            return Ok(ClassAtom::Char(c(b'\\')));
        }
        self.at += 1;
        Ok(ClassAtom::Char(self.character_escape(true)?))
    }
}

fn add(set: &mut Set, atom: ClassAtom) {
    match atom {
        ClassAtom::Char(ch) => set.add(ch, ch),
        ClassAtom::Set(other) => set.union(&other),
    }
}
fn digit(ch: u32, radix: u32) -> Option<u32> {
    char::from_u32(ch)?.to_digit(radix)
}
pub(crate) fn is_high(unit: u32) -> bool {
    (0xD800..=0xDBFF).contains(&unit)
}
pub(crate) fn is_low(unit: u32) -> bool {
    (0xDC00..=0xDFFF).contains(&unit)
}
pub(crate) fn combine(high: u32, low: u32) -> u32 {
    0x1_0000 + ((high - 0xD800) << 10) + (low - 0xDC00)
}
