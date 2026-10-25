/// Component-aware filesystem glob. `*` stays in one directory, `**` crosses it.
/// Uses a bounded dynamic program rather than exponential wildcard backtracking.
pub(super) fn glob(pattern: &str, text: &str, filesystem: bool) -> bool {
    wildcard(pattern, text, filesystem, true)
}

pub(super) fn wildcard(
    pattern: &str,
    text: &str,
    filesystem: bool,
    question_wildcard: bool,
) -> bool {
    const MAX_MATCH_WORK: usize = 64 * 1024;
    // Policy matching is host work and cannot consume Wasm fuel. Fail closed on
    // adversarial pattern/input combinations instead of doing unbounded CPU work.
    if pattern.len().saturating_mul(text.len()) > MAX_MATCH_WORK {
        return false;
    }
    let p: Vec<_> = pattern.chars().collect();
    let t: Vec<_> = text.chars().collect();
    let mut row = vec![false; t.len() + 1];
    row[0] = true;
    let mut i = 0;
    while i < p.len() {
        let recursive = p[i] == '*' && p.get(i + 1) == Some(&'*');
        let mut next = vec![false; row.len()];
        if p[i] == '*' {
            next[0] = row[0];
            for j in 0..t.len() {
                next[j + 1] = row[j + 1] || (next[j] && (!filesystem || recursive || t[j] != '/'));
            }
            if recursive {
                i += 1;
            }
        } else {
            for j in 0..t.len() {
                next[j + 1] = row[j]
                    && (p[i] == t[j]
                        || question_wildcard && p[i] == '?' && (!filesystem || t[j] != '/'));
            }
        }
        row = next;
        i += 1;
    }
    row[t.len()]
}
