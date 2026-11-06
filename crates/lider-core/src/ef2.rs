//! The .ef2 export format and plain URL lists (Tasks → Export / Import in 1.x).

use crate::models::Download;

/// `<` / url / optional `referer: X` / `>` blocks with CRLF line endings, sorted by id.
pub fn export_ef2<'a>(downloads: impl IntoIterator<Item = &'a Download>) -> String {
    let mut items: Vec<&Download> = downloads.into_iter().collect();
    items.sort_by_key(|d| d.id);
    let mut out = String::new();
    for d in items {
        out.push_str("<\r\n");
        out.push_str(&d.url);
        out.push_str("\r\n");
        if !d.referer.is_empty() {
            out.push_str(&format!("referer: {}\r\n", d.referer));
        }
        out.push_str(">\r\n");
    }
    out
}

/// One URL per line (sorted by id) and a final newline.
pub fn export_text<'a>(downloads: impl IntoIterator<Item = &'a Download>) -> String {
    let mut items: Vec<&Download> = downloads.into_iter().collect();
    items.sort_by_key(|d| d.id);
    items
        .iter()
        .map(|d| d.url.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// (url, referer) of every block; the first line of a block is the URL, `referer:` matches in any case.
pub fn parse_ef2(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut block: Option<Vec<String>> = None;
    for line in text.split(['\n', '\r']) {
        let line = line.trim();
        if line == "<" {
            block = Some(Vec::new());
        } else if line == ">" && block.is_some() {
            let b = block.take().unwrap();
            if let Some(url) = b.first() {
                let referer = b[1..]
                    .iter()
                    .find(|l| l.to_lowercase().starts_with("referer:"))
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, v)| v.trim().to_string())
                    .unwrap_or_default();
                out.push((url.clone(), referer));
            }
        } else if let Some(b) = block.as_mut()
            && !line.is_empty()
        {
            b.push(line.to_string());
        }
    }
    out
}

/// The web addresses in a text (whitespace-separated words that are http, https or ftp URLs).
pub fn urls_in(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| crate::view::is_url(w))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of test_ui.py `test_ef2_round_trip_format`.
    #[test]
    fn ef2_round_trip_format() {
        let text =
            "<\r\nhttps://a/1.zip\r\nreferer: https://a/\r\n>\r\n<\r\nhttps://b/2.zip\r\n>\r\n";
        assert_eq!(
            parse_ef2(text),
            [
                ("https://a/1.zip".into(), "https://a/".into()),
                ("https://b/2.zip".into(), String::new())
            ]
        );
        let mut a = Download::new(2, "https://b/2.zip", "2.zip", "/d");
        let mut b = Download::new(1, "https://a/1.zip", "1.zip", "/d");
        b.referer = "https://a/".into();
        a.referer.clear();
        assert_eq!(export_ef2([&a, &b]), text);
        assert_eq!(parse_ef2(&export_ef2([&a, &b])).len(), 2);
        assert_eq!(export_text([&a, &b]), "https://a/1.zip\nhttps://b/2.zip\n");
        assert_eq!(
            parse_ef2("<\nREFERER: x\n>\n"),
            [("REFERER: x".into(), String::new())]
        );
        assert!(parse_ef2("<\n>\n").is_empty());
    }

    #[test]
    fn urls_in_text() {
        assert_eq!(
            urls_in("see https://a/1 and ftp://b/2\nnot:a/url"),
            ["https://a/1", "ftp://b/2"]
        );
    }
}
