//! The running program: the download manager, what the user has selected, and the 500 ms tick that
//! polls the manager and rebuilds what the window shows (the view model of `lider_core::view`).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use lider_core::manager::DownloadManager;
use lider_core::paths::Dirs;
use lider_core::search::SearchSettings;
use lider_core::view::{self, Row, StatusBar, TreeFilter, TreeNode};

use crate::theme::{Palette, ThemeName};

pub const TICK: Duration = Duration::from_millis(500);

/// What the window shows, computed from the manager.
#[derive(Default)]
pub struct View {
    pub tree: Vec<TreeNode>,
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub status: Option<StatusBar>,
}

pub struct App {
    /// The engine runs on this runtime; it lives as long as the window.
    #[allow(dead_code)]
    pub rt: tokio::runtime::Runtime,
    pub manager: DownloadManager,
    #[allow(dead_code)]
    pub dirs: Dirs,
    pub filter: TreeFilter,
    pub search: SearchSettings,
    pub checked: HashSet<i64>,
    pub selected: Vec<i64>,
    pub visible: Vec<i64>,
    pub theme: ThemeName,
    pub palette: Palette,
    pub view: View,
    last_tick: Option<Instant>,
}

impl App {
    pub fn new(rt: tokio::runtime::Runtime, manager: DownloadManager, dirs: Dirs) -> App {
        let theme = ThemeName::parse(&manager.settings.theme);
        let search = SearchSettings {
            query: manager.settings.search_string.clone(),
            flags: manager.settings.search_flags & 0x1F,
            active: false,
        };
        App {
            rt,
            manager,
            dirs,
            filter: TreeFilter::default(),
            search,
            checked: HashSet::new(),
            selected: vec![],
            visible: vec![],
            theme,
            palette: Palette::of(theme),
            view: View::default(),
            last_tick: None,
        }
    }

    /// Poll the manager when a tick is due; true when the view was rebuilt.
    pub fn tick(&mut self) -> bool {
        if self.last_tick.is_some_and(|t| t.elapsed() < TICK) {
            return false;
        }
        self.last_tick = Some(Instant::now());
        let _events = self.manager.poll(None);
        self.rebuild();
        true
    }

    /// Recompute the view (after a tick or an action of the user).
    pub fn rebuild(&mut self) {
        let m = &self.manager;
        let tree = view::tree(m);
        self.filter = view::keep_filter(&tree, &self.filter);
        let visible = view::visible(m, &self.filter, &self.search);
        if visible != self.visible {
            view::prune_checks(&visible, &mut self.checked);
            let keep: HashSet<i64> = visible.iter().copied().collect();
            self.selected.retain(|i| keep.contains(i));
            self.visible = visible;
        }
        let rates: HashMap<i64, f64> = self
            .visible
            .iter()
            .filter(|i| m.is_running(**i))
            .filter_map(|i| m.task(*i).map(|t| (*i, t.snapshot().rate)))
            .collect();
        let total: f64 = m
            .downloads
            .keys()
            .filter(|i| m.is_running(**i))
            .filter_map(|i| {
                rates
                    .get(i)
                    .copied()
                    .or_else(|| m.task(*i).map(|t| t.snapshot().rate))
            })
            .sum();
        self.view = View {
            columns: view::columns(m),
            rows: view::rows(m, &self.visible, &self.checked, &|id| {
                rates.get(&id).copied()
            }),
            status: Some(view::status_bar(m, total)),
            tree,
        };
    }

    pub fn set_filter(&mut self, f: TreeFilter) {
        self.filter = f;
        self.rebuild();
    }
}
