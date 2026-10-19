use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Gauge, Paragraph, Wrap};

use super::{footer, header, key_hints};
use crate::app::{App, Edit, SaveState, Settings, rgb};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(settings) = &app.settings else {
        return;
    };
    header(frame, area, "SETTINGS", app);
    let mut entries = settings.config.entries();
    entries.extend([
        ("SAVE", String::new()),
        ("RESET DEFAULTS", String::new()),
        ("CANCEL", String::new()),
    ]);
    let field = entries.get(settings.selected).map_or("", |(name, _)| *name);
    let hint = if matches!(field, "das_ms" | "arr_ms" | "soft_drop_ms") && !app.keyboard_releases {
        "Terminal has no key releases; OS key repeat applies."
    } else if field == "das_ms" {
        "Delay before horizontal repeat. Left/Right: 10 ms."
    } else if field == "arr_ms" {
        "Horizontal repeat interval; 0 = move to wall after DAS."
    } else if field == "soft_drop_ms" {
        "Drop interval without DAS; 0 = drop to floor without locking."
    } else if field == "practice_speed_percent" {
        "Practice gravity only; 100% = normal. Left/Right: 10%."
    } else if field.starts_with("keys.") {
        "Enter opens key bindings; press a key to assign it"
    } else if field.starts_with("theme.") {
        "Enter opens palette / RGB color picker"
    } else {
        "Left/Right changes choices. Changes apply only after SAVE."
    };
    frame.render_widget(
        Paragraph::new(hint)
            .centered()
            .style(Style::default().fg(app.config.theme.muted)),
        Rect::new(area.x, area.y + 3, area.width, 2),
    );
    // Hide background rows in modal editors so long values, e.g. key names, cannot peek out.
    if settings.edit.is_none() || matches!(settings.edit, Some(Edit::Name(_))) {
        setting_rows(frame, area, settings, &entries, app);
    }
    if let Some(edit) = &settings.edit {
        editor(frame, area, edit, field, app);
    }
    let error = settings.error.as_ref().or(match &settings.save {
        SaveState::Failed(error) => Some(error),
        _ => None,
    });
    if let Some(error) = error {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .centered()
                .wrap(Wrap { trim: true }),
            Rect::new(area.x + 2, area.bottom() - 6, area.width - 4, 3),
        );
    }
    let hints = match &settings.edit {
        Some(Edit::Name(_)) => vec![
            key_hints(&[("Enter", "accept"), ("Esc", "cancel edit")], app),
            key_hints(&[("Backspace", "delete"), ("Delete", "clear")], app),
        ],
        Some(Edit::Keys {
            capturing: true, ..
        }) => vec![Line::from("Press the key to assign (including Esc / F2)").centered()],
        Some(Edit::Keys { .. }) => vec![
            key_hints(
                &[("↑ ↓", "select"), ("Enter", "assign"), ("Delete", "remove")],
                app,
            ),
            key_hints(&[("F2", "accept"), ("Esc", "cancel edit")], app),
        ],
        Some(Edit::Color(_)) => vec![
            key_hints(
                &[
                    ("↑ ↓ ← →", "adjust"),
                    ("Tab", "palette/RGB"),
                    ("Delete", "terminal default"),
                ],
                app,
            ),
            key_hints(&[("Enter", "accept"), ("Esc", "cancel edit")], app),
        ],
        None => vec![
            key_hints(
                &[("↑ ↓", "navigate"), ("← →", "change"), ("Enter", "edit")],
                app,
            ),
            key_hints(&[("F2", "save"), ("Esc", "cancel")], app),
        ],
    };
    footer(frame, area, hints, app);
}

