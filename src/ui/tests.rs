use ratatui::Terminal;
use ratatui::backend::{Backend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

use super::*;
use crate::app::{HelpReturn, SaveState};
use crate::config::Config;
use crate::game::Mode;

fn screen(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

/// Locate visible text in a test buffer, failing when the expected text is absent.
pub(super) fn position_of(buffer: &Buffer, text: &str) -> (u16, u16) {
    let index = buffer
        .content
        .windows(text.chars().count())
        .position(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == text)
        .unwrap();
    buffer.pos_of(index)
}

#[rstest::rstest]
fn menu_choices_are_centered_and_share_a_column(
    #[values((80, 28), (81, 29), (100, 40))] size: (u16, u16),
) {
    let app = App::try_from(Config::default()).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let x = position_of(buffer, "START GAME").0;
    for label in ["RECORDS", "SETTINGS", "QUIT"] {
        assert_eq!(position_of(buffer, label).0, x);
    }
    let width = u16::try_from(focused("START GAME", true, &app).width()).unwrap();
    assert!((x - 2).abs_diff(size.0 - (x - 2 + width)) <= 1);
}

#[rstest::rstest]
fn help_controls_share_key_and_description_columns(
    #[values((80, 28), (100, 40))] size: (u16, u16),
    #[values(false, true)] wide_keys: bool,
) {
    let mut app = App::try_from(Config::default()).unwrap();
    if wide_keys {
        app.config.keys.left = vec!["pageup".parse().unwrap()];
        app.config.keys.right = vec!["pagedown".parse().unwrap()];
    }
    app.start();
    app.screen = Screen::Help(HelpReturn::Playing);
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let x = position_of(buffer, "Move left / right").0;
    for description in [
        "Rotate counterclockwise / clockwise",
        "Hard drop",
        "Rotate 180",
        "Soft drop (hold; legacy: repeat)",
        "Hold",
        "Pause",
    ] {
        assert_eq!(position_of(buffer, description).0, x);
    }
    for key in ["SPACE", "DOWN", "ESC"] {
        let (key_x, _) = position_of(buffer, key);
        assert_eq!(key_x + u16::try_from(key.len()).unwrap() + 4, x);
    }
}

#[rstest::rstest]
fn result_summary_labels_and_values_share_columns(
    #[values((80, 28), (100, 40))] size: (u16, u16),
    #[values(0, 60_000)] elapsed_seconds: u64,
) {
    let mut app = App::try_from(Config {
        mode: Mode::Practice,
        ..Config::default()
    })
    .unwrap();
    app.start();
    app.game
        .as_mut()
        .unwrap()
        .advance(Duration::from_secs(elapsed_seconds));
    app.screen = Screen::GameOver;
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let x = position_of(buffer, "SCORE 000000").0;
    for text in [
        "LINES 0".to_owned(),
        format!(
            "TIME  {}",
            format_duration(Duration::from_secs(elapsed_seconds))
        ),
    ] {
        assert_eq!(position_of(buffer, &text).0, x);
    }
    assert_eq!(
        buffer[(x + 6, position_of(buffer, "LINES 0").1)].symbol(),
        "0"
    );
}

#[rstest::rstest]
fn record_headings_align_with_values(#[values((80, 28), (100, 40))] size: (u16, u16)) {
    let mut app = App::try_from(Config {
        mode: Mode::Blitz,
        ..Config::default()
    })
    .unwrap();
    app.start();
    let mut record =
        crate::storage::Record::from_game(app.game.as_ref().unwrap(), "Alice").unwrap();
    record.score = 123456;
    record.level = 7;
    record.lines = 123;
    record.elapsed_ms = 12345;
    app.open_records(Mode::Blitz);
    app.records = crate::app::RecordsState::Loaded(vec![record]);
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(
        position_of(buffer, "PLAYER").0,
        position_of(buffer, "Alice").0
    );
    for (heading, value) in [
        ("#", "1"),
        ("SCORE", "123456"),
        ("LV", "07"),
        ("LINES", "123"),
        ("TIME", "00:12.345"),
    ] {
        let (x, y) = position_of(buffer, heading);
        let start = x + u16::try_from(heading.len()).unwrap() - u16::try_from(value.len()).unwrap();
        let rendered: String = (0..u16::try_from(value.len()).unwrap())
            .map(|offset| buffer[(start + offset, y + 2)].symbol())
            .collect();
        assert_eq!(rendered, value, "{heading} is misaligned");
    }
}

#[rstest::rstest]
fn marathon_and_help_fit_the_terminal(#[values((80, 28), (100, 40))] size: (u16, u16)) {
    let mut app = App::try_from(Config {
        mode: Mode::Marathon,
        ..Config::default()
    })
    .unwrap();
    assert!(screen(&app, size.0, size.1).contains("MARATHON"));
    app.start();
    let text = screen(&app, size.0, size.1);
    assert!(text.contains("MARATHON"), "{text}");
    assert!(text.contains("LEVEL"), "{text}");
    app.screen = Screen::Help(HelpReturn::Playing);
    let text = screen(&app, size.0, size.1);
    for expected in [
        "MARATHON: survive",
        "max 20",
        "PRACTICE: unlimited",
        "close",
    ] {
        assert!(text.contains(expected), "{text}");
    }
}

#[rstest::rstest]
#[case(Screen::Paused, 0, vec!["RESUME", "MAIN MENU"])]
#[case(Screen::Paused, 1, vec!["RESUME", "MAIN MENU"])]
#[case(Screen::GameOver, 0, vec!["PLAY AGAIN", "STATISTICS", "RECORDS", "MAIN MENU"])]
#[case(Screen::GameOver, 1, vec!["PLAY AGAIN", "STATISTICS", "RECORDS", "MAIN MENU"])]
#[case(Screen::GameOver, 2, vec!["PLAY AGAIN", "STATISTICS", "RECORDS", "MAIN MENU"])]
#[case(Screen::GameOver, 3, vec!["PLAY AGAIN", "STATISTICS", "RECORDS", "MAIN MENU"])]
fn dialog_choices_share_a_label_column(
    #[case] screen: Screen,
    #[case] selected: usize,
    #[case] labels: Vec<&str>,
    #[values((80, 28), (100, 40))] size: (u16, u16),
) {
    let mut app = App::try_from(Config::default()).unwrap();
    app.start();
    app.screen = screen;
    app.save = SaveState::Saved;
    app.dialog_selection = selected;
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let column = position_of(buffer, labels.first().unwrap()).0;
    for (index, label) in labels.iter().enumerate() {
        let (x, y) = position_of(buffer, label);
        assert_eq!(x, column, "{label} is misaligned");
        assert_eq!(
            buffer[(x - 2, y)].symbol(),
            if index == selected { ">" } else { " " }
        );
    }
}

#[rstest::rstest]
#[case("", None)]
#[case("Alice", Some(("e", 1)))]
#[case("가나다", Some(("다", 2)))]
#[case("가나다라마바사아자차카타파하가끝", Some(("끝", 2)))]
#[case("🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀", Some(("🦀", 2)))]
#[case("e\u{301}", Some(("e\u{301}", 1)))]
fn name_cursor_follows_the_visible_text(
    #[case] name: &str,
    #[case] last: Option<(&str, u16)>,
    #[values((80, 28), (100, 40))] size: (u16, u16),
) {
    let mut app = App::try_from(Config::default()).unwrap();
    app.name_draft = Some(name.into());
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let cursor = terminal.backend_mut().get_cursor_position().unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[cursor].symbol(), " ");
    if let Some((symbol, width)) = last {
        assert_eq!(buffer[(cursor.x - width, cursor.y)].symbol(), symbol);
    }
}

#[rstest::rstest]
#[case(Screen::Menu, false, "Enter", "select")]
#[case(Screen::Menu, false, "↑", "navigate")]
#[case(Screen::Menu, true, "Backspace", "delete")]
#[case(Screen::Playing, false, "SPACE", "hard drop")]
#[case(Screen::Paused, false, "Enter", "select")]
#[case(Screen::GameOver, false, "Enter", "select")]
#[case(Screen::Records, false, "Esc", "back")]
#[case(Screen::Help(HelpReturn::Playing), false, "Esc", "close")]
fn footer_distinguishes_keys_from_actions(
    #[case] screen: Screen,
    #[case] editing: bool,
    #[case] key: &str,
    #[case] action: &str,
    #[values((80, 28), (100, 40))] size: (u16, u16),
) {
    let mut app = App::try_from(Config::default()).unwrap();
    app.start();
    app.screen = screen;
    if editing {
        app.name_draft = Some("Alice".into());
    }
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let (x, y) = position_of(buffer, key);
    let width = u16::try_from(Line::from(key).width()).unwrap();
    for (column, symbol) in [(x - 1, "["), (x + width, "]")] {
        let border = &buffer[(column, y)];
        assert_eq!(border.symbol(), symbol);
        assert_eq!(border.fg, app.config.theme.border);
    }
    let key = &buffer[(x, y)];
    let (action_x, action_y) = position_of(buffer, action);
    assert_eq!(action_y, y);
    let action = &buffer[(action_x, action_y)];
    assert_eq!(key.fg, app.config.theme.accent);
    assert!(key.modifier.contains(Modifier::BOLD));
    assert_eq!(action.fg, app.config.theme.muted);
    assert!(!action.modifier.contains(Modifier::BOLD));
}

#[rstest::rstest]
#[case(MenuItem::Player, "PLAYER")]
#[case(MenuItem::Mode, "MODE")]
#[case(MenuItem::Start, "START GAME")]
#[case(MenuItem::Records, "RECORDS")]
#[case(MenuItem::Settings, "SETTINGS")]
#[case(MenuItem::Quit, "QUIT")]
fn menu_highlights_the_selected_item(#[case] item: MenuItem, #[case] label: &str) {
    let mut app = App::try_from(Config::default()).unwrap();
    app.menu_selection = item;
    let text = screen(&app, 80, 28);
    assert!(text.contains("MAIN MENU"));
    assert!(text.contains("40 LINES"));
    assert!(text.contains(&format!("> {label}")));
}

#[rstest::rstest]
#[case(Screen::Playing, SaveState::None, &["40 LINES", "HOLD", "NEXT"])]
#[case(Screen::GameOver, SaveState::Pending, &["Saving record"])]
#[case(Screen::GameOver, SaveState::Failed("Database unavailable".into()), &["Save failed: R retry"])]
#[case(Screen::Records, SaveState::None, &["No records"])]
fn gameplay_and_record_states_render(
    #[case] state: Screen,
    #[case] save: SaveState,
    #[case] labels: &[&str],
) {
    let mut app = App::try_from(Config::default()).unwrap();
    app.start();
    app.screen = state;
    app.save = save;
    let text = screen(&app, 80, 28);
    for label in labels {
        assert!(text.contains(label), "missing {label}");
    }
    assert!(!text.contains("Record saved"));
}

#[rstest::rstest]
#[case(40, 10)]
#[case(80, 27)]
#[case(79, 28)]
fn tiny_terminal_shows_resize_message(#[case] width: u16, #[case] height: u16) {
    let app = App::try_from(Config::default()).unwrap();
    assert!(screen(&app, width, height).contains("Resize terminal"));
}

#[test]
fn name_validation_notice_stays_above_the_key_hints() {
    let mut app = App::try_from(Config::default()).unwrap();
    app.name_draft = Some(String::new());
    app.notice = Some("player must contain 1–16 printable characters".into());
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    assert!(position_of(buffer, "player must contain").1 < position_of(buffer, "Backspace").1 - 1);
    let (x, y) = position_of(buffer, "Backspace");
    assert_eq!(buffer[(x - 1, y)].symbol(), "[");
    assert_eq!(buffer[(x + 9, y)].symbol(), "]");
}

#[rstest::rstest]
#[case(80, 28)]
#[case(100, 40)]
fn save_error_stays_above_the_key_hints(#[case] width: u16, #[case] height: u16) {
    let mut app = App::try_from(Config::default()).unwrap();
    app.start();
    app.screen = Screen::GameOver;
    app.save = SaveState::Failed("Database unavailable".into());
    app.key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let (x, y) = position_of(buffer, "Enter");
    let error_y = position_of(buffer, "Database unavailable").1;
    assert!(position_of(buffer, "Keep this result").1 < error_y);
    assert!(screen(&app, width, height).contains("save."));
    assert!(error_y < y);
    assert_eq!(buffer[(x - 1, y)].symbol(), "[");
    assert_eq!(buffer[(x + 5, y)].symbol(), "]");
}

#[test]
fn selected_record_remains_visible_at_minimum_size() {
    let mut app = App::try_from(Config::default()).unwrap();
    app.start();
    let base = crate::storage::Record::from_game(app.game.as_ref().unwrap(), "PLAYER").unwrap();
    app.open_records(Mode::FortyLines);
    app.records = crate::app::RecordsState::Loaded(
        (0..20)
            .map(|index| crate::storage::Record {
                id: index.to_string(),
                player: format!("PLAYER{index:02}"),
                ..base.clone()
            })
            .collect(),
    );
    app.record_selection = 19;
    let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("PLAYER19"));
    assert!(text.contains("DATE"));
    assert!(text.contains("20 records"));
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.symbol() == "P" && cell.fg == app.config.theme.accent)
    );
}
