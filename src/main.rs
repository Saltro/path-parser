//! path-parser: visualize `$PATH` in a TUI tree.

mod app;
mod path;
mod ui;

use std::io::{self, Stdout};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind, MouseEvent};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    ExecutableCommand,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use app::App;
use ui::LayoutInfo;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTALL_URL: &str =
    "https://raw.githubusercontent.com/Saltro/path-parser/master/install.sh";

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // ── CLI subcommands (run before entering the TUI) ──────────────
    if args.len() > 1 {
        match args[1].as_str() {
            "upgrade" => return cmd_upgrade(&args[2..]),
            "uninstall" => return cmd_uninstall(&args[2..]),
            "--version" | "-V" | "version" => {
                println!("path-parser v{}", VERSION);
                return Ok(());
            }
            "--help" | "-h" | "help" => {
                print_help();
                return Ok(());
            }
            other => {
                eprintln!("path-parser: unknown subcommand '{}'", other);
                eprintln!("Run 'path-parser --help' for usage.");
                std::process::exit(1);
            }
        }
    }

    run_tui()
}

/// Download and run the install script to self-upgrade.
fn cmd_upgrade(extra_args: &[String]) -> io::Result<()> {
    println!("Upgrading path-parser...");
    println!();

    // Build the bash command: curl the install script and pipe to bash,
    // forwarding any extra args (e.g. --pre).
    let mut bash_cmd = format!("curl -fsSL {} | bash -s --", INSTALL_URL);
    for arg in extra_args {
        bash_cmd.push(' ');
        bash_cmd.push_str(arg);
    }

    let status = Command::new("bash")
        .arg("-c")
        .arg(&bash_cmd)
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// Remove the installed binary.
fn cmd_uninstall(args: &[String]) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let path = exe.to_string_lossy();

    // Confirm unless --yes / -y is passed.
    if !args.iter().any(|a| a == "--yes" || a == "-y") {
        use std::io::Write;
        print!("Remove {}? [y/N] ", path);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let answer = input.trim().to_lowercase();
        if answer != "y" && answer != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    match std::fs::remove_file(&exe) {
        Ok(()) => {
            println!("Removed {}.", path);
            println!("Bye! 👋");
            Ok(())
        }
        Err(e) => {
            eprintln!("path-parser: failed to remove {}: {}", path, e);
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        "\
path-parser v{version} — visualize $PATH in an interactive tree TUI.

USAGE:
    path-parser              Launch the TUI
    path-parser upgrade      Upgrade to the latest release
    path-parser upgrade --pre   Upgrade to the latest master pre-release
    path-parser uninstall    Remove this binary
    path-parser --version    Print version and exit
    path-parser --help       Show this help

INSTALL (one-liner):
    curl -fsSL {url} | bash

KEYS (in the TUI):
    ↑/k  ↓/j        Move up / down
    g  G            Jump to top / bottom
    PgUp  PgDn      Page up / down
    Ctrl-U  Ctrl-D  Half-page up / down
    Enter / →       Toggle expand / collapse
    ← / Esc         Collapse
    *  -            Expand all / collapse all
    /               Search (type substring, Enter to confirm)
    n  N            Next / previous search match
    o               Open filter picker
    c               Copy path under cursor to clipboard
    q  Ctrl-C       Quit",
        version = VERSION,
        url = INSTALL_URL
    );
}
fn run_tui() -> io::Result<()> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut app = App::new(&path_var);

    // Setup terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let res = run_event_loop(&mut terminal, &mut app);

    // Teardown.
    disable_raw_mode()?;
    let _ = terminal.backend_mut().execute(DisableMouseCapture);
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("path-parser: {}", e);
    }
    Ok(())
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    // We need to know the tree area (for mouse hit-testing) and the
    // viewport height (for page-up/down). Both come from the most
    // recent `draw` call. Start with a sensible default; updated on
    // every frame.
    let mut layout = LayoutInfo {
        tree: Rect::default(),
        viewport_h: 20,
    };

    loop {
        let mut drawn: Option<LayoutInfo> = None;
        terminal.draw(|f| {
            let info = ui::draw(f, app);
            drawn = Some(info);
        })?;
        if let Some(info) = drawn {
            layout = info;
        }

        // Poll with a small timeout so we stay responsive to SIGWINCH etc.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => {
                // Only react to Press (and Repeat) events; ignore Release on
                // platforms that emit them (e.g. Windows).
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                app.on_key(key, layout.viewport_h);
                if app.quit {
                    break;
                }
            }
            Event::Mouse(me) => {
                handle_mouse(app, me, &layout);
                if app.quit {
                    break;
                }
            }
            Event::Resize(_, _) => {
                // ratatui re-lays-out on next draw automatically.
            }
            _ => {}
        }
    }
    Ok(())
}

fn handle_mouse(app: &mut App, ev: MouseEvent, layout: &LayoutInfo) {
    app.on_mouse(ev, layout.tree);
}