fn setting_rows(
    frame: &mut Frame<'_>,
    area: Rect,
    settings: &Settings,
    entries: &[(&str, String)],
    app: &App,
) {
    let rows = usize::from(area.height.saturating_sub(13)).max(1);
    let start = settings.selected.saturating_sub(rows - 1);
    for (index, (name, value)) in entries.iter().enumerate().skip(start).take(rows) {
        let y = area.y + 6 + u16::try_from(index - start).unwrap_or_default();
        let selected = index == settings.selected;
        let style = Style::default().fg(if selected {
            app.config.theme.accent
        } else {
            app.config.theme.foreground
        });
        let label = match *name {
            "das_ms" => "DAS".into(),
            "arr_ms" => "ARR".into(),
            "soft_drop_ms" => "SOFT DROP".into(),
            "practice_speed_percent" => "PRACTICE SPEED".into(),
            _ => name
                .replace("keys.", "KEY ")
                .replace("theme.", "COLOR ")
                .replace('_', " ")
                .to_uppercase(),
        };
        frame.render_widget(
            Paragraph::new(format!("{} {label}", if selected { ">" } else { " " })).style(style),
            Rect::new(area.x + 2, y, 30, 1),
        );
        let editing = selected && matches!(settings.edit, Some(Edit::Name(_)));
        let value = if let Some(Edit::Name(text)) = settings.edit.as_ref().filter(|_| editing) {
            format!("{text} ")
        } else {
            match *name {
                "ghost_enabled" if settings.config.ghost_enabled => "< ON >".into(),
                "ghost_enabled" => "< OFF >".into(),
                "mode" => format!("< {} >", settings.config.mode.label()),
                "next_queue_length" => format!("< {} >", settings.config.next_queue_length),
                "das_ms" | "arr_ms" | "soft_drop_ms" => format!("< {value} ms >"),
                "practice_speed_percent" => format!("< {value}% >"),
                _ => value.clone(),
            }
        };
        let value_area = Rect::new(area.x + 33, y, area.width - 35, 1);
        frame.render_widget(Line::from(value).right_aligned().style(style), value_area);
        if editing {
            frame.set_cursor_position((value_area.right() - 1, y));
        }
    }
    frame.render_widget(
        Paragraph::new(format!("{}/{}", settings.selected + 1, entries.len()))
            .centered()
            .style(Style::default().fg(app.config.theme.muted)),
        Rect::new(area.x, area.bottom() - 7, area.width, 1),
    );
}

fn editor(frame: &mut Frame<'_>, area: Rect, edit: &Edit, field: &str, app: &App) {
    if matches!(edit, Edit::Name(_)) {
        return;
    }
    // Keep the edited target visible when the list is hidden, e.g. COUNTER CLOCKWISE.
    let target = field
        .rsplit('.')
        .next()
        .unwrap_or(field)
        .replace('_', " ")
        .to_uppercase();
    let panel = Rect::new(area.x + 4, area.y + 5, area.width - 8, area.height - 8);
    frame.render_widget(Clear, panel);
    frame.render_widget(
        Block::bordered()
            .style(Style::default().bg(app.config.theme.background))
            .title_top(match edit {
                Edit::Keys { .. } => format!(" KEY BINDINGS — {target} "),
                _ => format!(" COLOR PICKER — {target} "),
            }),
        panel,
    );
    match edit {
        Edit::Keys {
            bindings,
            selected,
            capturing,
        } => {
            let rows = usize::from(panel.height.saturating_sub(7));
            let start = selected.saturating_sub(rows.saturating_sub(1));
            for index in start..=bindings.len().min(start + rows.saturating_sub(1)) {
                let label = bindings
                    .get(index)
                    .map_or_else(|| "+ ADD KEY".into(), ToString::to_string);
                let label = if index == *selected && *capturing {
                    "PRESS A KEY…".into()
                } else {
                    label
                };
                let style = Style::default().fg(if index == *selected {
                    app.config.theme.accent
                } else {
                    app.config.theme.foreground
                });
                frame.render_widget(
                    Paragraph::new(format!(
                        "{} {label}",
                        if index == *selected { ">" } else { " " }
                    ))
                    .style(style),
                    Rect::new(
                        panel.x + 3,
                        panel.y + 2 + u16::try_from(index - start).unwrap_or_default(),
                        panel.width - 6,
                        1,
                    ),
                );
            }
        }
        Edit::Color(picker) => {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}    {}",
                    if picker.palette { "PALETTE" } else { "RGB" },
                    picker.value
                ))
                .centered(),
                Rect::new(panel.x + 1, panel.y + 1, panel.width - 2, 1),
            );
            let components = rgb(picker.value);
            if picker.palette {
                palette(frame, panel, picker.value, picker.index());
            } else {
                sliders(frame, panel, components, picker.channel);
            }
            frame.render_widget(
                Paragraph::new("PREVIEW")
                    .centered()
                    .style(
                        Style::default()
                            .bg(picker.value)
                            .fg(if picker.value == Color::Reset {
                                Color::Reset
                            } else {
                                contrast(components)
                            }),
                    ),
                Rect::new(panel.x + 5, panel.y + 12, panel.width - 10, 2),
            );
        }
        Edit::Name(_) => {}
    }
}

