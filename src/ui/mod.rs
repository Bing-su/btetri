mod gameplay;
mod records;
mod settings;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, MenuItem, Screen};
use crate::config::Keys;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 28;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let theme = &app.config.theme;
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.foreground)),
        frame.area(),
    );
    if frame.area().width < MIN_WIDTH || frame.area().height < MIN_HEIGHT {
        frame.render_widget(Paragraph::new(format!("Resize terminal to at least {MIN_WIDTH} x {MIN_HEIGHT}\nThe game is paused. Esc resumes after resizing."))
            .centered(), centered(frame.area(), frame.area().width, 3));
        return;
    }
    let shell = centered(
        frame.area(),
        frame.area().width.min(94),
        frame.area().height.min(30),
    );
    match app.screen {
        Screen::Menu => menu(frame, shell, app),
        Screen::Playing | Screen::Help(_) | Screen::Paused | Screen::GameOver => {
            gameplay::draw(frame, shell, app)
        }
        Screen::Records => records::draw(frame, shell, app),
        Screen::Settings => settings::draw(frame, shell, app),
    }
    if matches!(app.screen, Screen::Help(_)) {
        help(frame, shell, app);
    }
    if let Some(notice) = &app.notice
        && app.screen != Screen::GameOver
    {
        frame.render_widget(
            Paragraph::new(notice.as_str())
                .style(Style::default().fg(theme.muted))
                .centered()
                .wrap(Wrap { trim: true }),
            Rect::new(shell.x, shell.bottom().saturating_sub(5), shell.width, 2),
        );
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn header(frame: &mut Frame<'_>, area: Rect, title: &str, app: &App) {
    let theme = &app.config.theme;
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme.border))
            .title_top(
                Line::from(format!(" {title} "))
                    .centered()
                    .style(Style::default().fg(theme.foreground)),
            ),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

fn footer(frame: &mut Frame<'_>, area: Rect, lines: Vec<Line<'_>>, app: &App) {
    let height = u16::try_from(lines.len())
        .unwrap_or(area.height)
        .min(area.height);
    frame.render_widget(
        Paragraph::new(lines)
            .centered()
            .style(Style::default().fg(app.config.theme.muted)),
        Rect::new(
            area.x,
            area.bottom().saturating_sub(height),
            area.width,
            height,
        ),
    );
}

fn key_hints(bindings: &[(&str, &str)], app: &App) -> Line<'static> {
    let border = Style::default().fg(app.config.theme.border);
    let key = Style::default()
        .fg(app.config.theme.accent)
        .add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    for (index, (label, action)) in bindings.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        for (index, label) in label.split(' ').enumerate() {
            if index > 0 {
                spans.push(Span::raw(" "));
            }
            spans.extend([
                Span::styled("[", border),
                Span::styled(label.to_owned(), key),
                Span::styled("]", border),
            ]);
        }
        spans.push(Span::styled(
            format!(" {action}"),
            Style::default().fg(app.config.theme.muted),
        ));
    }
    Line::from(spans)
}

fn focused(label: &str, selected: bool, app: &App) -> Line<'static> {
    // Reserve equal widths so centered choices, e.g. RESUME / MAIN MENU, share a label column.
    Line::from(format!("{} {label:<11}", if selected { ">" } else { " " })).style(
        Style::default().fg(if selected {
            app.config.theme.accent
        } else {
            app.config.theme.foreground
        }),
    )
}

