//! Keyframe lookup and safe platform rules; source order/layers/media stay in Rust.
use crate::{Environment, Styles, prelude::*};
use alloc::format;
use varta_common::view::{
    stylesheet::{Keyframe, Keyframes},
    syntax,
};
type Rank = (usize, u32, usize);
type Selected<'a> = BTreeMap<&'a str, (Rank, &'a Keyframes)>;
impl Styles {
    /// The winning complete keyframes rule of a name (rules never merge).
    pub fn keyframes(&self, name: &str, env: &Environment) -> Option<&Keyframes> {
        let ranks = self.index.ranks(&env.media);
        self.sheets
            .iter()
            .flat_map(|(sheet, s)| {
                s.keyframes
                    .iter()
                    .enumerate()
                    .map(move |(index, k)| (*sheet, index, k))
            })
            .filter(|(_, _, k)| k.name == name && k.media.iter().all(|m| m.matches(&env.media)))
            .max_by_key(|(sheet, index, k)| (ranks.rank(*sheet, k.layer), *sheet, *index))
            .map(|(_, _, k)| k)
    }
    /// Only the admitted keyframes reach the browser's CSSOM. Ordinary guest
    /// rules still never enter the browser cascade; all computed styles are Rust's.
    pub fn animation_css(&self, env: &Environment) -> String {
        // Select every name in one traversal. Looking up each distinct name
        // separately would make a sheet of empty keyframes quadratic.
        let ranks = self.index.ranks(&env.media);
        let mut winners = Selected::new();
        for (sheet, s) in &self.sheets {
            for (index, k) in s.keyframes.iter().enumerate() {
                if !k.media.iter().all(|m| m.matches(&env.media)) {
                    continue;
                }
                let rank = (ranks.rank(*sheet, k.layer), *sheet, index);
                if winners
                    .get(k.name.as_str())
                    .is_none_or(|(old, _)| rank > *old)
                {
                    winners.insert(&k.name, (rank, k));
                }
            }
        }
        let mut out = String::new();
        for (_, (_, k)) in winners {
            out.push_str("@keyframes ");
            syntax::serialize_identifier(&k.name, &mut out);
            out.push('{');
            for Keyframe {
                offset,
                declarations,
            } in &k.frames
            {
                out.push_str(&format!("{}%{{", offset * 100.0));
                for (p, value) in declarations {
                    out.push_str(p.name());
                    out.push(':');
                    out.push_str(value);
                    out.push(';');
                }
                out.push('}');
            }
            out.push('}');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::{Styles, session};
    #[test]
    fn many_keyframe_names_emit_once_and_share_the_ordinary_work_budget() {
        let env = session::environment((1024.0, 768.0), 16.0, 16.0, 0);
        let mut styles = Styles::default();
        let mut source = alloc::string::String::new();
        for i in 0..4096 {
            source.push_str(&alloc::format!("@keyframes name{i}{{}}"));
        }
        let mut work = 0;
        styles.set(1, &source, &mut work).unwrap();
        let css = styles.animation_css(&env);
        assert_eq!(css.matches("@keyframes ").count(), 4096);
        styles
            .set(2, "@keyframes name1 {to{opacity:.5}}", &mut work)
            .unwrap();
        let css = styles.animation_css(&env);
        assert_eq!(css.matches("@keyframes ").count(), 4096);
        assert!(css.contains("@keyframes name1{100%{opacity:.5;}}"));
    }
}
