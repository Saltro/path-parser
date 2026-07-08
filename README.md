# path-parser

A tiny Rust tool that visualizes your `$PATH` in an interactive **tree TUI**, so at a glance you can see:

- Which directories are in `PATH`, and in what order (**numbered** on the left)
- Which executables live in each directory
- **Which entries are shadowed** by earlier directories (same name → the earlier one wins)
- **Duplicate directories** (the same path appearing more than once)
- Copy any path to the system clipboard with one keystroke
- **Search** by name and jump through matches

```
   PATH   6 entries, 3 overwritten, 1 dupes
 1 ▸ /usr/local/bin
 2 ▾ /usr/bin                     (2 overwritten)
     ├─ python   (overwritten by /usr/local/bin/python)
     ├─ git
     └─ ssh
 3 [=] /usr/bin                   (duplicate of #2)
 4 ▸ ~/.cargo/bin
 >> ↑/k ↓/j  g top  G end  PgUp/PgDn  C-u/C-d  / search  n/N next  ↵/→ toggle  ← collapse  * all  - none  o filters  c copy  q quit
```

## Features

- 🔢 **Numbered entries** — each top-level PATH directory gets a 1-based index in a left-hand column. Children and `(empty)` markers are not numbered.
- 🌳 **Tree view** — each directory can be expanded/collapsed independently; multiple can be open at once.
- ⬆️⬇️ **Smooth navigation** — `↑/k`, `↓/j`; `g` jumps to the **top**, `G` to the **bottom**; `Enter/→/l` **toggles** (expands if collapsed, collapses if expanded); `←/Esc/h` collapses.
- 📜 **Paging** — `PageUp` / `PageDown` move by a full viewport; `Ctrl-D` / `Ctrl-U` move by half a viewport.
- 🖱️ **Mouse support** — scroll wheel moves the cursor; left-click selects; left-clicking the already-selected row (or right-clicking anywhere) toggles expand/collapse.
- 🔎 **Search** — press `/` and type a substring (case-insensitive). The input starts fresh each time. Press `Enter` to confirm: the first match is auto-jumped to and its parent directory is auto-expanded. `n` goes to the next match, `N` to the previous. Matches are highlighted with a **yellow background + bold** (original text color is preserved). Search scans **all** PATH directories and **all** their children, even collapsed ones — so you can find a file inside a directory you haven't opened yet. `Ctrl-H` in the search prompt works like Backspace. `Esc` cancels without losing your previous committed search.
- 🚩 **Overwrite detection** — when a later directory contains a name already provided by an earlier one, the later occurrence is shown in yellow with strikethrough and `(overwritten by <path>)`. The directory line shows `(N overwritten)`.
- 🆔 **Duplicate detection** — if the *same directory path* appears more than once in `$PATH`, later occurrences are shown in **red** with `[=]` and `(duplicate of #N)`. No "overwritten" fluff — it's literally the same folder.
- 📋 **One-key copy** — press `c` on any row (directory or child) and its path (`$HOME` auto-collapsed to `~`) goes to the system clipboard.
- 🏠 **`~` friendly** — display collapses `$HOME` to `~`; filesystem access expands it correctly.
- ❌ **Missing dirs** — shown in red with `[!] (missing)`.
- 🎛️ **Filter picker** — press `o` to open a small modal with three togglable options:
  - **hide duplicate directories**
  - **hide overwritten files**
  - **hide missing directories**

  Use `↑/k` `↓/j` to move, `Space` or `Enter` to toggle, `Esc` (or `o`/`q`) to close. When toggled on, `m` is a shortcut for "hide missing".
- ➖➖ **Expand/collapse all** — `*` (or `+`) expands every directory; `-` (or `_`) collapses everything.
- 🪟 **Cross-platform** — macOS (arm64 / x64), Windows x64, Linux (x64 / arm64).

## Install

### One-liner (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/Saltro/path-parser/master/install.sh | bash
```

This downloads the latest release for your platform and installs it to `~/.local/bin/path-parser`.

Options:

```bash
# Install the latest master pre-release instead of a stable version
curl -fsSL https://raw.githubusercontent.com/Saltro/path-parser/master/install.sh | bash -s -- --pre

