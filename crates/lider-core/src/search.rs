//! Download-list search (`search.py` in 1.x).
//!
//! Two modes share one predicate:
//! * Filter mode (the SearchBar's "Find" button): the visible list only keeps matching downloads.
//! * Find Next (Ctrl+F / F3): a cursor walks the visible rows from the last hit, wraps once to the
//!   top, and selects the first match.

use caseless::default_case_fold_str;

use crate::models::Download;

/// Bits of the saved `search_flags` setting.
pub mod flags {
    pub const MATCH_CASE: i64 = 0x01;
    pub const WHOLE_STRING: i64 = 0x02;
    pub const TARGET_NAME: i64 = 0x04;
    pub const TARGET_DESCRIPTION: i64 = 0x08;
    /// Site name / download link / parent web page / referer.
    pub const TARGET_LINKS: i64 = 0x10;
    pub const TARGETS: i64 = TARGET_NAME | TARGET_DESCRIPTION | TARGET_LINKS;
    pub const DEFAULT: i64 = TARGET_NAME;
}

pub const NO_TARGET: &str = "The checkbox for at least one of the search targets must be enabled";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{NO_TARGET}")]
pub struct NoTargetError;

/// The fields search looks at.
pub trait Searchable {
    fn filename(&self) -> &str;
    fn description(&self) -> &str;
    fn url(&self) -> &str;
    fn parent_page(&self) -> &str;
    fn referer(&self) -> &str;
}

impl<T: Searchable + ?Sized> Searchable for &T {
    fn filename(&self) -> &str {
        (**self).filename()
    }
    fn description(&self) -> &str {
        (**self).description()
    }
    fn url(&self) -> &str {
        (**self).url()
    }
    fn parent_page(&self) -> &str {
        (**self).parent_page()
    }
    fn referer(&self) -> &str {
        (**self).referer()
    }
}

