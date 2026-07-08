//! Application state and event handling.

use crate::path::{display_path, parse, PathEntry};
use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use std::path::PathBuf;

// ----------------------------------------------------------------------
// Row / target types
// ----------------------------------------------------------------------

/// A flat, navigable row in the tree view.
#[derive(Debug, Clone, Copy)]
pub enum Row {
    /// A top-level PATH entry. `number` is the 1-based display index
    /// (counts only entries, not children or empty-markers).
    Entry { index: usize, number: usize },
    /// A child file inside an expanded PATH entry.
    Child { entry: usize, child: usize },
    /// Synthetic "(empty)" row for expanded-but-empty directories.
    Empty(usize),
}

/// A logical search hit — identified by what it points to, not by its
/// current flat row index (which depends on expansion state).
#[derive(Debug, Clone, Copy)]
pub enum MatchTarget {
    Entry(usize),
    Child { entry: usize, child: usize },
}

// ----------------------------------------------------------------------
// Filters
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
pub struct Filters {
    pub hide_duplicates: bool,
    pub hide_overwritten: bool,
    pub hide_missing: bool,
}

impl Filters {
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.hide_duplicates {
            parts.push("no-dupes");
        }
        if self.hide_overwritten {
            parts.push("no-overwrites");
        }
        if self.hide_missing {
            parts.push("no-missing");
        }
        if parts.is_empty() {
            "off".to_string()
        } else {
            parts.join(",")
        }
    }
}

// ----------------------------------------------------------------------
// Search state
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Search {
    /// Current query being edited, or the last committed query if not
    /// editing.
    pub query: String,
    /// The last successfully-committed query (drives `matches`).
    pub committed: String,
    /// Whether we're currently editing the query in the prompt.
    pub editing: bool,
    /// Cached list of logical matches. Recomputed on commit.
    pub matches: Vec<MatchTarget>,
    /// Index into `matches` that the user is currently on.
    pub current: Option<usize>,
}

// ----------------------------------------------------------------------
// Modal
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modal {
    #[default]
    None,
    Filter { selected: usize },
}

pub const FILTER_OPTIONS: [(&str, &str); 3] = [
    ("hide duplicate directories", "d"),
    ("hide overwritten files", "o"),
    ("hide missing directories", "m"),
];

// ----------------------------------------------------------------------
// App
// ----------------------------------------------------------------------

pub struct App {
    pub entries: Vec<PathEntry>,
    pub expanded: Vec<bool>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub status: String,
    pub filters: Filters,
    pub search: Search,
    pub modal: Modal,
    pub quit: bool,
}

impl App {
    pub fn new(path_var: &str) -> Self {
        let entries = parse(path_var);
        let expanded = vec![false; entries.len()];
        let status = if entries.is_empty() {
            "$PATH is empty".to_string()
        } else {
            String::new()
        };
        Self {
            entries,
            expanded,
            cursor: 0,
            scroll_offset: 0,
            status,
            filters: Filters::default(),
            search: Search::default(),
            modal: Modal::None,
            quit: false,
        }
    }

    // ------------------------------------------------------------------
    // Row construction
    // ------------------------------------------------------------------

