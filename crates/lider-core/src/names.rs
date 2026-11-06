//! File names from server answers and URLs (the helpers at the top of `engine.py` in 1.x).

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::pyutil::{splitext_ext, unquote, url_path};

static CD_EXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)filename\*\s*=\s*([\w-]*)'[^']*'([^;]+)").unwrap());
static CD_QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)filename\s*=\s*"([^"]*)""#).unwrap());
static CD_PLAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)filename\s*=\s*([^;]+)").unwrap());
static UNSAFE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[/\\\x00-\x1f]").unwrap());

/// The file name in a `Content-Disposition` header: RFC 5987 `filename*=` first (percent-decoded in
/// its charset), then `filename="…"`, then a bare `filename=…`.
pub fn parse_content_disposition(value: Option<&str>) -> Option<String> {
    let value = value.filter(|v| !v.is_empty())?;
    if let Some(m) = CD_EXT.captures(value) {
        let charset = if m[1].is_empty() { "utf-8" } else { &m[1] };
        let raw = m[2].trim().trim_matches('"');
        return Some(unquote(raw, charset));
    }
    if let Some(m) = CD_QUOTED.captures(value) {
        return Some(m[1].trim().to_string());
    }
    CD_PLAIN.captures(value).map(|m| m[1].trim().to_string())
}

/// The last part of the URL's decoded path, or "index.html".
pub fn filename_from_url(url: &str) -> String {
    let path = unquote(url_path(url), "utf-8");
    let name = path.rsplit('/').next().unwrap_or("");
    if name.is_empty() {
        "index.html".into()
    } else {
        name.into()
    }
}

/// A name that is safe as one path component: separators and control characters become `_`, outer
/// spaces and dots go, an empty name becomes "download", and the stem is cut so the name stays under
/// 240 characters.
pub fn safe_filename(name: &str) -> String {
    let replaced = UNSAFE.replace_all(name, "_");
    let trimmed = replaced.trim().trim_matches('.');
    let name = if trimmed.is_empty() {
        "download"
    } else {
        trimmed
    };
    let ext = splitext_ext(name);
    let stem: Vec<char> = name[..name.len() - ext.len()].chars().collect();
    // Python slicing: stem[:240 - len(ext)], where a negative end counts from the back.
    let limit = 240 - ext.chars().count() as isize;
    let keep = if limit >= 0 {
        (limit as usize).min(stem.len())
    } else {
        stem.len().saturating_sub(limit.unsigned_abs())
    };
    stem[..keep].iter().collect::<String>() + ext
}

/// `file.zip` -> `file (2).zip` when the name is taken, then 3, 4, …
pub fn unique_path(path: &str) -> String {
    if !Path::new(path).exists() {
        return path.to_string();
    }
    let ext = splitext_ext(path);
    let stem = &path[..path.len() - ext.len()];
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|p| !Path::new(p).exists())
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of test_engine.py `test_content_disposition`.
    #[test]
    fn content_disposition() {
        assert_eq!(
            parse_content_disposition(Some(r#"attachment; filename="a b.zip""#)).as_deref(),
            Some("a b.zip")
        );
        assert_eq!(
            parse_content_disposition(Some("attachment; filename*=UTF-8''%E2%82%AC.txt"))
                .as_deref(),
            Some("€.txt")
        );
        assert_eq!(parse_content_disposition(None), None);
        assert_eq!(parse_content_disposition(Some("")), None);
        assert_eq!(
            parse_content_disposition(Some("attachment; FILENAME= plain.txt ; x=1")).as_deref(),
            Some("plain.txt")
        );
        assert_eq!(
            parse_content_disposition(Some(
                r#"attachment; filename="fallback.txt"; filename*=iso-8859-1'en'%E9t%E9.txt"#
            ))
            .as_deref(),
            Some("été.txt")
        );
        assert_eq!(parse_content_disposition(Some("inline")), None);
    }

    /// Port of test_engine.py `test_unique_path`.
    #[test]
    fn unique_path_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f.zip").to_string_lossy().into_owned();
        assert_eq!(unique_path(&p), p);
        std::fs::write(&p, "").unwrap();
        let second = tmp.path().join("f (2).zip").to_string_lossy().into_owned();
        assert_eq!(unique_path(&p), second);
        std::fs::write(&second, "").unwrap();
        assert!(unique_path(&p).ends_with("f (3).zip"));
        let tgz = tmp.path().join("a.tar.gz").to_string_lossy().into_owned();
        std::fs::write(&tgz, "").unwrap();
        assert!(unique_path(&tgz).ends_with("a.tar (2).gz"));
    }

    #[test]
    fn url_names() {
        assert_eq!(
            filename_from_url("http://h/files/test%20file.bin"),
            "test file.bin"
        );
        assert_eq!(filename_from_url("http://h/dir/"), "index.html");
        assert_eq!(filename_from_url("http://h"), "index.html");
        assert_eq!(filename_from_url("http://h/a.zip?x=/y.zip"), "a.zip");
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_filename("a/b\\c\u{1}.txt"), "a_b_c_.txt");
        assert_eq!(safe_filename("  ..name.. "), "name");
        assert_eq!(safe_filename("..."), "download");
        assert_eq!(safe_filename(""), "download");
        let long = format!("{}.zip", "x".repeat(300));
        let s = safe_filename(&long);
        assert_eq!(s.chars().count(), 240);
        assert!(s.ends_with(".zip"));
        assert_eq!(safe_filename("Review: A07 / A08"), "Review: A07 _ A08");
    }
}
