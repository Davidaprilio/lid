//! Batch download addresses (`BatchDialog.urls` in 1.x): one `*` in the address is replaced by numbers
//! or letters.

/// What replaces the asterisk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Fill {
    /// `from..=to` (counting down when `to < from`), zero-padded to `width` digits.
    Numbers { from: u32, to: u32, width: usize },
    /// Letters of `a–z` then `A–Z` between the two (in either order); empty means a / z.
    Letters { from: String, to: String },
}

const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The addresses, or none when the pattern does not hold exactly one `*`.
pub fn expand(pattern: &str, fill: &Fill) -> Vec<String> {
    let pattern = pattern.trim();
    if pattern.matches('*').count() != 1 {
        return vec![];
    }
    let parts: Vec<String> = match fill {
        Fill::Numbers { from, to, width } => {
            let (a, b) = (*from as i64, *to as i64);
            let step = if b >= a { 1 } else { -1 };
            let mut v = Vec::new();
            let mut n = a;
            loop {
                v.push(format!("{n:0width$}", width = *width));
                if n == b {
                    break;
                }
                n += step;
            }
            v
        }
        Fill::Letters { from, to } => {
            let a = if from.is_empty() { "a" } else { from };
            let b = if to.is_empty() { "z" } else { to };
            let (Some(i), Some(j)) = (index(a), index(b)) else {
                return vec![];
            };
            LETTERS[i.min(j)..=i.max(j)]
                .chars()
                .map(String::from)
                .collect()
        }
    };
    parts.iter().map(|p| pattern.replace('*', p)).collect()
}

/// The position of a single letter in `a–zA–Z`.
fn index(s: &str) -> Option<usize> {
    let mut c = s.chars();
    let (Some(ch), None) = (c.next(), c.next()) else {
        return None;
    };
    LETTERS.find(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(from: u32, to: u32, width: usize) -> Fill {
        Fill::Numbers { from, to, width }
    }

    #[test]
    fn numbers() {
        let u = expand("https://x/img*.jpg", &n(1, 3, 2));
        assert_eq!(
            u,
            [
                "https://x/img01.jpg",
                "https://x/img02.jpg",
                "https://x/img03.jpg"
            ]
        );
        assert_eq!(
            expand("https://x/*", &n(3, 1, 1)),
            ["https://x/3", "https://x/2", "https://x/1"]
        );
        assert_eq!(expand("https://x/*", &n(5, 5, 3)), ["https://x/005"]);
        assert_eq!(
            expand("https://x/*", &n(9, 11, 1)),
            ["https://x/9", "https://x/10", "https://x/11"]
        );
    }

    #[test]
    fn one_asterisk_only() {
        assert!(expand("https://x/a.jpg", &n(1, 2, 1)).is_empty());
        assert!(expand("https://x/*/*.jpg", &n(1, 2, 1)).is_empty());
    }

    #[test]
    fn letters() {
        let f = |a: &str, b: &str| Fill::Letters {
            from: a.into(),
            to: b.into(),
        };
        assert_eq!(expand("u/*", &f("a", "c")), ["u/a", "u/b", "u/c"]);
        assert_eq!(expand("u/*", &f("c", "a")), ["u/a", "u/b", "u/c"]); // never reversed
        assert_eq!(expand("u/*", &f("y", "B")), ["u/y", "u/z", "u/A", "u/B"]);
        assert_eq!(expand("u/*", &f("", "")).len(), 26);
        assert!(expand("u/*", &f("1", "z")).is_empty());
    }
}
