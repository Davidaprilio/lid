//! What the main window shows, computed from the manager (`download_model.py` and the list, tree,
//! status bar and details code of `main_window.py`/`details.py` in 1.x). The window only draws it.
//!
//! The UI state the window owns (tree selection, search, checked and selected rows) is passed in.

use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::checksum;
use crate::engine::Snapshot;
use crate::fmt;
use crate::manager::DownloadManager;
use crate::models::{Download, MAIN_QUEUE, SYNC_QUEUE, Segment, Status};
use crate::search::{SearchSettings, filter};
use crate::settings::DEFAULT_COLUMNS;

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(https?|ftp)://\S+\n?\z").unwrap());
static HTTP_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^HTTP (\d{3})").unwrap());

/// `^(https?|ftp)://\S+$`, case-insensitive (the URL check of the add dialogs and the browser path).
pub fn is_url(s: &str) -> bool {
    URL_RE.is_match(s)
}

/// Categories with their own icon; others use "folder".
const CATEGORY_ICONS: [&str; 5] = ["compressed", "documents", "music", "programs", "video"];
/// Listed only under "All Downloads".
pub const HIDDEN_TREE_CATEGORIES: [&str; 1] = ["General"];
/// Columns drawn in the monospace font.
pub const MONO_KEYS: [&str; 3] = ["size", "time_left", "rate"];
/// Smallest widths of some columns.
pub const MIN_WIDTH: [(&str, i64); 5] = [
    ("queue", 34),
    ("size", 88),
    ("status", 170),
    ("time_left", 96),
    ("rate", 116),
];

pub fn category_icon(name: &str) -> String {
    let lower = name.to_lowercase();
    if CATEGORY_ICONS.contains(&lower.as_str()) {
        format!("category-{lower}")
    } else {
        "folder".into()
    }
}

pub fn column_title(key: &str) -> &'static str {
    DEFAULT_COLUMNS
        .iter()
        .find(|c| c.key == key)
        .map_or("", |c| c.title)
}

// ------------------------------------------------------------------ tree

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    All,
    Unfinished,
    Finished,
    Grabber,
    Queues,
    Queue,
}

/// A node of the sidebar: what the list shows when it is selected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
pub struct TreeFilter {
    pub kind: NodeKind,
    pub value: Option<String>,
}

