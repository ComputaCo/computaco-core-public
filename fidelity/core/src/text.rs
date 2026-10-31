//! String utilities the comparisons lean on: normalization, similarity and the `*` glob.

/// Collapse whitespace, trim, drop a GTK/Qt mnemonic underscore or ampersand before a
/// letter, drop a trailing ellipsis and a trailing label colon.
pub fn normalize_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut pending_space = false;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        if (c == '_' || c == '&')
            && i + 1 < chars.len()
            && chars[i + 1].is_alphanumeric()
            && (i == 0 || !chars[i - 1].is_alphanumeric())
        {
            i += 1;
            continue;
        }
        if c == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
            // `&&` is a literal ampersand in wx/Qt labels.
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push('&');
            i += 2;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
        i += 1;
    }
    loop {
        let before = out.len();
        if let Some(t) = out.strip_suffix('…') {
            out = t.trim_end().to_owned();
        }
        if let Some(t) = out.strip_suffix("...") {
            out = t.trim_end().to_owned();
        }
        if let Some(t) = out.strip_suffix(':') {
            out = t.trim_end().to_owned();
        }
        if out.len() == before {
            break;
        }
    }
    out
}

/// Levenshtein distance over chars.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// 1 for equal strings, 0 for nothing in common; case-insensitive.
pub fn similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let la = a.to_lowercase();
    let lb = b.to_lowercase();
    if la == lb {
        return 1.0;
    }
    let n = la.chars().count().max(lb.chars().count());
    if n == 0 {
        return 1.0;
    }
    1.0 - levenshtein(&la, &lb) as f32 / n as f32
}

/// `*` matches any run of characters; everything else is literal and case-sensitive.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(normalize_name("_File"), "File");
        assert_eq!(normalize_name("&Save As..."), "Save As");
        assert_eq!(normalize_name("Open  Recent…"), "Open Recent");
        assert_eq!(normalize_name("Name:"), "Name");
        assert_eq!(normalize_name("Cut && Paste"), "Cut & Paste");
        assert_eq!(normalize_name("snake_case_name"), "snake_case_name");
    }
    #[test]
    fn distances() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert!((similarity("Save", "save") - 1.0).abs() < 1e-6);
        assert!(similarity("Save", "Open") < 0.5);
    }
    #[test]
    fn globs() {
        assert!(glob_matches("window:*:close", "window:12:close"));
        assert!(glob_matches("*", ""));
        assert!(!glob_matches("a*c", "ab"));
        assert!(glob_matches("*.kicad_sch", "x/y.kicad_sch"));
    }
}