    pub fn rows(&self) -> Vec<Row> {
        let mut out = Vec::new();
        let mut n = 0;
        for (i, e) in self.entries.iter().enumerate() {
            if self.filters.hide_duplicates && e.duplicate_of.is_some() {
                continue;
            }
            if self.filters.hide_missing && !e.exists {
                continue;
            }
            n += 1;
            out.push(Row::Entry { index: i, number: n });
            if self.expanded.get(i).copied().unwrap_or(false) {
                if e.exists && e.children.is_empty() {
                    out.push(Row::Empty(i));
                } else {
                    for (j, c) in e.children.iter().enumerate() {
                        if self.filters.hide_overwritten && c.overwritten_by.is_some() {
                            continue;
                        }
                        out.push(Row::Child { entry: i, child: j });
                    }
                }
            }
        }
        out
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    // ------------------------------------------------------------------
    // Cursor / viewport
    // ------------------------------------------------------------------

    fn clamp_cursor(&mut self) {
        let max = self.row_count().saturating_sub(1);
        if self.cursor > max {
            self.cursor = max;
        }
    }

    pub fn ensure_cursor_visible(&mut self, viewport_h: usize) {
        let vh = viewport_h.max(1);
        let max = self.row_count().saturating_sub(1);
        if self.cursor > max {
            self.cursor = max;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + vh {
            self.scroll_offset = self.cursor + 1 - vh;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.row_count().saturating_sub(1);
        if self.cursor < max {
            self.cursor += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_to_top(&mut self) {
        self.cursor = 0;
    }

    pub fn move_to_bottom(&mut self) {
        self.cursor = self.row_count().saturating_sub(1);
    }

    pub fn move_page(&mut self, viewport_h: usize, down: bool) {
        let vh = viewport_h.max(1);
        let max = self.row_count().saturating_sub(1);
        if down {
            self.cursor = (self.cursor + vh).min(max);
        } else {
            self.cursor = self.cursor.saturating_sub(vh);
        }
    }

    pub fn move_half_page(&mut self, viewport_h: usize, down: bool) {
        let half = viewport_h.max(2) / 2;
        let max = self.row_count().saturating_sub(1);
        if down {
            self.cursor = (self.cursor + half).min(max);
        } else {
            self.cursor = self.cursor.saturating_sub(half);
        }
    }

    // ------------------------------------------------------------------
    // Expand / collapse
    // ------------------------------------------------------------------

    fn entry_index_of_row(row: Row) -> Option<usize> {
        match row {
            Row::Entry { index, .. } => Some(index),
            Row::Child { entry, .. } => Some(entry),
            Row::Empty(i) => Some(i),
        }
    }

    fn entry_under_cursor(&self) -> Option<usize> {
        let rows = self.rows();
        let row = rows.get(self.cursor)?;
        Self::entry_index_of_row(*row)
    }

    pub fn expand_entry(&mut self, idx: usize) {
        if self.entries[idx].exists {
            self.expanded[idx] = true;
        }
    }

    pub fn expand_here(&mut self) {
        if let Some(idx) = self.entry_under_cursor() {
            self.expand_entry(idx);
        }
    }

    pub fn collapse_here(&mut self) {
        let Some(idx) = self.entry_under_cursor() else {
            return;
        };
        self.expanded[idx] = false;
        let new_rows = self.rows();
        if let Some(pos) = new_rows.iter().position(|r| match r {
            Row::Entry { index, .. } => *index == idx,
            _ => false,
        }) {
            self.cursor = pos;
        }
        self.clamp_cursor();
    }

    pub fn toggle_here(&mut self) {
        let Some(idx) = self.entry_under_cursor() else {
            return;
        };
        if !self.entries[idx].exists {
            return;
        }
        if self.expanded[idx] {
            self.collapse_here();
        } else {
            self.expand_here();
        }
    }

    pub fn expand_all(&mut self) {
        for (i, e) in self.entries.iter().enumerate() {
            if e.exists {
                self.expanded[i] = true;
            }
        }
        self.status = "Expanded all".to_string();
    }

    pub fn collapse_all(&mut self) {
        for i in 0..self.entries.len() {
            self.expanded[i] = false;
        }
        self.cursor = 0;
        self.scroll_offset = 0;
        self.status = "Collapsed all".to_string();
    }

    // ------------------------------------------------------------------
    // Filters
    // ------------------------------------------------------------------

    pub fn toggle_filter_duplicates(&mut self) {
        self.filters.hide_duplicates = !self.filters.hide_duplicates;
        self.clamp_cursor();
        self.status = format!("Filter: {}", self.filters.label());
        self.recompute_search_matches();
    }

    pub fn toggle_filter_overwritten(&mut self) {
        self.filters.hide_overwritten = !self.filters.hide_overwritten;
        self.clamp_cursor();
        self.status = format!("Filter: {}", self.filters.label());
        self.recompute_search_matches();
    }

    pub fn toggle_filter_missing(&mut self) {
        self.filters.hide_missing = !self.filters.hide_missing;
        self.clamp_cursor();
        self.status = format!("Filter: {}", self.filters.label());
        self.recompute_search_matches();
    }

    // ------------------------------------------------------------------
    // Search
    // ------------------------------------------------------------------

    /// Begin editing a search query. Re-opening `/` always starts from
    /// an empty query (the previously committed search is preserved
    /// separately and will be restored on cancel).
    pub fn start_search(&mut self) {
        self.search.query.clear();
        self.search.editing = true;
    }

    /// Finish editing (commit) and jump to the first match.
    pub fn commit_search(&mut self) {
        self.search.editing = false;
        self.search.committed = self.search.query.clone();
        self.recompute_search_matches();
        if self.search.committed.is_empty() {
            self.status = "Search cleared".to_string();
            self.search.current = None;
            return;
        }
        if self.search.matches.is_empty() {
            self.status = format!("No matches for '{}'", self.search.committed);
            self.search.current = None;
        } else {
            self.search.current = Some(0);
            self.jump_to_current_match();
            self.status = format!(
                "Match 1/{} for '{}'",
                self.search.matches.len(),
                self.search.committed
            );
        }
    }

    /// Cancel editing: restore the previously committed query and close
    /// the prompt.
    pub fn cancel_search(&mut self) {
        self.search.editing = false;
        self.search.query = self.search.committed.clone();
    }

    pub fn search_push_char(&mut self, ch: char) {
        self.search.query.push(ch);
    }

    pub fn search_pop_char(&mut self) {
        self.search.query.pop();
    }

    /// Build `search.matches` by scanning *every* entry and *every*
    /// child (respecting active filters, but IGNORING expansion state).
    /// This way a collapsed directory is still searchable.
    fn recompute_search_matches(&mut self) {
        if self.search.committed.is_empty() {
            self.search.matches.clear();
            self.search.current = None;
            return;
        }
        let q = self.search.committed.to_lowercase();
        let mut matches = Vec::new();

        for (i, e) in self.entries.iter().enumerate() {
            // Skip entries hidden by filters.
            if self.filters.hide_duplicates && e.duplicate_of.is_some() {
                continue;
            }
            if self.filters.hide_missing && !e.exists {
                continue;
            }
            // Check the entry itself.
            let entry_text = display_path(&e.resolved).to_lowercase();
            if entry_text.contains(&q) {
                matches.push(MatchTarget::Entry(i));
            }
            // Check all children (regardless of whether the entry is
            // expanded). Skip children hidden by the overwrite filter.
            for (j, c) in e.children.iter().enumerate() {
                if self.filters.hide_overwritten && c.overwritten_by.is_some() {
                    continue;
                }
                if c.name.to_lowercase().contains(&q) {
                    matches.push(MatchTarget::Child { entry: i, child: j });
                }
            }
        }

        self.search.matches = matches;
        // Clamp current.
        self.search.current = match self.search.current {
            Some(c) if c < self.search.matches.len() => Some(c),
            Some(_) if !self.search.matches.is_empty() => Some(0),
            _ => None,
        };
    }

    /// Jump to the logical match pointed to by `search.current`.
    /// Auto-expands the parent entry if the match is a child.
    fn jump_to_current_match(&mut self) {
        let Some(cur) = self.search.current else {
            return;
        };
        let Some(&target) = self.search.matches.get(cur) else {
            return;
        };

        // Ensure the parent entry is expanded so the match is visible.
        match target {
            MatchTarget::Entry(_) => {}
            MatchTarget::Child { entry, .. } => {
                self.expand_entry(entry);
            }
        }

        // Now find the flat row index that corresponds to this target.
        let rows = self.rows();
        let abs = rows.iter().position(|r| row_matches_target(*r, target));

        if let Some(idx) = abs {
            self.cursor = idx;
        }
    }

    pub fn search_next(&mut self) {
        if self.search.matches.is_empty() {
            self.status = "No active search".to_string();
            return;
        }
        let n = self.search.matches.len();
        let next = match self.search.current {
            Some(c) => (c + 1) % n,
            None => 0,
        };
        self.search.current = Some(next);
        self.jump_to_current_match();
        self.status = format!(
            "Match {}/{} for '{}'",
            next + 1,
            n,
            self.search.committed
        );
    }

    pub fn search_prev(&mut self) {
        if self.search.matches.is_empty() {
            self.status = "No active search".to_string();
            return;
        }
        let n = self.search.matches.len();
        let prev = match self.search.current {
            Some(c) => (c + n - 1) % n,
            None => n - 1,
        };
        self.search.current = Some(prev);
        self.jump_to_current_match();
        self.status = format!(
            "Match {}/{} for '{}'",
            prev + 1,
            n,
            self.search.committed
        );
    }

    // ------------------------------------------------------------------
    // Filter modal
    // ------------------------------------------------------------------

    pub fn open_filter_modal(&mut self) {
        self.modal = Modal::Filter { selected: 0 };
    }

    pub fn close_modal(&mut self) {
        self.modal = Modal::None;
    }

    // ------------------------------------------------------------------
    // Clipboard
    // ------------------------------------------------------------------

    pub fn path_under_cursor(&self) -> Option<PathBuf> {
        let rows = self.rows();
        let row = rows.get(self.cursor)?;
        Some(match row {
            Row::Entry { index, .. } => self.entries[*index].resolved.clone(),
            Row::Child { entry, child } => {
                let e = &self.entries[*entry];
                e.resolved.join(&e.children[*child].name)
            }
            Row::Empty(i) => self.entries[*i].resolved.clone(),
        })
    }

    pub fn copy_here(&mut self) {
        let Some(p) = self.path_under_cursor() else {
            self.status = "Nothing to copy".into();
            return;
        };
        let text = display_path(&p);
        match arboard::Clipboard::new() {
            Ok(mut cb) => match cb.set_text(text.clone()) {
                Ok(_) => self.status = format!("Copied: {}", text),
                Err(e) => self.status = format!("Copy failed: {}", e),
            },
            Err(e) => self.status = format!("Clipboard unavailable: {}", e),
        }
    }

    // ------------------------------------------------------------------
    // Input dispatch
    // ------------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent, viewport_h: usize) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Ctrl-C always quits.
        if ctrl {
            if let KeyCode::Char('c') | KeyCode::Char('C') = key.code {
                self.quit = true;
                return;
            }
        }

        match self.modal {
            Modal::None if self.search.editing => self.on_key_search_editor(key, ctrl),
            Modal::None => self.on_key_tree(key, viewport_h, ctrl),
            Modal::Filter { .. } => self.on_key_filter_modal(key),
        }
    }

    fn on_key_tree(&mut self, key: KeyEvent, viewport_h: usize, ctrl: bool) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,

            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') if !ctrl => {
                self.move_down()
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') if !ctrl => self.move_up(),
            KeyCode::Char('g') if !ctrl => self.move_to_top(),
            KeyCode::Char('G') if !ctrl => self.move_to_bottom(),

            KeyCode::PageDown => self.move_page(viewport_h, true),
            KeyCode::PageUp => self.move_page(viewport_h, false),
            KeyCode::Char('d') | KeyCode::Char('D') if ctrl => {
                self.move_half_page(viewport_h, true)
            }
            KeyCode::Char('u') | KeyCode::Char('U') if ctrl => {
                self.move_half_page(viewport_h, false)
            }

            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L')
                if !ctrl =>
            {
                self.toggle_here();
            }
            KeyCode::Left | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('H')
                if !ctrl =>
            {
                self.collapse_here();
            }

            KeyCode::Char('*') | KeyCode::Char('+') if !ctrl => self.expand_all(),
            KeyCode::Char('-') | KeyCode::Char('_') if !ctrl => self.collapse_all(),

            // Search.
            KeyCode::Char('/') if !ctrl => self.start_search(),
            KeyCode::Char('n') if !ctrl => self.search_next(),
            KeyCode::Char('N') if !ctrl => self.search_prev(),

            // Filter modal.
            KeyCode::Char('o') | KeyCode::Char('O') if !ctrl => self.open_filter_modal(),

            // Copy.
            KeyCode::Char('c') | KeyCode::Char('C') if !ctrl => self.copy_here(),

            _ => {}
        }
    }

    fn on_key_search_editor(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::Enter => self.commit_search(),
            KeyCode::Esc => self.cancel_search(),
            KeyCode::Backspace => self.search_pop_char(),
            // Ctrl-H = Backspace (some terminals send it this way).
            KeyCode::Char('h') | KeyCode::Char('H') if ctrl => self.search_pop_char(),
            KeyCode::Char(ch) if !ctrl => self.search_push_char(ch),
            _ => {}
        }
    }

    fn on_key_filter_modal(&mut self, key: KeyEvent) {
        let sel = match self.modal {
            Modal::Filter { selected } => selected,
            _ => return,
        };
        let n = FILTER_OPTIONS.len();
        match key.code {
            KeyCode::Esc
            | KeyCode::Char('o')
            | KeyCode::Char('O')
            | KeyCode::Char('q')
            | KeyCode::Char('Q') => self.close_modal(),

            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                self.modal = Modal::Filter {
                    selected: (sel + n - 1) % n,
                };
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                self.modal = Modal::Filter {
                    selected: (sel + 1) % n,
                };
            }
            KeyCode::Enter | KeyCode::Char(' ') => match sel {
                0 => self.toggle_filter_duplicates(),
                1 => self.toggle_filter_overwritten(),
                2 => self.toggle_filter_missing(),
                _ => {}
            },
            KeyCode::Char('d') | KeyCode::Char('D') => self.toggle_filter_duplicates(),
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_filter_missing(),
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Mouse
    // ------------------------------------------------------------------

    pub fn on_mouse(&mut self, ev: MouseEvent, tree_area: Rect) {
        if self.modal != Modal::None || self.search.editing {
            return;
        }

        let x = ev.column as usize;
        let y = ev.row as usize;

        if x < tree_area.left() as usize
            || x >= tree_area.right() as usize
            || y < tree_area.top() as usize
            || y >= tree_area.bottom() as usize
        {
            return;
        }

        match ev.kind {
            MouseEventKind::ScrollDown => self.move_down(),
            MouseEventKind::ScrollUp => self.move_up(),
            MouseEventKind::Down(MouseButton::Left) => {
                let rel = y - tree_area.top() as usize;
                let abs = self.scroll_offset + rel;
                let max = self.row_count().saturating_sub(1);
                if abs > max {
                    return;
                }
                if abs == self.cursor {
                    self.toggle_here();
                } else {
                    self.cursor = abs;
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                let rel = y - tree_area.top() as usize;
                let abs = self.scroll_offset + rel;
                let max = self.row_count().saturating_sub(1);
                if abs > max {
                    return;
                }
                self.cursor = abs;
                self.toggle_here();
            }
            _ => {}
        }
    }
}

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

/// Does a flat `Row` correspond to the given logical `MatchTarget`?
fn row_matches_target(row: Row, target: MatchTarget) -> bool {
    match (row, target) {
        (Row::Entry { index: ia, .. }, MatchTarget::Entry(ib)) => ia == ib,
        (
            Row::Child {
                entry: ea, child: ca,
            },
            MatchTarget::Child {
                entry: eb,
                child: cb,
            },
        ) => ea == eb && ca == cb,
        _ => false,
    }
}
