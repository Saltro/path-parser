# AGENTS.md

Quick-reference for AI collaborators (Claude / Cursor / Copilot / …).

## Project

`path-parser` is a small Rust TUI that displays `$PATH` as an interactive
tree, highlighting executables that are **shadowed** by an earlier entry
and directories that are **duplicated** verbatim. Top-level entries are
**numbered** on the left; a `/` search jumps to matches and auto-expands
their parents; an `o`-triggered modal lets the user toggle view filters.

## Stack

- **Language**: Rust (edition 2021)
- **TUI**: [ratatui](https://docs.rs/ratatui) 0.28
- **Terminal + mouse events**: [crossterm](https://docs.rs/crossterm) 0.28
  (cross-platform: mac / win / linux)
- **Clipboard**: [arboard](https://docs.rs/arboard) 3.4 (mac / win / linux X11+Wayland)
- **`$HOME` resolution**: [dirs](https://docs.rs/dirs) 5

## Layout

```
src/
  main.rs   # entry: raw-mode setup, event loop, teardown, mouse wiring
  app.rs    # App state machine: cursor, expanded, filters, search, modal, input
  path.rs   # PATH parsing, ~ expansion, overwrite + duplicate detection
  ui.rs     # ratatui rendering: header / number-col / tree / bottom / modal
Cargo.toml
README.md
AGENTS.md
```

## Data model

### `path::PathEntry`
```rust
pub struct PathEntry {
    pub index: usize,            // position in PATH (0 = highest priority)
    pub raw: String,             // raw string from PATH (may contain ~)
    pub resolved: PathBuf,       // absolute, ~-expanded path
    pub exists: bool,            // dir exists & is readable
    pub children: Vec<ChildEntry>,
    pub shadowed_count: usize,   // # children overwritten by an earlier entry
    pub duplicate_of: Option<usize>, // Some(idx) if resolved path == entry #idx
}
```

### `path::ChildEntry`
```rust
pub struct ChildEntry {
    pub name: String,
    pub overwritten_by: Option<PathBuf>, // Some(abs) if shadowed by this earlier path
}
```

### `app::Row` (flat visible-row list)
```rust
pub enum Row {
    Entry { index: usize, number: usize }, // top-level PATH dir; number = 1-based display idx
    Child { entry: usize, child: usize },  // expanded child file
    Empty(usize),                          // expanded-but-empty dir → "(empty)"
}
```
`number` is assigned by `App::rows()` only to `Entry` variants, in display
order (so it respects the active duplicate filter). Children and `Empty`
markers are intentionally unnumbered.

### `app::Filters`
```rust
pub struct Filters {
    pub hide_duplicates: bool,   // skip entries whose `duplicate_of.is_some()`
    pub hide_overwritten: bool,  // skip children whose `overwritten_by.is_some()`
    pub hide_missing: bool,      // skip entries that don't exist on disk
}
```

### `app::MatchTarget`
```rust
pub enum MatchTarget {
    Entry(usize),                        // matches entry #index
    Child { entry: usize, child: usize },// matches child #child of entry #entry
}
```
Logical identity of a search hit — not tied to the current flat row index
(which depends on expansion state).

### `app::Search`
```rust
pub struct Search {
    pub query: String,           // what's in the prompt right now (editing buffer)
    pub committed: String,       // last confirmed query (drives matches + highlights)
    pub editing: bool,           // prompt open?
    pub matches: Vec<MatchTarget>,
    pub current: Option<usize>,  // index into `matches`
}
```

**Search lifecycle:**
- `/` pressed → `start_search`: `query.clear()`, `editing = true`
- User types → `query` grows/shrinks (`Ctrl-H` = Backspace)
- `Enter` → `commit_search`: `committed = query`, `recompute_search_matches`,
  `jump_to_current_match` (auto-expands parent of first hit)
- `Esc` → `cancel_search`: `query = committed.clone()`, `editing = false`
  (so highlights stay as they were before opening the prompt)

**Scope:** `recompute_search_matches` iterates over **all** entries (respecting
`hide_duplicates` filter) and **all** their children (respecting
`hide_overwritten` filter), regardless of `expanded[]`. Collapsed dirs are
still searchable — jumping to a child hit auto-expands its parent.

**Highlight:** yellow background + bold, preserving the original foreground
color (so overwritten children keep their yellow strikethrough look).

### `app::Modal`
```rust
pub enum Modal { None, Filter { selected: usize } }
```
Opened by `o`; lists the three filter toggles.

`App::rows()` rebuilds the flat `Vec<Row>` from `entries` + `expanded` +
active `filters`. The cursor is an index into this vec.

## Overwrite + duplicate detection (in `path::parse`)

1. Split `$PATH` by `:` (unix) or `;` (windows).
2. For each raw entry, expand `~` and check existence.
3. **Duplicates**: maintain `HashMap<resolved_path, first_index>`. If we've
   seen this absolute path before, set `duplicate_of = Some(first_index)`.
4. Populate `children` by `read_dir` (skip subdirs). For duplicates, clone
   the children from the first occurrence so the user can still see what's
   inside when they expand the duplicate.
5. **Overwrites**: walk entries in PATH order, maintaining
   `HashMap<name, absolute_path>` of the *first* time each file name is
   seen. Every later occurrence of the same name gets
   `overwritten_by = Some(owner_abs_path)`. The *shadowed* entry's
   `shadowed_count` counts how many of its own children are overwritten.

This matches shell semantics (earlier match wins) and the user's mental
model ("later stuff is overwritten by earlier stuff").

## Rendering layout

```
┌──┬───────────────────────────────────────────────────┐
│  │ PATH  6 entries, 3 overwritten, 1 dupes           │  header (1 row, spans both cols)
├──┼───────────────────────────────────────────────────┤
│ 1│ ▸ /usr/local/bin                                  │
│ 2│ ▾ /usr/bin                  (2 overwritten)       │  tree = [number-col (4) | content]
│  │    ├─ python  (overwritten by /usr/.../python)    │
│  │    └─ git                                         │
│ 3│ [=] /usr/bin                    (duplicate of #2) │
│  │ ...                                               │
└──┴───────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────┐
│ ↑/k ↓/j  g top  G end  / search  n/N  o filters ...  │  bottom (1 row): help or /-prompt
└──────────────────────────────────────────────────────┘
        ┌─────────────────────────────┐
        │ Filters                     │  (modal, only when open)
        │ [✓] hide duplicate dirs     │
        │ [ ] hide overwritten files  │
        │ [ ] hide missing dirs       │
        │  ↑/k ↓/j  Space/Enter  Esc  │
        └─────────────────────────────┘
```

Key rendering details:

- **Number column** is a separate `ratatui::layout::Rect` to the left of
  the content area (width = `NUMBER_COL_WIDTH = 4`). It shows `"{:>2} "`
  for `Entry` rows and `"   "` for children/empty.
- **Viewport control**: `ui::draw` slices both the number-col list and
  the content list by `app.scroll_offset .. scroll_offset + viewport_h`
  and feeds them to two `ratatui::widgets::List`s with the *same*
  relative selection index. This keeps the two columns perfectly in sync.
- **Mouse hit-testing**: `LayoutInfo.tree` is the **content** rect
  (excludes the number column) so clicks land on the right row.
- **Search highlighting**: `maybe_highlight_search(text, query, base)`
  returns a `Vec<Span>` where each occurrence of `query` is rendered in
  green + bold + underline.
- **Modal**: centered, bordered, black background. Renders on top of
  everything after the main layout is drawn.

`ui::draw` returns `LayoutInfo { tree: Rect, viewport_h: usize }` so
`main.rs` can forward the tree rect to `App::on_mouse`.

## Event loop (`main.rs`)

1. `enable_raw_mode` + `EnterAlternateScreen` + `EnableMouseCapture`.
2. Loop:
   - `terminal.draw(|f| { let info = ui::draw(f, app); stash = Some(info); })`
     — ratatui's `draw` returns `CompletedFrame`, not the closure value,
     so we smuggle `LayoutInfo` out via a mutable `Option`.
   - `event::poll(250ms)` → `event::read`.
     - `Event::Key` with `kind != Release` → `app.on_key(key, viewport_h)`.
     - `Event::Mouse` → `app.on_mouse(ev, layout.tree)`.
     - `Event::Resize` → ignored (next draw relayouts automatically).
   - `app.quit` → break.
3. `disable_raw_mode` + `DisableMouseCapture` + `LeaveAlternateScreen` + `show_cursor`.

**Why skip `Release`?** Windows crossterm emits Press + Release for each
physical keypress; without filtering, one keystroke triggers two logic
updates (e.g. `j` moves by 2).

## Input dispatch

`App::on_key` routes by mode:

| Mode | Handler |
| --- | --- |
| `search.editing` | `on_key_search_editor` |
| `modal != None` | `on_key_filter_modal` (currently only filter picker exists) |
| else | `on_key_tree` |

`Ctrl-C` is intercepted at the top and always quits.

### Tree mode cheatsheet

| Input | Action |
| --- | --- |
| `q` / `Ctrl-C` | `quit = true` |
| `↑/k`, `↓/j` | `move_up` / `move_down` |
| `g` | `move_to_top` |
| `G` | `move_to_bottom` |
| `PageUp`, `PageDown` | `move_page(viewport_h, up/down)` |
| `Ctrl-U`, `Ctrl-D` | `move_half_page(viewport_h, up/down)` |
| `Enter` / `→` / `l` | `toggle_here` + `recompute_search_matches` |
| `←` / `Esc` / `h` | `collapse_here` + `recompute_search_matches` |
| `*` / `+` | `expand_all` (+ recompute) |
| `-` / `_` | `collapse_all` (+ recompute) |
| `/` | `start_search` (opens prompt) |
| `n` | `search_next` |
| `N` | `search_prev` |
| `o` | `open_filter_modal` |
| `c` | `copy_here` |

### Search editor (`/`)

| Input | Action |
| --- | --- |
| printable char | append to `search.query` |
| `Backspace` / `Ctrl-H` | pop last char |
| `Enter` | `commit_search` (close prompt + jump to first match) |
| `Esc` | `cancel_search` (close prompt, restore committed query) |

`commit_search` sets `search.committed = query`, calls `recompute_search_matches`,
and if there's a match, calls `jump_to_current_match`, which:

1. Looks up the `MatchTarget` at `search.current`.
2. If it's a `Child`, expands the parent entry.
3. Rebuilds `rows()` and finds the flat index of the matching row.
4. Sets `cursor` to that index.

`recompute_search_matches` iterates **all** entries and **all** children
(respecting filters, ignoring expansion state), so collapsed dirs are still
searchable.

### Filter modal (`o`)

| Input | Action |
| --- | --- |
| `↑/k`, `↓/j` | move selection (wraps) |
| `Space` / `Enter` | toggle the selected filter |
| `d` | shortcut for "hide duplicates" |
| `m` | shortcut for "hide missing" |
| `Esc` / `o` / `q` | close modal |

Filter options are defined in `app::FILTER_OPTIONS`:
```rust
pub const FILTER_OPTIONS: [(&str, &str); 3] = [
    ("hide duplicate directories", "d"),
    ("hide overwritten files", "o"),
    ("hide missing directories", "m"),
];
```
Toggling an option calls the corresponding `toggle_filter_*` method and
keeps the modal open so the user can flip multiple options.

### Mouse (`app.on_mouse`)

Mouse events are **ignored** while `search.editing` or a modal is open.
Otherwise, hit-test against `layout.tree` (the content rect).

| Event | Action |
| --- | --- |
| ScrollUp / ScrollDown | `move_up` / `move_down` |
| Left-click row | `cursor = abs_index`; if already selected, `toggle_here` + `recompute_search_matches` |
| Right-click row | `cursor = abs_index` + `toggle_here` + `recompute_search_matches` |

## Clipboard

`arboard::Clipboard::new()?.set_text(display_path(&p))`. Failures are
written to `app.status` (never panic). On Linux without a display server
this just means `c` shows `Copy failed: ...` — the rest of the app is
unaffected.

## Cross-platform notes

- **PATH separator**: `:` unix, `;` windows — chosen via `cfg!(windows)`
  in `split_path_var`.
- **`~` expansion**: `dirs::home_dir()` (cross-platform).
- **Executable bit**: intentionally not checked (not portable; scripts
  with shebangs are fine without +x if invoked via `python foo` etc.).
- **Mouse capture**: enabled at startup with `EnableMouseCapture`,
  disabled at teardown with `DisableMouseCapture` (best-effort, errors
  ignored).
- **Release profile**: `opt-level = "z"`, `lto = true`, `strip = true`,
  `panic = "abort"` for smallest binary.

## Cross-compiling

Use [`cross`](https://github.com/cross-rs/cross):

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-pc-windows-gnu
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

macOS targets are best built natively on Mac:
`cargo build --release --target aarch64-apple-darwin` (or `x86_64-...`).

## Tests

```bash
cargo test     # unit tests in path.rs (split, ~ round-trip, duplicates)
cargo clippy   # please run before committing
cargo build --release && ./target/release/path-parser   # manual smoke test
```

## Possible extensions

- `--path <str>` CLI flag: parse a given string instead of `$PATH`.
- `--flat` / `--all`: start with every entry expanded.
- Regex search, or search that also matches inside `(overwritten by ...)`
  annotations.
- Show file metadata: size, mtime, whether it's executable.
- A "where is `<cmd>` actually resolved from?" query (`which`-style).
- Configurable color theme.
- Persistent filter + search-history via a config file.
- Preview pane on the right showing `ls -la`-style info for the selected row.

When adding features:
1. Data / parsing → `path.rs`.
2. Interaction / state → `app.rs` (new field on `App` + branch in `on_key`
   and the right mode-specific handler).
3. Display → `ui.rs` (extend `render_*`; remember the number column and
   the modal overlay).
4. Update the key table in `README.md` and the cheatsheet above in
   `AGENTS.md`.
