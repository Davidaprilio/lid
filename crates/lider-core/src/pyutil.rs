//! Small standard-library behaviours that 1.x relies on, ported exactly so the rules users already
//! wrote (category patterns, server limits, file type lists) keep matching the same names.

/// Python's `fnmatch.fnmatchcase` (and `fnmatch.fnmatch` on POSIX, which does not fold case).
///
/// `*` matches any run of characters (also `/`), `?` one character, `[seq]` / `[!seq]` a set with
/// `a-z` ranges. A `[` without a closing `]` is a literal `[`.
pub fn fnmatch(name: &str, pattern: &str) -> bool {
    let name: Vec<char> = name.chars().collect();
    let tokens = compile(pattern);
    matches(&name, &tokens)
}

#[derive(Debug)]
enum Token {
    Star,
    Any,
    Char(char),
    Set {
        negate: bool,
        items: Vec<(char, char)>,
    },
}

fn compile(pattern: &str) -> Vec<Token> {
    let p: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < p.len() {
        let c = p[i];
        i += 1;
        match c {
            '*' => {
                if !matches!(out.last(), Some(Token::Star)) {
                    out.push(Token::Star);
                }
            }
            '?' => out.push(Token::Any),
            '[' => {
                // Find the closing bracket the way fnmatch.translate does: a ']' right after '[' or '[!'
                // is part of the set.
                let mut j = i;
                if j < p.len() && p[j] == '!' {
                    j += 1;
                }
                if j < p.len() && p[j] == ']' {
                    j += 1;
                }
                while j < p.len() && p[j] != ']' {
                    j += 1;
                }
                if j >= p.len() {
                    out.push(Token::Char('['));
                    continue;
                }
                let mut body = &p[i..j];
                i = j + 1;
                let negate = body.first() == Some(&'!');
                if negate {
                    body = &body[1..];
                }
                let mut items = Vec::new();
                let mut k = 0;
                while k < body.len() {
                    if k + 2 < body.len() && body[k + 1] == '-' {
                        // A reversed range like z-a matches nothing, as in Python 3.
                        items.push((body[k], body[k + 2]));
                        k += 3;
                    } else {
                        items.push((body[k], body[k]));
                        k += 1;
                    }
                }
                out.push(Token::Set { negate, items });
            }
            c => out.push(Token::Char(c)),
        }
    }
    out
}

fn matches(name: &[char], tokens: &[Token]) -> bool {
    // Iterative matching with backtracking to the last star.
    let (mut n, mut t) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if t < tokens.len() {
            match &tokens[t] {
                Token::Star => {
                    star = Some((t, n));
                    t += 1;
                    continue;
                }
                Token::Any => {
                    n += 1;
                    t += 1;
                    continue;
                }
                Token::Char(c) if *c == name[n] => {
                    n += 1;
                    t += 1;
                    continue;
                }
                Token::Set { negate, items } => {
                    let hit = items.iter().any(|&(lo, hi)| lo <= name[n] && name[n] <= hi);
                    if hit != *negate {
                        n += 1;
                        t += 1;
                        continue;
                    }
                }
                Token::Char(_) => {}
            }
        }
        match star {
            Some((st, sn)) => {
                t = st + 1;
                n = sn + 1;
                star = Some((st, sn + 1));
            }
            None => return false,
        }
    }
    tokens[t..].iter().all(|tok| matches!(tok, Token::Star))
}

/// The extension of `os.path.splitext(path)`, including the dot (`""` when there is none).
///
/// Leading dots of the last path component do not start an extension: `.bashrc` has none.
pub fn splitext_ext(path: &str) -> &str {
    let sep = path.rfind('/').map(|i| i as isize).unwrap_or(-1);
    let Some(dot) = path.rfind('.') else {
        return "";
    };
    if (dot as isize) <= sep {
        return "";
    }
    let start = (sep + 1) as usize;
    if path[start..dot].chars().all(|c| c == '.') {
        return "";
    }
    &path[dot..]
}

