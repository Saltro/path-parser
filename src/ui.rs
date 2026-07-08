//! TUI rendering.

use crate::app::{App, FILTER_OPTIONS, Modal, Row};
use crate::path::display_path;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Widget};
use ratatui::Frame;

const COLOR_HEADER: Color = Color::Cyan;
const COLOR_PATH: Color = Color::White;
const COLOR_CHILD: Color = Color::White;
const COLOR_OVERWRITTEN: Color = Color::Yellow;
const COLOR_DUPLICATE: Color = Color::Red;
const COLOR_HIGHLIGHT: Color = Color::LightBlue;
const COLOR_STATUS: Color = Color::Gray;
const COLOR_MARKER: Color = Color::LightMagenta;
const COLOR_NUMBER: Color = Color::LightBlue;
const COLOR_MODAL_BG: Color = Color::Black;

/// Width of the left-side number column (e.g. " 1 ").
const NUMBER_COL_WIDTH: u16 = 4;

/// Layout info returned from `draw` so main.rs can forward the tree
/// viewport rect to the mouse handler.
pub struct LayoutInfo {
    pub tree: Rect,
    pub viewport_h: usize,
}

pub fn draw(f: &mut Frame, app: &mut App) -> LayoutInfo {
    let area = f.area();
    let [body, bottom] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
    let [header, tree] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(body);

    // --- Header line ---
    let total_overwritten: usize = app.entries.iter().map(|e| e.shadowed_count).sum();
    let total_duplicates: usize = app.entries.iter().filter(|e| e.duplicate_of.is_some()).count();

    let header_line = Line::from(vec![
        Span::raw(" ".repeat(NUMBER_COL_WIDTH as usize)), // pad over number column
        Span::styled(
            "PATH",
            Style::default()
                .fg(COLOR_HEADER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{} entries, {} overwritten, {} dupes",
                app.entries.len(),
                total_overwritten,
                total_duplicates
            ),
            Style::default().fg(COLOR_STATUS),
        ),
    ]);
    SingleLine(header_line).render(header, f.buffer_mut());

    // --- Tree body (with number column on the left) ---
    // Split tree area into [number col | content].
    let [num_col, content] = Layout::horizontal([
        Constraint::Length(NUMBER_COL_WIDTH),
        Constraint::Min(10),
    ])
    .areas(tree);

    let viewport_h = tree.height as usize;
    app.ensure_cursor_visible(viewport_h);
    let offset = app.scroll_offset;

    // Build content items and matching number-column items.
    let all_rows = app.rows();
    let (content_items, num_items): (Vec<ListItem>, Vec<ListItem>) = all_rows
        .iter()
        .map(|&r| (render_content_row(app, r), render_number_row(app, r)))
        .unzip();

    let visible_content: Vec<ListItem> = content_items
        .into_iter()
        .skip(offset)
        .take(viewport_h)
        .collect();
    let visible_nums: Vec<ListItem> = num_items
        .into_iter()
        .skip(offset)
        .take(viewport_h)
        .collect();

    let selected_rel = if app.cursor >= offset {
        Some(app.cursor - offset)
    } else {
        None
    };

    // Render number column (no highlight symbol, but dim the bg on selected
    // row for visual continuity).
    let num_list = List::new(visible_nums).highlight_style(
        Style::default()
            .fg(COLOR_HIGHLIGHT)
            .add_modifier(Modifier::BOLD),
    );
    let mut num_state = ListState::default();
    num_state.select(selected_rel);
    f.render_stateful_widget(num_list, num_col, &mut num_state);

    // Render content.
    let list = List::new(visible_content)
        .highlight_style(
            Style::default()
                .fg(COLOR_HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    let mut state = ListState::default();
    state.select(selected_rel);
    f.render_stateful_widget(list, content, &mut state);

    // --- Bottom line: help / search prompt / status ---
    render_bottom(bottom, f.buffer_mut(), app);

    // --- Modal overlay (filter picker) ---
    if let Modal::Filter { selected } = app.modal {
        render_filter_modal(f, app, selected);
    }

    LayoutInfo {
        tree: content, // mouse hit-tests against the content area
        viewport_h,
    }
}

// ----------------------------------------------------------------------
// Bottom line (help or search prompt)
// ----------------------------------------------------------------------

fn render_bottom(area: Rect, buf: &mut Buffer, app: &App) {
    if app.search.editing {
        let line = Line::from(vec![
            Span::styled(
                "/",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.search.query.clone(),
                Style::default().fg(Color::White),
            ),
            Span::styled("█", Style::default().fg(Color::White)), // cursor
            Span::styled(
                "  [Enter confirm, Esc cancel]",
                Style::default().fg(COLOR_STATUS),
            ),
        ]);
        SingleLine(line).render(area, buf);
        return;
    }

    let help = "↑/k ↓/j  g top  G end  PgUp/PgDn  C-u/C-d  / search  n/N next  ↵/→ toggle  ← collapse  * all  - none  o filters  c copy  q quit";
    let line = if app.status.is_empty() {
        Line::from(Span::styled(help, Style::default().fg(COLOR_STATUS)))
    } else {
        Line::from(vec![
            Span::styled(help, Style::default().fg(COLOR_STATUS)),
            Span::raw("    "),
            Span::styled(app.status.clone(), Style::default().fg(Color::White)),
        ])
    };
    SingleLine(line).render(area, buf);
}

// ----------------------------------------------------------------------
// Filter modal
// ----------------------------------------------------------------------

fn render_filter_modal(f: &mut Frame, app: &App, selected: usize) {
    let area = f.area();

    // Modal dimensions.
    let width = 46u16.min(area.width.saturating_sub(4));
    let height = (FILTER_OPTIONS.len() as u16 + 4).min(area.height.saturating_sub(4));

    let x = area.left() + (area.width.saturating_sub(width)) / 2;
    let y = area.top() + (area.height.saturating_sub(height)) / 2;
    let modal = Rect::new(x, y, width, height);

    // Background block.
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Filters ")
        .style(Style::default().bg(COLOR_MODAL_BG).fg(Color::White));
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    // Build option lines.
    let mut items: Vec<ListItem> = Vec::new();
    for (i, (label, _key)) in FILTER_OPTIONS.iter().enumerate() {
        let on = match i {
            0 => app.filters.hide_duplicates,
            1 => app.filters.hide_overwritten,
            2 => app.filters.hide_missing,
            _ => false,
        };
        let mark = if on { "✓" } else { " " };
        let style = if i == selected {
            Style::default()
                .fg(COLOR_HIGHLIGHT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let line = Line::from(vec![
            Span::styled(" [", Style::default().fg(Color::White)),
            Span::styled(mark.to_string(), Style::default().fg(Color::Green)),
            Span::styled("] ", Style::default().fg(Color::White)),
            Span::styled((*label).to_string(), style),
        ]);
        items.push(ListItem::new(line));
    }
    items.push(ListItem::new(Line::from(Span::styled(
        "  ↑/k ↓/j move   Space/Enter toggle   Esc close",
        Style::default().fg(COLOR_STATUS),
    ))));

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(COLOR_HIGHLIGHT)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, inner, &mut state);
}

// ----------------------------------------------------------------------
// Row rendering
// ----------------------------------------------------------------------

/// Render the number-column cell for a row (blank for non-entry rows).
fn render_number_row(_app: &App, row: Row) -> ListItem<'static> {
    match row {
        Row::Entry { number, .. } => ListItem::new(Line::from(Span::styled(
            format!("{:>2} ", number),
            Style::default()
                .fg(COLOR_NUMBER)
                .add_modifier(Modifier::BOLD),
        ))),
        Row::Child { .. } | Row::Empty(_) => {
            ListItem::new(Line::from(Span::raw("   ")))
        }
    }
}

/// Render the main content for a row.
fn render_content_row(app: &App, row: Row) -> ListItem<'static> {
    match row {
        Row::Entry { index, .. } => render_entry(app, index),
        Row::Child { entry, child } => render_child(app, entry, child),
        Row::Empty(i) => render_empty(i),
    }
}

fn render_entry(app: &App, i: usize) -> ListItem<'static> {
    let e = &app.entries[i];
    let expanded = app.expanded.get(i).copied().unwrap_or(false);

    let marker = if e.duplicate_of.is_some() {
        Span::styled("[=] ", Style::default().fg(COLOR_DUPLICATE))
    } else if !e.exists {
        Span::styled("[!] ", Style::default().fg(Color::Red))
    } else if expanded {
        Span::styled("▾ ", Style::default().fg(COLOR_MARKER))
    } else {
        Span::styled("▸ ", Style::default().fg(COLOR_MARKER))
    };

    let path_str = display_path(&e.resolved);
    let path_style = if e.duplicate_of.is_some() {
        Style::default()
            .fg(COLOR_DUPLICATE)
            .add_modifier(Modifier::BOLD)
    } else if !e.exists {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(COLOR_PATH)
    };
    let path_span = maybe_highlight_search(
        &path_str,
        &app.search.committed,
        path_style,
        true,
    );

    let mut spans = vec![marker];
    spans.extend(path_span);

    if let Some(orig) = e.duplicate_of {
        spans.push(Span::styled(
            format!("  (duplicate of #{})", orig + 1),
            Style::default()
                .fg(COLOR_DUPLICATE)
                .add_modifier(Modifier::ITALIC),
        ));
    } else if !e.exists {
        spans.push(Span::styled(
            "  (missing)".to_string(),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::ITALIC),
        ));
    } else if e.shadowed_count > 0 {
        spans.push(Span::styled(
            format!("  ({} overwritten)", e.shadowed_count),
            Style::default()
                .fg(COLOR_OVERWRITTEN)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn render_child(app: &App, entry: usize, child: usize) -> ListItem<'static> {
    let e = &app.entries[entry];
    let c = &e.children[child];

    let is_last = child + 1 == e.children.len();
    let branch = if is_last { "└─ " } else { "├─ " };

    let base_name_style = if c.overwritten_by.is_some() {
        Style::default()
            .fg(COLOR_OVERWRITTEN)
            .add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(COLOR_CHILD)
    };

    let mut spans = vec![
        Span::raw("   "),
        Span::styled(branch.to_string(), Style::default().fg(COLOR_STATUS)),
    ];
    spans.extend(maybe_highlight_search(
        &c.name,
        &app.search.committed,
        base_name_style,
        true,
    ));

    if let Some(by) = &c.overwritten_by {
        spans.push(Span::styled(
            format!("  (overwritten by {})", display_path(by)),
            Style::default()
                .fg(COLOR_OVERWRITTEN)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn render_empty(_entry: usize) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        "   └─ (empty)",
        Style::default()
            .fg(COLOR_STATUS)
            .add_modifier(Modifier::ITALIC),
    )))
}

/// If `query` is non-empty and appears in `text`, return a vec of spans
/// that highlights each occurrence with a **yellow background + bold**
/// while preserving the original foreground color. Otherwise returns a
/// single span styled with `base`.
fn maybe_highlight_search(
    text: &str,
    query: &str,
    base: Style,
    _whole: bool,
) -> Vec<Span<'static>> {
    if query.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let q = query.to_lowercase();
    let t_lower = text.to_lowercase();
    let text_chars: Vec<char> = text.chars().collect();
    let lower_chars: Vec<char> = t_lower.chars().collect();
    let qlen = q.chars().count();
    if qlen == 0 {
        return vec![Span::styled(text.to_string(), base)];
    }

    // Highlight style: keep original foreground, add bold, yellow bg.
    let hl = base
        .add_modifier(Modifier::BOLD)
        .bg(Color::Yellow);

    let mut out: Vec<Span> = Vec::new();
    let mut cursor = 0;
    let mut i = 0;
    while i + qlen <= lower_chars.len() {
        if lower_chars[i..i + qlen].iter().collect::<String>() == q {
            // Push the prefix [cursor..i) as base.
            if cursor < i {
                let s: String = text_chars[cursor..i].iter().collect();
                out.push(Span::styled(s, base));
            }
            // Push the match highlighted.
            let s: String = text_chars[i..i + qlen].iter().collect();
            out.push(Span::styled(s, hl));
            i += qlen;
            cursor = i;
        } else {
            i += 1;
        }
    }
    if out.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    // Tail.
    if cursor < text_chars.len() {
        let s: String = text_chars[cursor..].iter().collect();
        out.push(Span::styled(s, base));
    }
    out
}

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

/// Renders a single `Line` into a rect, clearing the row first.
struct SingleLine<'a>(Line<'a>);
impl<'a> Widget for SingleLine<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, area.top())) {
                cell.reset();
            }
        }
        let mut col = area.left();
        for span in self.0.spans {
            for ch in span.content.chars() {
                if col >= area.right() {
                    break;
                }
                if let Some(cell) = buf.cell_mut((col, area.top())) {
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(span.style);
                }
                col += 1;
            }
        }
    }
}
