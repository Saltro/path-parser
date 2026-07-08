//! path-parser: visualize `$PATH` in a TUI tree.

mod app;
mod path;
mod ui;

use std::io::{self, Stdout};
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

fn main() -> io::Result<()> {
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