/// `urllib.parse.urlsplit(url).hostname`: lower-cased host without user info, port or IPv6 brackets;
/// `None` when the URL has no network location.
pub fn url_hostname(url: &str) -> Option<String> {
    let rest = strip_scheme(url);
    let rest = rest.strip_prefix("//")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let netloc = &rest[..end];
    let host_port = netloc.rsplit_once('@').map_or(netloc, |(_, h)| h);
    let host = if let Some(inner) = host_port.strip_prefix('[') {
        inner.split_once(']').map_or(inner, |(h, _)| h)
    } else {
        host_port.split_once(':').map_or(host_port, |(h, _)| h)
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

/// `urlsplit(url).scheme`, lower-cased, or `""`.
pub fn url_scheme(url: &str) -> String {
    let rest = strip_scheme(url);
    if rest.len() == url.len() {
        String::new()
    } else {
        url[..url.len() - rest.len() - 1].to_ascii_lowercase()
    }
}

/// The URL without `scheme:` when it starts with a valid scheme (a letter, then letters, digits, `+-.`).
fn strip_scheme(url: &str) -> &str {
    let url = url.trim_start_matches(|c: char| c <= ' ');
    if let Some(colon) = url.find(':') {
        let scheme = &url[..colon];
        let mut chars = scheme.chars();
        if chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        {
            return &url[colon + 1..];
        }
    }
    url
}

/// Python truthiness of a JSON value (`bool(x)`), used by the settings migration shims.
pub fn truthy(v: &serde_json::Value) -> bool {
    use serde_json::Value;
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_like_python() {
        assert!(fnmatch("r01", "r0*"));
        assert!(fnmatch("r1", "r1*"));
        assert!(fnmatch("cdn.soundhost.com", "*.soundhost.com"));
        assert!(!fnmatch("soundhost.com", "*.soundhost.com"));
        assert!(fnmatch("a/b", "a*b")); // * also crosses '/'
        assert!(fnmatch("abc", "a?c"));
        assert!(!fnmatch("ac", "a?c"));
        assert!(fnmatch("b", "[abc]"));
        assert!(!fnmatch("d", "[abc]"));
        assert!(fnmatch("d", "[!abc]"));
        assert!(fnmatch("q", "[a-z]"));
        assert!(!fnmatch("Q", "[a-z]")); // case-sensitive on POSIX
        assert!(fnmatch("]", "[]]"));
        assert!(fnmatch("[x", "[x")); // no closing bracket: literal
        assert!(fnmatch("", "*"));
        assert!(!fnmatch("", "?"));
        assert!(fnmatch("zip", "zip"));
        assert!(!fnmatch("zipx", "zip"));
        assert!(fnmatch("aaab", "*a*b"));
        assert!(!fnmatch("aaa", "*a*b"));
    }

    #[test]
    fn splitext_like_python() {
        assert_eq!(splitext_ext("a.tar.gz"), ".gz");
        assert_eq!(splitext_ext("a"), "");
        assert_eq!(splitext_ext(".bashrc"), "");
        assert_eq!(splitext_ext("..a"), "");
        assert_eq!(splitext_ext("..a.b"), ".b");
        assert_eq!(splitext_ext("a."), ".");
        assert_eq!(splitext_ext("dir.x/file"), "");
        assert_eq!(splitext_ext("dir/.x"), "");
    }

    #[test]
    fn hostname_like_urlsplit() {
        assert_eq!(
            url_hostname("https://CDN.Example.com:8080/x").as_deref(),
            Some("cdn.example.com")
        );
        assert_eq!(
            url_hostname("ftp://user:pw@host/x").as_deref(),
            Some("host")
        );
        assert_eq!(url_hostname("http://[::1]:80/").as_deref(), Some("::1"));
        assert_eq!(url_hostname("x.zip"), None);
        assert_eq!(url_hostname(""), None);
        assert_eq!(url_hostname("https:///path"), None);
        assert_eq!(url_scheme("FTP://h/x"), "ftp");
        assert_eq!(url_scheme("x.zip"), "");
    }
}
