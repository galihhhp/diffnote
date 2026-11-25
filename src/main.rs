mod app;
mod comments;
mod config;
mod diff;
mod git;
mod scripts;
mod transport;
mod ui;

use anyhow::Result;
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn main() -> Result<()> {
    let mut repo = PathBuf::from(
        std::env::var("DIFFNOTE_REPO").unwrap_or_else(|_| ".".to_string()),
    );
    let mut session: Option<String> = None;
    let mut block: Option<String> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index + 1 < args.len() {
        match args[index].as_str() {
            "--repo" => repo = PathBuf::from(&args[index + 1]),
            "--session" => session = Some(args[index + 1].clone()),
            "--block" => block = Some(args[index + 1].clone()),
            _ => {}
        }
        index += 2;
    }
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableBracketedPaste)?;
    let _ = execute!(
        std::io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    let result = run(&mut terminal, &repo, session, block);
    let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    let _ = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    repo: &Path,
    session: Option<String>,
    block: Option<String>,
) -> Result<()> {
    let mut app = app::App::new(repo, session, block)?;
    while !app.should_quit {
        app.poll_editor();
        app.poll_tree();
        terminal.draw(|frame| ui::draw(frame, &app))?;
        if app.mode == app::Mode::Loading {
            app.execute_pending();
            continue;
        }
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                        app.handle_key(key);
                    }
                }
                Event::Paste(text) => app.handle_paste(text),
                _ => {}
            }
        }
    }
    Ok(())
}