fn menu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    header(frame, area, "MAIN MENU", app);
    let form = centered(area, 42, 16);
    let player = app.name_draft.as_deref().unwrap_or(&app.config.player);
    for (index, (item, value)) in [
        (MenuItem::Player, player.to_owned()),
        (MenuItem::Mode, format!("< {} >", app.config.mode.label())),
    ]
    .into_iter()
    .enumerate()
    {
        let y = form.y + u16::try_from(index).unwrap_or_default() * 3;
        let selected = app.menu_selection == item;
        frame.render_widget(
            focused(item.label(), selected, app),
            Rect::new(form.x, y, 17, 1),
        );
        let value_area = Rect::new(form.x + 17, y, form.width - 17, 1);
        let editing = item == MenuItem::Player && app.name_draft.is_some();
        // Leave a cell after the name for the cursor; Line also keeps long Unicode names' ends visible.
        let value = if editing { format!("{value} ") } else { value };
        frame.render_widget(
            Line::from(value)
                .right_aligned()
                .style(Style::default().fg(if selected {
                    app.config.theme.accent
                } else {
                    app.config.theme.foreground
                })),
            value_area,
        );
        frame.render_widget(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(app.config.theme.border)),
            Rect::new(form.x + 2, y + 1, form.width - 2, 1),
        );
        if editing {
            frame.set_cursor_position((value_area.right().saturating_sub(1), y));
        }
    }
    for (index, item) in [
        MenuItem::Start,
        MenuItem::Records,
        MenuItem::Settings,
        MenuItem::Quit,
    ]
    .into_iter()
    .enumerate()
    {
        frame.render_widget(
            Paragraph::new(focused(item.label(), app.menu_selection == item, app)),
            Rect::new(
                form.x + 12,
                form.y + 6 + u16::try_from(index).unwrap_or_default() * 2,
                20,
                1,
            ),
        );
    }
    let hints = if app.name_draft.is_some() {
        vec![
            Line::from("Type name"),
            key_hints(&[("Backspace", "delete"), ("Delete", "clear")], app),
            key_hints(&[("Enter", "save"), ("Esc", "cancel")], app),
        ]
    } else {
        vec![
            key_hints(&[("↑ ↓", "navigate"), ("← →", "change")], app),
            key_hints(&[("Enter", "select"), ("Esc", "quit")], app),
        ]
    };
    footer(frame, area, hints, app);
}

fn help(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let area = centered(area, 54, 19);
    frame.render_widget(Clear, area);
    let keys = &app.config.keys;
    let mut lines = vec![
        Line::from("CONTROLS — PAUSED"),
        Line::from(""),
        Line::from(format!(
            "{} / {}    Move left / right",
            Keys::label(&keys.left),
            Keys::label(&keys.right)
        )),
        Line::from(format!(
            "{} / {}    Rotate counterclockwise / clockwise",
            Keys::label(&keys.counter_clockwise),
            Keys::label(&keys.clockwise)
        )),
        Line::from(format!("{}    Hard drop", Keys::label(&keys.hard_drop))),
        Line::from(format!("{}    Rotate 180", Keys::label(&keys.rotate_180))),
        Line::from(format!(
            "{}    Soft drop (hold; legacy: repeat)",
            Keys::label(&keys.soft_drop)
        )),
        Line::from(format!("{}    Hold", Keys::label(&keys.hold))),
        Line::from(format!("{}    Pause", Keys::label(&keys.pause))),
        Line::from(""),
        Line::from("40 LINES: clear 40 lines at fixed gravity"),
        Line::from("BLITZ: score within 2 minutes; gravity increases"),
        Line::from("PRACTICE: unlimited; clear board on topout"),
        Line::from(""),
    ];
    lines.push(key_hints(&[("Esc", "close")], app));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(Block::bordered())
            .style(Style::default().bg(app.config.theme.background)),
        area,
    );
}

fn format_duration(duration: Duration) -> String {
    let milliseconds = duration.as_millis();
    format!(
        "{:02}:{:02}.{:03}",
        milliseconds / 60_000,
        milliseconds / 1_000 % 60,
        milliseconds % 1_000
    )
}

#[cfg(test)]
mod tests {
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

    fn position_of(buffer: &Buffer, text: &str) -> (u16, u16) {
        let index = buffer
            .content
            .windows(text.chars().count())
            .position(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == text)
            .unwrap();
        buffer.pos_of(index)
    }

    #[rstest::rstest]
    #[case(Screen::Paused, 0, vec!["RESUME", "MAIN MENU"])]
    #[case(Screen::Paused, 1, vec!["RESUME", "MAIN MENU"])]
    #[case(Screen::GameOver, 0, vec!["PLAY AGAIN", "RECORDS", "MAIN MENU"])]
    #[case(Screen::GameOver, 1, vec!["PLAY AGAIN", "RECORDS", "MAIN MENU"])]
    #[case(Screen::GameOver, 2, vec!["PLAY AGAIN", "RECORDS", "MAIN MENU"])]
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
        assert!(
            position_of(buffer, "player must contain").1 < position_of(buffer, "Backspace").1 - 1
        );
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
}
