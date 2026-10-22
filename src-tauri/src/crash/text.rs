//! Small text helpers mirroring PCL's string extension methods.
//! Rust's `regex` has no lookaround, so ported patterns capture group 1 instead.

use regex::Regex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

thread_local! {
    static CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
}

pub fn rx(pat: &str) -> Regex {
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(pat.to_string())
            .or_insert_with(|| Regex::new(pat).unwrap_or_else(|e| panic!("bad crash regex {pat:?}: {e}")))
            .clone()
    })
}

/// First match. Returns capture group 1 when the pattern has one, else the whole match.
pub fn seek(text: &str, pat: &str) -> Option<String> {
    let caps = rx(pat).captures(text)?;
    caps.get(1).or_else(|| caps.get(0)).map(|m| m.as_str().to_string())
}

/// All matches, same group rule as `seek`.
pub fn search(text: &str, pat: &str) -> Vec<String> {
    rx(pat).captures_iter(text).filter_map(|c| c.get(1).or_else(|| c.get(0)).map(|m| m.as_str().to_string())).collect()
}

pub fn check(text: &str, pat: &str) -> bool {
    rx(pat).is_match(text)
}

/// Text between the first `a` and the next `b`. Empty if `a` is missing;
/// runs to the end if `b` is missing.
pub fn between(text: &str, a: &str, b: &str) -> String {
    let Some(i) = text.find(a) else { return String::new() };
    let rest = &text[i + a.len()..];
    match rest.find(b) {
        Some(j) => rest[..j].to_string(),
        None => rest.to_string(),
    }
}

pub fn before_first<'a>(text: &'a str, s: &str) -> &'a str {
    match text.find(s) {
        Some(i) => &text[..i],
        None => text,
    }
}

#[allow(dead_code)]
pub fn after_last<'a>(text: &'a str, s: &str) -> &'a str {
    match text.rfind(s) {
        Some(i) => &text[i + s.len()..],
        None => text,
    }
}

pub fn contains_ci(text: &str, s: &str) -> bool {
    text.to_lowercase().contains(&s.to_lowercase())
}

pub fn dedup(v: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    v.into_iter().filter(|s| seen.insert(s.clone())).collect()
}

pub fn trim_crlf_space(s: &str) -> String {
    s.trim_matches(|c| c == '\r' || c == '\n' || c == ' ').to_string()
}

/// Port of PCL `GetHeadTailLines`: keep the first `head` and last `tail` distinct
/// lines, drop empty lines, join with `\n`.
pub fn head_tail_lines(raw: &[String], head: usize, tail: usize) -> String {
    if raw.len() <= head + tail {
        let mut seen = HashSet::new();
        let mut out = String::new();
        for l in raw.iter().filter(|l| seen.insert(l.as_str())) {
            out.push_str(l);
            out.push('\n');
        }
        return out.trim_end_matches('\n').to_string();
    }
    let mut lines: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut real_head = 0usize;
    let mut viewed = raw.len();
    for (i, l) in raw.iter().enumerate() {
        if seen.contains(l.as_str()) {
            continue;
        }
        real_head += 1;
        lines.push(l);
        seen.insert(l);
        if real_head >= head {
            viewed = i;
            break;
        }
    }
    let mut real_tail = 0usize;
    for i in (viewed.min(raw.len())..raw.len()).rev() {
        let l = raw[i].as_str();
        if seen.contains(l) {
            continue;
        }
        real_tail += 1;
        lines.insert(real_head.min(lines.len()), l);
        seen.insert(l);
        if real_tail >= tail {
            break;
        }
    }
    let mut out = String::new();
    for l in lines {
        if l.is_empty() {
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(seek("a=12;", r"a=(\d+)").as_deref(), Some("12"));
        assert_eq!(seek("abc", r"b+").as_deref(), Some("b"));
        assert_eq!(between("x[1]y[2]", "[", "]"), "1");
        assert_eq!(before_first("abc|def", "|"), "abc");
        assert_eq!(after_last("a¨b¨c", "¨"), "c");
    }

    #[test]
    fn head_tail_dedups_and_limits() {
        let raw: Vec<String> = (0..10).map(|i| format!("l{i}")).collect();
        let out = head_tail_lines(&raw, 2, 2);
        assert_eq!(out, "l0\nl1\nl8\nl9\n");
        let dup = vec!["a".to_string(), "a".to_string(), "b".to_string()];
        assert_eq!(head_tail_lines(&dup, 10, 10), "a\nb");
    }
}