impl Searchable for Download {
    fn filename(&self) -> &str {
        &self.filename
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn url(&self) -> &str {
        &self.url
    }
    fn parent_page(&self) -> &str {
        &self.parent_page
    }
    fn referer(&self) -> &str {
        &self.referer
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSettings {
    pub query: String,
    pub flags: i64,
    /// While set, the download list only shows matches; closing the SearchBar clears it.
    pub active: bool,
}

impl Default for SearchSettings {
    fn default() -> Self {
        SearchSettings {
            query: String::new(),
            flags: flags::DEFAULT,
            active: false,
        }
    }
}

impl SearchSettings {
    pub fn new(query: &str, flags: i64) -> SearchSettings {
        SearchSettings {
            query: query.into(),
            flags,
            active: false,
        }
    }

    pub fn match_case(&self) -> bool {
        self.flags & flags::MATCH_CASE != 0
    }

    pub fn whole_string(&self) -> bool {
        self.flags & flags::WHOLE_STRING != 0
    }

    /// Refuse to search with no target, and turn "File name" back on.
    pub fn validate(&mut self) -> Result<(), NoTargetError> {
        if self.flags & flags::TARGETS == 0 {
            self.flags |= flags::TARGET_NAME;
            return Err(NoTargetError);
        }
        Ok(())
    }
}

/// The compiled match predicate of a [`SearchSettings`].
pub struct Matcher {
    flags: i64,
    needle: String,
}

impl Matcher {
    pub fn new(settings: &SearchSettings) -> Matcher {
        let needle = if settings.match_case() {
            settings.query.clone()
        } else {
            default_case_fold_str(&settings.query)
        };
        Matcher {
            flags: settings.flags,
            needle,
        }
    }

    fn hit(&self, value: &str) -> bool {
        if value.is_empty() {
            return false;
        }
        let folded;
        let value = if self.flags & flags::MATCH_CASE != 0 {
            value
        } else {
            // Full case folding, so "ß" matches "SS".
            folded = default_case_fold_str(value);
            &folded
        };
        if self.flags & flags::WHOLE_STRING != 0 {
            // "Match whole string only" is full-field equality, not word matching.
            value == self.needle
        } else {
            value.contains(&self.needle)
        }
    }

    pub fn matches(&self, d: &impl Searchable) -> bool {
        // An empty query matches everything.
        if self.needle.is_empty() {
            return true;
        }
        let f = self.flags;
        (f & flags::TARGET_NAME != 0 && self.hit(d.filename()))
            || (f & flags::TARGET_DESCRIPTION != 0 && self.hit(d.description()))
            || (f & flags::TARGET_LINKS != 0
                && (self.hit(d.url()) || self.hit(d.parent_page()) || self.hit(d.referer())))
    }
}

/// SearchBar mode: the rows to show.
pub fn filter<'a, T: Searchable>(
    rows: &'a [T],
    settings: &mut SearchSettings,
) -> Result<Vec<&'a T>, NoTargetError> {
    if !settings.active {
        return Ok(rows.iter().collect());
    }
    settings.validate()?;
    let m = Matcher::new(settings);
    Ok(rows.iter().filter(|d| m.matches(*d)).collect())
}

/// Ctrl+F / F3 state: a resume cursor over the currently visible rows.
#[derive(Debug, Clone, Default)]
pub struct FindNext {
    pub settings: SearchSettings,
    pub cursor: usize,
    last_query: String,
}

impl FindNext {
    pub fn new(settings: SearchSettings) -> FindNext {
        FindNext {
            settings,
            cursor: 0,
            last_query: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// The index of the next matching row; `Ok(None)` means "No such files found".
    pub fn next<T: Searchable>(&mut self, rows: &[T]) -> Result<Option<usize>, NoTargetError> {
        if self.settings.query.is_empty() {
            return Ok(None);
        }
        self.settings.validate()?;
        if self.settings.query != self.last_query {
            self.last_query = self.settings.query.clone();
            self.cursor = 0;
        }
        let m = Matcher::new(&self.settings);
        let start = self.cursor.min(rows.len());
        // From the cursor to the end, then wrap to the top once.
        let found = (start..rows.len())
            .chain(0..start)
            .find(|&i| m.matches(&rows[i]));
        self.cursor = found.map_or(0, |i| i + 1);
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    //! Port of tests/test_search.py.
    use super::flags::*;
    use super::*;

    fn dl(
        id: i64,
        filename: &str,
        description: &str,
        url: &str,
        parent: &str,
        referer: &str,
    ) -> Download {
        let mut d = Download::new(id, url, filename, "/tmp");
        d.description = description.into();
        d.parent_page = parent.into();
        d.referer = referer.into();
        d
    }

    fn rows() -> Vec<Download> {
        vec![
            dl(
                1,
                "ubuntu.iso",
                "Ubuntu LTS",
                "https://releases.ubuntu.com/u.iso",
                "https://ubuntu.com/download",
                "https://ubuntu.com/",
            ),
            dl(2, "Straße.pdf", "map", "https://example.de/s.pdf", "", ""),
            dl(
                3,
                "setup.exe",
                "converter",
                "https://cdn.vendor.com/setup.exe",
                "",
                "https://www.google.com/",
            ),
            dl(
                4,
                "Setup.exe",
                "",
                "https://mirror.example.org/Setup.exe",
                "",
                "",
            ),
            dl(
                5,
                "lecture-01.mp4",
                "lecture 1",
                "https://video.uni.edu/01.mp4",
                "",
                "",
            ),
        ]
    }

    fn ids(query: &str, flags: i64) -> Vec<i64> {
        let r = rows();
        let mut s = SearchSettings {
            query: query.into(),
            flags,
            active: true,
        };
        filter(&r, &mut s).unwrap().iter().map(|d| d.id).collect()
    }

    #[test]
    fn empty_query_matches_everything() {
        let m = Matcher::new(&SearchSettings::new("", TARGET_NAME));
        assert!(rows().iter().all(|d| m.matches(d)));
    }

    #[test]
    fn case_insensitive_by_default() {
        assert_eq!(ids("SETUP", TARGET_NAME), [3, 4]);
    }

    #[test]
    fn match_case() {
        assert_eq!(ids("Setup", TARGET_NAME | MATCH_CASE), [4]);
    }

    #[test]
    fn casefold_handles_sharp_s() {
        assert_eq!(ids("STRASSE", TARGET_NAME), [2]);
    }

    #[test]
    fn whole_string_is_full_field_equality() {
        assert_eq!(ids("setup", TARGET_NAME | WHOLE_STRING), Vec::<i64>::new());
        assert_eq!(ids("setup.exe", TARGET_NAME | WHOLE_STRING), [3, 4]);
    }

    #[test]
    fn targets_are_independent() {
        assert_eq!(ids("ubuntu.com", TARGET_NAME), Vec::<i64>::new());
        assert_eq!(ids("ubuntu.com", TARGET_LINKS), [1]);
    }

    #[test]
    fn links_include_parent_page_and_referer() {
        assert_eq!(ids("google", TARGET_LINKS), [3]);
    }

    #[test]
    fn description_target() {
        assert_eq!(ids("lecture", TARGET_DESCRIPTION), [5]);
    }

    #[test]
    fn no_target_is_rejected_and_name_reenabled() {
        let r = rows();
        let mut s = SearchSettings {
            query: "x".into(),
            flags: MATCH_CASE,
            active: true,
        };
        assert_eq!(filter(&r, &mut s).unwrap_err().to_string(), NO_TARGET);
        assert_ne!(s.flags & TARGET_NAME, 0);
    }

    #[test]
    fn inactive_filter_shows_all() {
        let r = rows();
        let mut s = SearchSettings::new("nothing-matches", TARGET_NAME);
        assert_eq!(filter(&r, &mut s).unwrap().len(), 5);
    }

    #[test]
    fn find_next_walks_then_wraps_then_reports_none() {
        let r = rows();
        let mut f = FindNext::new(SearchSettings::new("setup", TARGET_NAME));
        assert_eq!(f.next(&r), Ok(Some(2)));
        assert_eq!(f.next(&r), Ok(Some(3)));
        assert_eq!(f.next(&r), Ok(Some(2))); // wrapped to the top
        f.settings.query = "missing".into();
        assert_eq!(f.next(&r), Ok(None));
        assert_eq!(f.cursor, 0);
    }

    #[test]
    fn find_next_new_query_restarts_from_top() {
        let r = rows();
        let mut f = FindNext::new(SearchSettings::new("lecture", TARGET_NAME));
        f.cursor = 4;
        f.last_query = "old".into();
        assert_eq!(f.next(&r), Ok(Some(4)));
        f.settings.query = "ubuntu".into();
        assert_eq!(f.next(&r), Ok(Some(0)));
    }

    #[test]
    fn find_next_empty_query_skips_validation() {
        let r = rows();
        let mut f = FindNext::new(SearchSettings::new("", 0));
        assert_eq!(f.next(&r), Ok(None));
        assert_eq!(f.settings.flags, 0);
    }
}
