//! Links of a web page (`LinkParser`/`extract_links` in `add.py` and the rows of `LinksDialog` in 1.x).

use std::collections::HashSet;

use ego_tree::iter::Edge;
use scraper::{Html, Node};
use serde::{Deserialize, Serialize};

use crate::categories;
use crate::names::{filename_from_url, safe_filename};
use crate::pyutil::url_scheme;
use crate::settings::Settings;
use crate::view::category_icon;

pub const HTML_EXT: [&str; 9] = [
    "", "htm", "html", "php", "asp", "aspx", "jsp", "shtml", "cgi",
];
pub const IMAGE_EXT: [&str; 8] = ["jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "ico"];

/// (url, text, kind): kind is "a" for links, or the tag of an embedded file (img, source, video, audio, embed).
pub type Link = (String, String, String);

fn join(base: &str, href: &str) -> String {
    match url::Url::parse(base).and_then(|b| b.join(href.trim())) {
        Ok(u) => u.to_string(),
        Err(_) => href.trim().to_string(),
    }
}

/// Links and embedded files of the page in the order 1.x finds them: an embedded file when its tag
/// opens, a link when it closes (with its text, whitespace collapsed). Only http, https and ftp.
pub fn extract_links(html: &str, base: &str) -> Vec<Link> {
    let doc = Html::parse_document(html);
    let mut out = Vec::new();
    let mut open: Option<(String, String)> = None; // href, text
    for edge in doc.tree.root().traverse() {
        match edge {
            Edge::Open(node) => match node.value() {
                Node::Element(e) => {
                    let tag = e.name();
                    if tag == "a" {
                        if let Some(href) = e.attr("href").filter(|h| !h.is_empty()) {
                            open = Some((join(base, href), String::new()));
                        }
                    } else if ["img", "source", "video", "audio", "embed"].contains(&tag)
                        && let Some(src) = e.attr("src").filter(|s| !s.is_empty())
                    {
                        out.push((
                            join(base, src),
                            e.attr("alt").unwrap_or("").to_string(),
                            tag.to_string(),
                        ));
                    }
                }
                Node::Text(t) => {
                    if let Some((_, text)) = open.as_mut() {
                        text.push_str(t);
                    }
                }
                _ => {}
            },
            Edge::Close(node) => {
                if let Node::Element(e) = node.value()
                    && e.name() == "a"
                    && let Some((href, text)) = open.take()
                {
                    out.push((
                        href,
                        text.split_whitespace().collect::<Vec<_>>().join(" "),
                        "a".into(),
                    ));
                }
            }
        }
    }
    out.into_iter()
        .filter(|(u, _, _)| ["http", "https", "ftp"].contains(&url_scheme(u).as_str()))
        .collect()
}

/// The list's filters.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Filters {
    pub hide_html: bool,
    pub hide_images: bool,
    pub hide_repeated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LinkRow {
    pub url: String,
    pub text: String,
    pub name: String,
    /// The upper-case extension.
    pub kind: String,
    /// The URL without its scheme.
    pub address: String,
    pub icon: String,
    pub category: String,
    /// Checked at first: the name is one of the auto-capture file types.
    pub checked: bool,
}

/// The rows the dialog lists for these filters.
pub fn rows(settings: &Settings, links: &[Link], f: Filters) -> Vec<LinkRow> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (url, text, kind) in links {
        let name = safe_filename(&filename_from_url(url));
        let ext = categories::extension(&name);
        if f.hide_html && kind == "a" && HTML_EXT.contains(&ext.as_str()) {
            continue;
        }
        if f.hide_images && kind == "img" {
            continue;
        }
        if f.hide_repeated && seen.contains(url) {
            continue;
        }
        seen.insert(url.clone());
        let cat = categories::classify(settings, &name, url);
        out.push(LinkRow {
            url: url.clone(),
            text: text.clone(),
            kind: ext.to_uppercase(),
            address: url
                .split_once("://")
                .map_or(url.as_str(), |(_, r)| r)
                .to_string(),
            icon: category_icon(&cat.name),
            category: cat.name.clone(),
            checked: categories::is_auto_capture_type(settings, &name),
            name,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Dirs;

    const PAGE: &str = r#"<html><body>
      <a href="/files/setup.exe">  Setup
         for Linux </a>
      <a href="page.html">Next page</a>
      <a href="mailto:x@y">mail</a>
      <a href="https://cdn.example.com/a.zip"><img src="thumb.png" alt="Thumb">Archive</a>
      <video src="clip.mp4"></video>
      <a href="ftp://ftp.example.com/b.iso">ISO</a>
      <a href="/files/setup.exe">again</a>
      <a href="">empty</a>
    </body></html>"#;

    #[test]
    fn extracts_like_1x() {
        let l = extract_links(PAGE, "https://example.com/dir/index.html");
        let urls: Vec<&str> = l.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://example.com/files/setup.exe",
                "https://example.com/dir/page.html",
                "https://example.com/dir/thumb.png",
                "https://cdn.example.com/a.zip",
                "https://example.com/dir/clip.mp4",
                "ftp://ftp.example.com/b.iso",
                "https://example.com/files/setup.exe",
            ]
        );
        assert_eq!(l[0].1, "Setup for Linux");
        assert_eq!((l[2].1.as_str(), l[2].2.as_str()), ("Thumb", "img"));
        assert_eq!(l[3].1, "Archive");
        assert_eq!(l[4].2, "video");
    }

    #[test]
    fn rows_and_filters() {
        let s = Settings::defaults(&Dirs::from_parts("/h".into(), None, None, None));
        let links = extract_links(PAGE, "https://example.com/dir/");
        let all = Filters {
            hide_html: false,
            hide_images: false,
            hide_repeated: false,
        };
        assert_eq!(rows(&s, &links, all).len(), 7);
        let default = Filters {
            hide_html: true,
            hide_images: false,
            hide_repeated: true,
        };
        let r = rows(&s, &links, default);
        let names: Vec<&str> = r.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            ["setup.exe", "thumb.png", "a.zip", "clip.mp4", "b.iso"]
        );
        assert_eq!(
            (r[0].kind.as_str(), r[0].checked, r[0].category.as_str()),
            ("EXE", true, "Programs")
        );
        assert!(!r[1].checked);
        assert_eq!(r[2].address, "cdn.example.com/a.zip");
        let no_img = Filters {
            hide_images: true,
            ..default
        };
        assert_eq!(rows(&s, &links, no_img).len(), 4);
    }
}