# Install a specific version
curl -fsSL https://raw.githubusercontent.com/Saltro/path-parser/master/install.sh | bash -s -- --version v0.1.0
```

> **Windows:** The install script requires bash. Use WSL or download the `.zip` asset from [Releases](https://github.com/Saltro/path-parser/releases) directly.

### Upgrade

Once installed, update to the latest version with:

```bash
path-parser upgrade          # latest stable release
path-parser upgrade --pre    # latest master pre-release
```

### From source

```bash
git clone <this-repo>
cd path-parser
cargo build --release
./target/release/path-parser
```

### Run directly

```bash
cargo run --release
```

## Keys

| Key | Action |
| --- | --- |
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `g` | Jump to top |
| `G` | Jump to bottom |
| `PageUp` | Move up by one viewport |
| `PageDown` | Move down by one viewport |
| `Ctrl-U` | Move up by half a viewport |
| `Ctrl-D` | Move down by half a viewport |
| `Enter` / `→` / `l` | Toggle (expand if collapsed, collapse if expanded) |
| `←` / `Esc` / `h` | Collapse |
| `*` / `+` | Expand all |
| `-` / `_` | Collapse all |
| `/` | Start search (type substring, `Enter` to confirm, `Esc` to cancel) |
| `n` | Next search match |
| `N` | Previous search match |
| `c` | Copy path under cursor to clipboard |
| `o` | Open filter picker |
| `q` / `Ctrl-C` | Quit |

### Inside the filter picker (`o`)

| Key | Action |
| --- | --- |
| `↑/k` `↓/j` | Move between options |
| `Space` / `Enter` | Toggle the selected option |
| `Esc` / `o` / `q` | Close the picker |

### Mouse

| Action | Effect |
| --- | --- |
| Scroll up / down | Move cursor |
| Left-click a row | Select it |
| Left-click the selected row | Toggle expand/collapse |
| Right-click a row | Select + toggle |

## Cross-platform builds

This project depends only on `std`, `crossterm`, `ratatui`, `arboard`, and `dirs` — all pure Rust or native system bindings. **No extra system libraries are required** on macOS or Windows. On Linux the clipboard feature needs `libxcb` (X11) dev headers at build time; see note below.

### Native build

```bash
cargo build --release
```

### Cross-compiling (recommended: [`cross`](https://github.com/cross-rs/cross))

```bash
cargo install cross --git https://github.com/cross-rs/cross

# macOS arm64 (build natively on Apple Silicon)
cargo build --release --target aarch64-apple-darwin
# macOS x64
cargo build --release --target x86_64-apple-darwin

# Windows x64
cross build --release --target x86_64-pc-windows-gnu

# Linux x64 (glibc)
cross build --release --target x86_64-unknown-linux-gnu

# Linux arm64
cross build --release --target aarch64-unknown-linux-gnu
```

Artifacts land in `target/<triple>/release/path-parser` (`.exe` on Windows).

> **Linux clipboard note**: `arboard` prefers X11 (`xcb`), falls back to Wayland. In a headless / no-display environment, `c` will fail gracefully — the status bar shows `Copy failed: ...`. Everything else keeps working.

## Design notes

- **Numbering.** Only top-level PATH entries get a number (1, 2, 3, …). Children and the synthetic `(empty)` markers are left blank so the eye can scan the directory order quickly.
- **"Overwritten" semantics.** Shell command lookup walks `PATH` left-to-right; the **first** match wins. So if both `/usr/local/bin/python` and `/usr/bin/python` exist, the **latter** is effectively overwritten by the former — which is how we mark it.
- **"Duplicate" semantics.** When the *exact same resolved path* appears more than once (e.g. `/usr/bin` listed twice, or `~/bin` and `/Users/you/bin` that canonicalize to the same thing), every occurrence after the first is flagged in **red** as `(duplicate of #N)`. Duplicates don't produce extra "overwritten" noise — they point to the same directory.
- **Search.** Case-insensitive substring match against the displayed text of every visible row (entry paths and child names). Confirming a search auto-expands the parent of the first match so the row is visible. `n` / `N` wrap around. Matches are highlighted with green + underline on the matching characters.
- **Filter picker.** A small centered modal lists the two available filters. Toggling an option immediately re-renders the tree (and re-runs search matching) without closing the picker, so you can flip both before dismissing it.
- **Flat listing only.** PATH directories are usually flat; we list only direct non-directory children and do not recurse, so the tree never explodes.
- **No executable-bit check.** The executable bit on POSIX is unreliable (scripts, `sudo`, Windows has no such thing). Everything in the directory (except sub-directories) is listed so the user can decide.

## Credits

- [ratatui](https://github.com/ratatui-org/ratatui) — TUI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) — cross-platform terminal + mouse
- [arboard](https://github.com/1Password/arboard) — cross-platform clipboard
- [dirs](https://github.com/dirs-dev/dirs-rs) — `$HOME` resolution

## License

MIT
