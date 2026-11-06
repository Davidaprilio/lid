//! Pick a category for a new download from its site and file type (`categories.py` in 1.x).

use crate::pyutil::{fnmatch, splitext_ext, url_hostname};
use crate::settings::{Category, Settings};

/// The file's extension without the dot, lower-cased (`""` when there is none).
pub fn extension(filename: &str) -> String {
    splitext_ext(filename)
        .trim_start_matches('.')
        .to_lowercase()
}

fn site_matches(cat: &Category, host: &str) -> bool {
    cat.sites.iter().any(|p| fnmatch(host, &p.to_lowercase()))
}

/// Site rules win over file-type rules; anything else goes to the first category ("General").
pub fn classify<'a>(settings: &'a Settings, filename: &str, url: &str) -> &'a Category {
    let host = url_hostname(url).unwrap_or_default();
    if !host.is_empty()
        && let Some(cat) = settings
            .categories
            .iter()
            .find(|c| !c.sites.is_empty() && site_matches(c, &host))
    {
        return cat;
    }
    let ext = extension(filename);
    if !ext.is_empty()
        && let Some(cat) = settings.categories.iter().find(|c| {
            c.extensions
                .iter()
                .any(|p| fnmatch(&ext, &p.to_lowercase()))
        })
    {
        return cat;
    }
    &settings.categories[0]
}

/// Whether the browser extension and the clipboard monitor take this file type.
pub fn is_auto_capture_type(settings: &Settings, filename: &str) -> bool {
    let ext = extension(filename);
    !ext.is_empty()
        && settings
            .auto_capture_types
            .split_whitespace()
            .any(|p| fnmatch(&ext, &p.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Dirs;

    fn settings() -> Settings {
        Settings::defaults(&Dirs::from_parts("/h".into(), None, None, None))
    }

    /// Port of test_manager.py `test_default_extension_lists`.
    #[test]
    fn default_extension_lists() {
        let s = settings();
        assert_eq!(classify(&s, "a.R01", "").name, "Compressed");
        assert_eq!(classify(&s, "a.pptx", "").name, "Documents");
        assert_eq!(classify(&s, "a.flac", "").name, "General");
        assert_eq!(classify(&s, "movie.mkv", "").name, "Video");
        assert!(is_auto_capture_type(&s, "setup.MSI"));
        assert!(!is_auto_capture_type(&s, "page.html"));
        assert!(!is_auto_capture_type(&s, "README"));
    }

    /// Port of test_manager.py `test_site_rule_wins`.
    #[test]
    fn site_rule_wins() {
        let mut s = settings();
        s.category_mut("Music").sites = vec!["*.soundhost.com".into()];
        assert_eq!(
            classify(&s, "x.zip", "https://cdn.soundhost.com/x.zip").name,
            "Music"
        );
        assert_eq!(
            classify(&s, "x.zip", "https://other.com/x.zip").name,
            "Compressed"
        );
    }
}