fn contrast([r, g, b]: [u8; 3]) -> Color {
    if u32::from(r) + u32::from(g) + u32::from(b) > 380 {
        Color::Black
    } else {
        Color::White
    }
}

fn palette(frame: &mut Frame<'_>, panel: Rect, value: Color, selected_index: u8) {
    let x = panel.x + (panel.width - 64) / 2;
    for index in 0..=255_u8 {
        let selected = value != Color::Reset && selected_index == index;
        let color = Color::Indexed(index);
        frame.render_widget(
            Paragraph::new(if selected { "[]" } else { "  " })
                .style(Style::default().bg(color).fg(contrast(rgb(color)))),
            Rect::new(
                x + u16::from(index % 32) * 2,
                panel.y + 3 + u16::from(index / 32),
                2,
                1,
            ),
        );
    }
}

fn sliders(frame: &mut Frame<'_>, panel: Rect, [r, g, b]: [u8; 3], selected: usize) {
    for (index, (name, value, color)) in [
        ("RED", r, Color::Red),
        ("GREEN", g, Color::Green),
        ("BLUE", b, Color::Blue),
    ]
    .into_iter()
    .enumerate()
    {
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(color))
                .ratio(f64::from(value) / 255.0)
                .label(format!(
                    "{} {name:<5} {value:03}",
                    if selected == index { ">" } else { " " }
                )),
            Rect::new(
                panel.x + 5,
                panel.y + 3 + u16::try_from(index).unwrap_or_default() * 3,
                panel.width - 10,
                1,
            ),
        );
    }
    frame.render_widget(
        Paragraph::new("PgUp/PgDn: ±16    Home/End: 0/255").centered(),
        Rect::new(panel.x + 1, panel.y + 10, panel.width - 2, 1),
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::{Screen, Settings};
    use crate::config::Config;
    use crate::ui::tests::position_of;

    #[rstest::rstest]
    fn color_controls_share_columns_and_palette_is_centered(
        #[values((80, 28), (81, 29), (100, 40))] size: (u16, u16),
    ) {
        let panel = Rect::new(4, 5, size.0 - 8, size.1 - 8);
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal
            .draw(|frame| sliders(frame, panel, [16, 17, 22], 0))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let red = position_of(buffer, "RED").0;
        for name in ["GREEN", "BLUE"] {
            assert_eq!(position_of(buffer, name).0, red);
        }
        let value = position_of(buffer, "016").0;
        for text in ["017", "022"] {
            assert_eq!(position_of(buffer, text).0, value);
        }
        terminal
            .draw(|frame| palette(frame, panel, Color::Indexed(0), 0))
            .unwrap();
        let (x, _) = position_of(terminal.backend().buffer(), "[]");
        assert!((x - panel.x).abs_diff(panel.right() - (x + 64)) <= 1);
    }

    #[rstest::rstest]
    #[case("das_ms", "DAS", "150 ms", "Delay before horizontal repeat")]
    #[case("arr_ms", "ARR", "30 ms", "0 = move to wall after DAS")]
    #[case("soft_drop_ms", "SOFT DROP", "30 ms", "Drop interval without DAS")]
    #[case(
        "practice_speed_percent",
        "PRACTICE SPEED",
        "100%",
        "Practice gravity only"
    )]
    fn numeric_settings_show_units_and_terminal_support(
        #[case] name: &str,
        #[case] label: &str,
        #[case] value: &str,
        #[case] hint: &str,
        #[values(false, true)] releases: bool,
        #[values((80, 28), (100, 40))] size: (u16, u16),
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.screen = Screen::Settings;
        app.keyboard_releases = releases;
        let mut draft = Settings::new(app.config.clone());
        draft.selected = draft
            .config
            .entries()
            .iter()
            .position(|&(field, _)| field == name)
            .unwrap();
        app.settings = Some(draft);
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal
            .draw(|frame| super::super::draw(frame, &app))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for expected in [
            format!("> {label}"),
            format!("< {value} >"),
            if releases || name == "practice_speed_percent" {
                hint.into()
            } else {
                "OS key repeat applies".into()
            },
        ] {
            assert!(text.contains(&expected), "missing {expected}");
        }
    }

    #[rstest::rstest]
    #[case(80, 28)]
    #[case(100, 40)]
    fn typed_editors_show_controls_and_color_preview(
        #[case] width: u16,
        #[case] height: u16,
        #[values(false, true)] palette_mode: bool,
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.screen = Screen::Settings;
        app.settings = Some(Settings::new(app.config.clone()));
        app.settings.as_mut().unwrap().selected = 18;
        let key =
            |code| crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        app.key(key(crossterm::event::KeyCode::Enter)).unwrap();
        if palette_mode {
            app.key(key(crossterm::event::KeyCode::Tab)).unwrap();
        }
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| super::super::draw(frame, &app))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for label in [
            "COLOR PICKER — BACKGROUND",
            "PREVIEW",
            "cancel edit",
            if palette_mode { "PALETTE" } else { "RED" },
        ] {
            assert!(text.contains(label), "missing {label}");
        }
        let buffer = terminal.backend().buffer();
        let shell = super::super::centered(
            Rect::new(0, 0, width, height),
            width.min(94),
            height.min(30),
        );
        for y in shell.y + 5..shell.bottom() - 3 {
            for x in (shell.x..shell.x + 4).chain(shell.right() - 4..shell.right()) {
                assert_eq!(
                    buffer[(x, y)].symbol(),
                    " ",
                    "background text beside editor at {x},{y}"
                );
            }
        }
        app.key(key(crossterm::event::KeyCode::Esc)).unwrap();
        app.settings.as_mut().unwrap().selected = 11;
        app.key(key(crossterm::event::KeyCode::Enter)).unwrap();
        app.key(key(crossterm::event::KeyCode::Enter)).unwrap();
        terminal
            .draw(|frame| super::super::draw(frame, &app))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("KEY BINDINGS — COUNTER CLOCKWISE"));
        assert!(text.contains("PRESS A KEY"));
        assert!(text.contains("including Esc / F2"));
    }

    #[rstest::rstest]
    #[case(80, 28)]
    #[case(100, 40)]
    fn settings_scroll_and_keep_errors_and_controls_visible(
        #[case] width: u16,
        #[case] height: u16,
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.screen = Screen::Settings;
        let mut draft = Settings::new(app.config.clone());
        draft.selected = draft.config.entries().len();
        draft.save = SaveState::Failed("Save failed: select SAVE to retry".into());
        app.settings = Some(draft);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| super::super::draw(frame, &app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        for label in ["SETTINGS", "> SAVE", "Save failed", "F2", "cancel"] {
            assert!(text.contains(label), "missing {label}");
        }
        app.settings.as_mut().unwrap().selected = 0;
        app.settings.as_mut().unwrap().edit =
            Some(Edit::Name("가나다라마바사아자차카타파하가끝".into()));
        terminal
            .draw(|frame| super::super::draw(frame, &app))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("끝"));
        assert!(text.contains("accept"));
    }
}