impl Default for TreeFilter {
    fn default() -> Self {
        TreeFilter {
            kind: NodeKind::All,
            value: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeNode {
    pub title: String,
    pub icon: String,
    pub filter: TreeFilter,
    /// The badge; 0 draws none.
    pub count: usize,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

/// Main, Synchronization, then the others sorted.
pub fn queue_names(m: &DownloadManager) -> Vec<String> {
    let mut rest: Vec<String> = m
        .queues
        .keys()
        .filter(|n| *n != MAIN_QUEUE && *n != SYNC_QUEUE)
        .cloned()
        .collect();
    rest.sort();
    let mut v = vec![MAIN_QUEUE.to_string(), SYNC_QUEUE.to_string()];
    v.extend(rest);
    v
}

fn count(m: &DownloadManager, f: &TreeFilter) -> usize {
    let ds = m.downloads.values();
    match f.kind {
        NodeKind::All | NodeKind::Unfinished | NodeKind::Finished => ds
            .filter(|d| match f.kind {
                NodeKind::All => true,
                NodeKind::Finished => d.status == Status::Complete,
                _ => d.status != Status::Complete,
            })
            .filter(|d| f.value.as_ref().is_none_or(|v| &d.category == v))
            .count(),
        NodeKind::Queue => ds
            .filter(|d| Some(&d.queue) == f.value.as_ref() && d.status != Status::Complete)
            .count(),
        _ => 0,
    }
}

pub fn tree(m: &DownloadManager) -> Vec<TreeNode> {
    let node = |title: &str, icon: &str, kind: NodeKind, value: Option<String>| {
        let filter = TreeFilter { kind, value };
        TreeNode {
            title: title.into(),
            icon: icon.into(),
            count: count(m, &filter),
            filter,
            expanded: false,
            children: vec![],
        }
    };
    let cats: Vec<&str> = m
        .settings
        .categories
        .iter()
        .map(|c| c.name.as_str())
        .filter(|n| !HIDDEN_TREE_CATEGORIES.contains(n))
        .collect();
    let mut out = Vec::new();
    for (kind, title, icon) in [
        (NodeKind::All, "All Downloads", "folder-download"),
        (NodeKind::Unfinished, "Unfinished", "unfinished"),
        (NodeKind::Finished, "Finished", "finished"),
    ] {
        let mut top = node(title, icon, kind.clone(), None);
        top.children = cats
            .iter()
            .map(|c| node(c, &category_icon(c), kind.clone(), Some(c.to_string())))
            .collect();
        top.expanded = kind == NodeKind::All;
        out.push(top);
    }
    out.push(node("Grabber projects", "grabber", NodeKind::Grabber, None));
    let mut queues = node("Queues", "queue", NodeKind::Queues, None);
    queues.children = queue_names(m)
        .iter()
        .map(|q| {
            node(
                q,
                if q == SYNC_QUEUE {
                    "queue-sync"
                } else {
                    "queue"
                },
                NodeKind::Queue,
                Some(q.clone()),
            )
        })
        .collect();
    queues.expanded = true;
    out.push(queues);
    out
}

/// The filter if the tree still has that node, else the first node (All Downloads).
pub fn keep_filter(nodes: &[TreeNode], f: &TreeFilter) -> TreeFilter {
    fn has(nodes: &[TreeNode], f: &TreeFilter) -> bool {
        nodes.iter().any(|n| &n.filter == f || has(&n.children, f))
    }
    if has(nodes, f) {
        f.clone()
    } else {
        TreeFilter::default()
    }
}

// ------------------------------------------------------------------ list

/// Sort order of a column key (1.x `SORT_KEYS`, including the odd ones: time left sorts by bytes left,
/// transfer rate by bytes downloaded). Unknown keys sort like "date_added".
pub fn compare(key: &str, a: &Download, b: &Download) -> std::cmp::Ordering {
    let f = |x: f64, y: f64| x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
    match key {
        "filename" => a.filename.to_lowercase().cmp(&b.filename.to_lowercase()),
        "size" => a.size.cmp(&b.size),
        "status" => a
            .status
            .as_str()
            .cmp(b.status.as_str())
            .then_with(|| f(a.progress(), b.progress())),
        "time_left" => (a.size - a.downloaded).cmp(&(b.size - b.downloaded)),
        "rate" => a.downloaded.cmp(&b.downloaded),
        "last_try" => f(a.last_try, b.last_try),
        "description" => a
            .description
            .to_lowercase()
            .cmp(&b.description.to_lowercase()),
        "save_to" => a.save_dir.to_lowercase().cmp(&b.save_dir.to_lowercase()),
        "referer" => a.referer.cmp(&b.referer),
        "parent_page" => a.parent_page.cmp(&b.parent_page),
        "queue" => a
            .queue
            .cmp(&b.queue)
            .then(a.queue_order.cmp(&b.queue_order)),
        _ => a.id.cmp(&b.id),
    }
}

pub const SORT_KEYS: [&str; 12] = [
    "date_added",
    "filename",
    "size",
    "status",
    "time_left",
    "rate",
    "last_try",
    "description",
    "save_to",
    "referer",
    "parent_page",
    "queue",
];

/// The ids the list shows for a tree node and the search, in display order. An invalid search (no
/// target) is ignored here.
pub fn visible(m: &DownloadManager, f: &TreeFilter, search: &SearchSettings) -> Vec<i64> {
    let mut items: Vec<&Download> = m
        .downloads
        .values()
        .filter(|d| match f.kind {
            NodeKind::Unfinished => d.status != Status::Complete,
            NodeKind::Finished => d.status == Status::Complete,
            NodeKind::Queue => Some(&d.queue) == f.value.as_ref(),
            NodeKind::Queues => !d.queue.is_empty(),
            NodeKind::Grabber => false,
            NodeKind::All => true,
        })
        .filter(|d| {
            !matches!(
                f.kind,
                NodeKind::All | NodeKind::Unfinished | NodeKind::Finished
            ) || f.value.as_ref().is_none_or(|v| &d.category == v)
        })
        .collect();
    let mut s = search.clone();
    if let Ok(found) = filter(&items, &mut s) {
        items = found.into_iter().copied().collect();
    }
    if f.kind == NodeKind::Queue {
        items.sort_by_key(|d| d.queue_order);
    } else {
        let key = if SORT_KEYS.contains(&m.settings.sort_key.as_str()) {
            m.settings.sort_key.as_str()
        } else {
            "date_added"
        };
        // A stable sort either way: like Python's reverse=True, equal items keep the list order.
        if m.settings.sort_desc {
            items.sort_by(|a, b| compare(key, b, a));
        } else {
            items.sort_by(|a, b| compare(key, a, b));
        }
    }
    items.iter().map(|d| d.id).collect()
}

/// What the Status pill shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pill {
    /// run | pause | done | verified | mismatch | error | queue
    pub kind: &'static str,
    pub progress: f64,
    pub text: String,
}

pub fn status_pill(m: &DownloadManager, d: &Download) -> Pill {
    let running = m.is_running(d.id);
    // Finished with a checksum; the manager starts the check on the next poll.
    let pending = d.status == Status::Complete
        && !d.checksum.is_empty()
        && d.checksum_result.is_empty()
        && m.has_task(d.id)
        && m.settings.verify_checksum;
    let pill = |kind, progress, text: String| Pill {
        kind,
        progress,
        text,
    };
    if d.status == Status::Verifying || pending {
        let p = m.verify_progress(d.id).unwrap_or(0.0);
        return pill("run", p, format!("Verifying {:.0}%", p * 100.0));
    }
    match d.status {
        Status::Complete => match d.checksum_result.as_str() {
            "verified" => pill("verified", 1.0, "Verified".into()),
            "mismatch" => pill("mismatch", 1.0, "Checksum mismatch".into()),
            _ => pill("done", 1.0, "Complete".into()),
        },
        Status::Error => {
            let text = HTTP_CODE
                .captures(&d.error)
                .map_or("Error".into(), |c| format!("Error {}", &c[1]));
            pill("error", d.progress(), text)
        }
        _ if running => {
            let text = if d.status != Status::Downloading {
                fmt::status(d)
            } else if d.size > 0 {
                format!("{:.1}%", d.progress() * 100.0)
            } else {
                fmt::size(d.downloaded)
            };
            pill("run", d.progress(), text)
        }
        _ if d.downloaded > 0 => {
            let text = if d.size > 0 {
                format!("{:.1}% · Paused", d.progress() * 100.0)
            } else {
                format!("{} · Paused", fmt::size(d.downloaded))
            };
            pill("pause", d.progress(), text)
        }
        _ => pill("queue", 0.0, "Queued".into()),
    }
}

/// The text of a cell ("" for the queue and status columns, which draw an icon and a pill).
pub fn cell_text(d: &Download, key: &str, running_rate: Option<f64>) -> String {
    match key {
        "filename" => d.filename.clone(),
        "size" => fmt::size(d.size),
        "time_left" => match running_rate {
            Some(r) if r > 0.0 && d.size > 0 => {
                fmt::duration(((d.size - d.downloaded) as f64 / r) as i64)
            }
            _ => String::new(),
        },
        "rate" => running_rate.map(fmt::rate).unwrap_or_default(),
        "last_try" => fmt::date(d.last_try),
        "date_added" => fmt::date(d.date_added),
        "description" => d.description.clone(),
        "save_to" => d.path(),
        "referer" => d.referer.clone(),
        "parent_page" => d.parent_page.clone(),
        _ => String::new(),
    }
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub id: i64,
    pub checked: bool,
    /// Text per visible column key.
    pub cells: Vec<String>,
    pub icon: String,
    pub pill: Pill,
    /// Tooltip of the status pill (the error) and of the queue icon (the queue name).
    pub error: String,
    pub queue: String,
}

/// The visible columns: the checkbox column `_check` first, then the user's visible columns.
pub fn columns(m: &DownloadManager) -> Vec<String> {
    m.settings
        .columns
        .iter()
        .filter(|c| c.1)
        .map(|c| c.0.clone())
        .collect()
}

/// Rows for `ids`; `rates` are the live rates of the running tasks.
pub fn rows(
    m: &DownloadManager,
    ids: &[i64],
    checked: &HashSet<i64>,
    rate: &dyn Fn(i64) -> Option<f64>,
) -> Vec<Row> {
    let cols = columns(m);
    ids.iter()
        .filter_map(|id| m.downloads.get(id))
        .map(|d| {
            let r = if m.is_running(d.id) { rate(d.id) } else { None };
            Row {
                id: d.id,
                checked: checked.contains(&d.id),
                cells: cols.iter().map(|k| cell_text(d, k, r)).collect(),
                icon: category_icon(&d.category),
                pill: status_pill(m, d),
                error: if d.status == Status::Error {
                    d.error.clone()
                } else {
                    String::new()
                },
                queue: d.queue.clone(),
            }
        })
        .collect()
}

/// The header checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    Unchecked,
    Partial,
    Checked,
}

pub fn check_state(visible: &[i64], checked: &HashSet<i64>) -> CheckState {
    let n = visible.iter().filter(|i| checked.contains(i)).count();
    if n == 0 {
        CheckState::Unchecked
    } else if n == visible.len() {
        CheckState::Checked
    } else {
        CheckState::Partial
    }
}

/// Clicking the header box: uncheck all when all are checked, else check all visible.
pub fn toggle_all(visible: &[i64], checked: &mut HashSet<i64>) {
    if check_state(visible, checked) == CheckState::Checked {
        checked.clear();
    } else {
        *checked = visible.iter().copied().collect();
    }
}

/// Checks only apply to what the list shows.
pub fn prune_checks(visible: &[i64], checked: &mut HashSet<i64>) {
    let v: HashSet<i64> = visible.iter().copied().collect();
    checked.retain(|i| v.contains(i));
}

/// What Start, Stop and Delete act on: the checked rows (in list order), or the selection when nothing
/// is checked.
pub fn targets(visible: &[i64], checked: &HashSet<i64>, selected: &[i64]) -> Vec<i64> {
    let c: Vec<i64> = visible
        .iter()
        .copied()
        .filter(|i| checked.contains(i))
        .collect();
    if c.is_empty() {
        let sel: BTreeSet<i64> = selected.iter().copied().collect();
        visible
            .iter()
            .copied()
            .filter(|i| sel.contains(i))
            .collect()
    } else {
        c
    }
}

/// Which actions are enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct ActionState {
    pub resume: bool,
    pub stop: bool,
    pub remove: bool,
    pub redownload: bool,
    pub stop_all: bool,
    pub delete_completed: bool,
    pub checked: usize,
}

pub fn actions(m: &DownloadManager, targets: &[i64], checked_count: usize) -> ActionState {
    let ds: Vec<&Download> = targets.iter().filter_map(|i| m.downloads.get(i)).collect();
    let running = ds.iter().any(|d| m.is_running(d.id));
    ActionState {
        resume: ds.iter().any(|d| {
            !m.is_running(d.id) && !matches!(d.status, Status::Complete | Status::Verifying)
        }),
        stop: running || ds.iter().any(|d| d.status == Status::Verifying),
        remove: !ds.is_empty(),
        redownload: !ds.is_empty() && !running,
        stop_all: m.task_count() > 0,
        delete_completed: m.downloads.values().any(|d| d.status == Status::Complete),
        checked: checked_count,
    }
}

// ------------------------------------------------------------------ status bar and tray

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusBar {
    pub speed: String,
    pub counts: String,
    pub limiter: String,
    pub clipboard: String,
}

/// `total_rate`: the sum of the running tasks' rates.
pub fn status_bar(m: &DownloadManager, total_rate: f64) -> StatusBar {
    let rate = fmt::rate(total_rate);
    let queued = m
        .downloads
        .values()
        .filter(|d| {
            !d.queue.is_empty()
                && !matches!(d.status, Status::Complete | Status::Verifying)
                && !m.is_running(d.id)
        })
        .count();
    let verifying = m.verifier_count();
    StatusBar {
        speed: format!(
            "↓ {}",
            if rate.is_empty() {
                "0 B/s".into()
            } else {
                rate
            }
        ),
        counts: format!(
            "{} downloading · {}{queued} in queue",
            m.task_count(),
            if verifying > 0 {
                format!("{verifying} verifying · ")
            } else {
                String::new()
            }
        ),
        limiter: format!(
            "Speed Limiter: {}",
            if m.settings.speed_limiter_on {
                "on"
            } else {
                "off"
            }
        ),
        clipboard: format!(
            "Clipboard: {}",
            if m.settings.monitor_clipboard {
                "on"
            } else {
                "off"
            }
        ),
    }
}

pub fn tray_tooltip(m: &DownloadManager) -> String {
    match m.task_count() {
        0 => "LID".into(),
        n => format!("LID - {n} active download(s)"),
    }
}

// ------------------------------------------------------------------ details pane

/// "2.39 / 5.68 GB": the unit once when both sizes share it.
pub fn of(part: i64, whole: i64) -> String {
    let (a, b) = (fmt::size(part), fmt::size(whole));
    let unit = |s: &str| s.split_whitespace().last().map(String::from);
    if unit(&a) == unit(&b) {
        format!("{} / {b}", a.split_whitespace().next().unwrap_or(""))
    } else {
        format!("{a} / {b}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectionRow {
    pub n: String,
    pub downloaded: String,
    pub speed: String,
    pub info: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Details {
    pub id: i64,
    pub filename: String,
    pub icon: String,
    /// The URL without its scheme; `url_full` is the tooltip.
    pub url: String,
    pub url_full: String,
    pub state: String,
    /// pause | skip | again | open | resume
    pub action: &'static str,
    pub action_label: &'static str,
    pub action_icon: &'static str,
    pub percent: String,
    pub of: String,
    pub bar: f64,
    pub caption: &'static str,
    pub size: i64,
    pub segments: Vec<Segment>,
    pub rate: String,
    pub time_left: String,
    pub done: String,
    pub connections: String,
    pub resume: &'static str,
    pub checksum: String,
    /// "" | okText | badText
    pub checksum_tone: &'static str,
    pub rows: Vec<ConnectionRow>,
    pub limit_on: bool,
    pub limit_kbps: i64,
}

pub const SEGMENTS: &str = "Start positions and download progress by connections";

pub fn details(m: &DownloadManager, d: &Download, snap: Option<&Snapshot>) -> Details {
    let running = m.is_running(d.id);
    let empty = Snapshot {
        segments: d.segments.clone(),
        connections: vec![],
        rate: 0.0,
        time_left: -1,
    };
    let snap = snap.unwrap_or(&empty);
    let kind = checksum::title(&d.checksum);
    let verifying = m.verify_progress(d.id);
    let state = match d.status {
        Status::Complete => "Complete".into(),
        Status::Error => format!("Error: {}", d.error),
        Status::Paused => "Paused".into(),
        Status::Assembling => "Assembling…".into(),
        Status::Connecting => "Connecting…".into(),
        Status::Verifying => format!("Verifying {kind}…"),
        _ if running => "Receiving data…".into(),
        _ if d.downloaded == 0 => "Queued".into(),
        _ => "Paused".into(),
    };
    let line = match (d.status, d.checksum_result.as_str()) {
        (Status::Complete, "verified") => "Complete · checksum verified".into(),
        (Status::Complete, "mismatch") => format!("Checksum mismatch · {}", d.category),
        _ => format!("{state} · {}", d.category),
    };
    let (action, label, icon) = if running {
        ("pause", "Pause", "stop")
    } else if d.status == Status::Verifying {
        ("skip", "Skip check", "")
    } else if d.status == Status::Complete {
        if d.checksum_result == "mismatch" {
            ("again", "Download again", "")
        } else {
            ("open", "Open", "")
        }
    } else {
        ("resume", "Resume", "resume")
    };
    let (percent, of_line, bar, caption) = match verifying {
        Some(v) => (
            format!("{:.0}%", v * 100.0),
            if d.size > 0 {
                format!(
                    "{} of {} read",
                    fmt::size((d.size as f64 * v) as i64),
                    fmt::size(d.size)
                )
            } else {
                String::new()
            },
            v,
            "Checksum progress — reading the merged file",
        ),
        None => (
            if d.size > 0 {
                format!("{:.1}%", d.progress() * 100.0)
            } else {
                fmt::size(d.downloaded)
            },
            if d.size > 0 {
                format!("{} of {}", fmt::size(d.downloaded), fmt::size(d.size))
            } else {
                String::new()
            },
            d.progress(),
            SEGMENTS,
        ),
    };
    let alive = snap.connections.iter().filter(|c| c.alive).count();
    let finished = snap
        .segments
        .iter()
        .filter(|s| s.end >= 0 && s.pos >= s.end)
        .count();
    let (check, tone) = if d.checksum.is_empty() {
        ("—".into(), "")
    } else if let Some(v) = verifying {
        (format!("{kind} · {:.0}%", v * 100.0), "")
    } else if d.status == Status::Complete
        && ["verified", "mismatch", "skipped"].contains(&d.checksum_result.as_str())
    {
        let tone = match d.checksum_result.as_str() {
            "verified" => "okText",
            "mismatch" => "badText",
            _ => "",
        };
        (format!("{kind} · {}", d.checksum_result), tone)
    } else {
        (format!("{kind} · pending"), "")
    };
    Details {
        id: d.id,
        filename: d.filename.clone(),
        icon: category_icon(&d.category),
        url: d
            .url
            .split_once("://")
            .map_or(d.url.as_str(), |(_, r)| r)
            .to_string(),
        url_full: d.url.clone(),
        state: line,
        action,
        action_label: label,
        action_icon: icon,
        percent,
        of: of_line,
        bar,
        caption,
        size: d.size,
        segments: snap.segments.clone(),
        rate: if running {
            fmt::rate(snap.rate)
        } else {
            "—".into()
        },
        time_left: if running && snap.time_left >= 0 {
            fmt::duration(snap.time_left)
        } else {
            "—".into()
        },
        done: if d.size > 0 {
            of(d.downloaded, d.size)
        } else {
            fmt::size(d.downloaded)
        },
        connections: if !snap.connections.is_empty() || finished > 0 {
            format!("{alive} active · {finished} done")
        } else {
            "—".into()
        },
        resume: match d.resumable {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "Unknown",
        },
        checksum: check,
        checksum_tone: tone,
        rows: snap
            .connections
            .iter()
            .map(|c| ConnectionRow {
                n: c.index.to_string(),
                downloaded: fmt::size(c.received),
                speed: if c.alive && running {
                    fmt::rate(c.rate)
                } else {
                    String::new()
                },
                info: if c.alive || running {
                    c.status.clone()
                } else {
                    String::new()
                },
            })
            .collect(),
        limit_on: d.speed_limit_kbps > 0,
        limit_kbps: if d.speed_limit_kbps > 0 {
            d.speed_limit_kbps
        } else {
            m.settings.speed_limit_kbps
        },
    }
}

/// A file name not used by another download in `save_dir` nor by a file there: `name`, `name (2)`, …
pub fn unique_name(m: &DownloadManager, save_dir: &str, name: &str) -> String {
    let taken: HashSet<&str> = m
        .downloads
        .values()
        .filter(|d| d.save_dir == save_dir)
        .map(|d| d.filename.as_str())
        .collect();
    let ext = crate::pyutil::splitext_ext(name);
    let stem = &name[..name.len() - ext.len()];
    let mut candidate = name.to_string();
    let mut n = 2;
    while taken.contains(candidate.as_str())
        || std::path::Path::new(save_dir).join(&candidate).exists()
    {
        candidate = format!("{stem} ({n}){ext}");
        n += 1;
    }
    candidate
}

/// Add addresses to the Main queue (batch, drop, import): a safe name from the URL, the category's
/// folder, a free name. Returns how many were added.
pub fn add_many(
    m: &mut DownloadManager,
    items: &[(String, String, String)],
    username: &str,
    password: &str,
) -> usize {
    for (url, referer, description) in items {
        let name = crate::names::safe_filename(&crate::names::filename_from_url(url));
        let cat = crate::categories::classify(&m.settings, &name, url).clone();
        let unique = unique_name(m, &cat.save_dir, &name);
        m.add(
            url,
            &unique,
            crate::manager::NewDownload {
                save_dir: Some(cat.save_dir.clone()),
                category: Some(cat.name.clone()),
                description: description.clone(),
                referer: referer.clone(),
                username: username.into(),
                password: password.into(),
                queue: MAIN_QUEUE.into(),
                ..Default::default()
            },
        );
    }
    items.len()
}
