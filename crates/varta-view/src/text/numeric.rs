//! CSS Fonts 4 numeric variants. Compact Copy state remains part of every
//! shaping/cache request; unsupported font features simply have no substitution.
use parley::{FontFeature, FontFeatures, setting::Tag};
use std::borrow::Cow;
use varta_common::view::syntax::{self, Token};

const FEATURES: [(&str, &[u8; 4]); 8] = [
    ("lining-nums", b"lnum"),
    ("oldstyle-nums", b"onum"),
    ("proportional-nums", b"pnum"),
    ("tabular-nums", b"tnum"),
    ("diagonal-fractions", b"frac"),
    ("stacked-fractions", b"afrc"),
    ("ordinal", b"ordn"),
    ("slashed-zero", b"zero"),
];
const KERN: Tag = Tag::new(b"kern");

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Numeric(u8);
impl Numeric {
    pub fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("normal") {
            return Some(Self::default());
        }
        let mut result = Self::default();
        for token in syntax::tokenize(value) {
            match token.token {
                Token::Whitespace => {}
                Token::Ident(name) => {
                    let index = FEATURES
                        .iter()
                        .position(|(word, _)| name.eq_ignore_ascii_case(word))?;
                    result.0 |= 1 << index;
                }
                _ => return None,
            }
        }
        Some(result)
    }
    pub fn features(self, tab: bool) -> FontFeatures<'static> {
        let mut features: Vec<_> = FEATURES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.0 & (1 << i) != 0)
            .map(|(_, (_, tag))| FontFeature::new(Tag::new(tag), 1))
            .collect();
        if tab {
            features.push(FontFeature::new(KERN, 1));
        }
        FontFeatures::List(Cow::Owned(features))
    }
}
