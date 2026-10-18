use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use crossterm::event::{
    self,
    DisableFocusChange,
    EnableFocusChange,
    Event,
    KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use miette::{IntoDiagnostic, WrapErr};

use crate::app::{App, Screen};
use crate::cli::{Command, Options};
use crate::config::{Config, Paths};
use crate::storage::Store;
use crate::ui;

pub fn run() -> miette::Result<()> {
    let options = Options::parse();
    if let Some(Command::Records(records)) = &options.command
        && records.mode == Some(crate::game::Mode::Practice)
    {
        return Err(miette::miette!(
            "practice has no ranked records; choose 40l or blitz"
        ));
    }
    let required_config = options.config.is_some();
    let paths = Paths::resolve(options.config, options.db)?;
    let mut config = Config::load(&paths.config, required_config)?;
    if let Some(Command::Play(play)) = &options.command {
        config.mode = play.mode;
        if let Some(name) = &play.name {
            config.player.clone_from(name);
        }
    }
    let mut app = App::try_from(config)?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(miette::miette!(
            "interactive terminal required; use --help for command-line options"
        ));
    }
    let store = Store::open(&paths.db)?;
    match options.command {
        Some(Command::Play(_)) => app.start(),
        Some(Command::Records(records)) => app.open_records(records.mode.unwrap_or_default()),
        None => {}
    }
    // Restore raw mode even if initialization fails after partially changing the terminal.
    scopeguard::defer!(ratatui::restore());
    let mut terminal = ratatui::try_init()
        .into_diagnostic()
        .wrap_err("cannot initialize terminal")?;
    let enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    scopeguard::defer! {
        // Cleanup is best effort after a disconnect, e.g. stdout is already closed.
        if enhanced {
            drop(execute!(io::stdout(), PopKeyboardEnhancementFlags));
        }
        drop(execute!(io::stdout(), DisableFocusChange));
    }
    execute!(io::stdout(), EnableFocusChange).into_diagnostic()?;
    if enhanced {
        execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    // Preserve typed Shift characters, e.g. '?' for help and uppercase names.
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        )
        .into_diagnostic()?;
    }
    app.keyboard_releases = enhanced || cfg!(windows);
    let result = event_loop(&mut terminal, &mut app, &store);
    app.finish_session(&store, result)
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    store: &Store,
) -> miette::Result<()> {
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        app.advance(now.saturating_duration_since(last))?;
        last = now;
        app.process_storage(store);
        // Database waits happen outside play; exclude them from the next game's elapsed time.
        if app.screen != Screen::Playing {
            last = Instant::now();
        }
        if app.quit {
            return Ok(());
        }
        let size = terminal.size().into_diagnostic()?;
        if size.width < ui::MIN_WIDTH || size.height < ui::MIN_HEIGHT {
            app.pause_for_resize();
        }
        terminal
            .draw(|frame| ui::draw(frame, app))
            .into_diagnostic()?;
        let timeout = Duration::from_millis(16).saturating_sub(last.elapsed());
        if event::poll(timeout)
            .into_diagnostic()
            .wrap_err("terminal input failed")?
        {
            match event::read()
                .into_diagnostic()
                .wrap_err("terminal input failed")?
            {
                Event::Key(key) => app.key(key)?,
                Event::FocusLost => app.pause_for_resize(),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::app::SaveState;
    use crate::game::Mode;

    #[rstest::rstest]
    #[case(false)]
    #[case(true)]
    fn shutdown_reports_unsaved_result_even_after_terminal_failure(#[case] terminal_failed: bool) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        let record = app.record.clone().unwrap();
        // Fail the final save, e.g. after a disconnect prevents an in-app retry.
        db.execute("DROP TABLE game_records", []).unwrap();
        let result = if terminal_failed {
            Err(miette::miette!("terminal input failed"))
        } else {
            Ok(())
        };

        let error = app.finish_session(&store, result).unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("Unsaved result:"), "{diagnostic}");
        assert!(diagnostic.contains(&record.id), "{diagnostic}");
        assert!(
            diagnostic.contains("cannot save completed game"),
            "{diagnostic}"
        );
        if terminal_failed {
            assert!(diagnostic.contains("terminal input failed"), "{diagnostic}");
        }
        assert_eq!(app.record.as_ref(), Some(&record));
    }

    #[rstest::rstest]
    #[case(false)]
    #[case(true)]
    fn pending_save_finishes_before_quit_and_records_load_synchronously(
        #[case] terminal_failed: bool,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("records.db")).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        app.quit = true;
        let result = app.finish_session(
            &store,
            if terminal_failed {
                Err(miette::miette!("terminal input failed"))
            } else {
                Ok(())
            },
        );
        if terminal_failed {
            assert_eq!(result.unwrap_err().to_string(), "terminal input failed");
        } else {
            result.unwrap();
        }
        assert!(matches!(app.save, SaveState::Saved));
        assert_eq!(
            store.records(Mode::Blitz).unwrap(),
            [app.record.clone().unwrap()]
        );
        app.open_records(Mode::Blitz);
        app.process_storage(&store);
        assert_eq!(app.records.rows(), [app.record.clone().unwrap()]);
    }

    #[test]
    fn save_failure_preserves_result_for_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        let record = app.record.clone().unwrap();
        // Simulate a storage failure without a timing-dependent disk or lock fixture.
        db.execute("DROP TABLE game_records", []).unwrap();
        app.process_storage(&store);
        assert!(matches!(app.save, SaveState::Failed(_)));
        assert_eq!(app.record.as_ref(), Some(&record));
        db.execute_batch(include_str!("../migrations/001_records.sql"))
            .unwrap();
        app.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
            .unwrap();
        app.process_storage(&store);
        assert!(matches!(app.save, SaveState::Saved));
        assert_eq!(store.records(Mode::Blitz).unwrap(), [record]);
    }
}
