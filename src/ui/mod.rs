//! Screen rendering and shared terminal layout; input and state changes belong to `app`.

mod gameplay;
mod records;
mod settings;
mod statistics;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

use crate::app::{App, MenuItem, Screen};
use crate::config::Keys;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 28;

/// Render the current screen and notices, or a resize prompt for a small terminal.
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
        Screen::Statistics => statistics::draw(frame, shell, app),
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
    for (index, item) in MenuItem::ALL.into_iter().skip(2).enumerate() {
        frame.render_widget(
            Paragraph::new(focused(item.label(), app.menu_selection == item, app)).centered(),
            Rect::new(
                form.x,
                form.y + 6 + u16::try_from(index).unwrap_or_default() * 2,
                form.width,
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
    let area = centered(area, 74, 20);
    frame.render_widget(Clear, area);
    let keys = &app.config.keys;
    let controls = [
        (
            format!("{} / {}", Keys::label(&keys.left), Keys::label(&keys.right)),
            "Move left / right",
        ),
        (
            format!(
                "{} / {}",
                Keys::label(&keys.counter_clockwise),
                Keys::label(&keys.clockwise)
            ),
            "Rotate counterclockwise / clockwise",
        ),
        (Keys::label(&keys.hard_drop), "Hard drop"),
        (Keys::label(&keys.rotate_180), "Rotate 180"),
        (
            Keys::label(&keys.soft_drop),
            "Soft drop (hold; legacy: repeat)",
        ),
        (Keys::label(&keys.hold), "Hold"),
        (Keys::label(&keys.pause), "Pause"),
    ];
    let key_width = controls
        .iter()
        .map(|(key, _)| Line::from(key.as_str()).width())
        .max()
        .unwrap_or_default();
    let action_width = controls
        .iter()
        .map(|(_, action)| action.len())
        .max()
        .unwrap_or_default();
    let key_width = u16::try_from(key_width).unwrap_or_default();
    let width = key_width + 4 + u16::try_from(action_width).unwrap_or_default();
    frame.render_widget(
        Block::bordered().style(Style::default().bg(app.config.theme.background)),
        area,
    );
    frame.render_widget(
        Paragraph::new("CONTROLS — PAUSED").centered(),
        Rect::new(area.x + 1, area.y + 1, area.width - 2, 1),
    );
    // Keep descriptions in one column even for bindings such as LEFT / RIGHT and SPACE.
    let table_area = centered(
        Rect::new(area.x + 1, area.y + 3, area.width - 2, 7),
        width,
        7,
    );
    frame.render_widget(
        Table::new(
            controls.into_iter().map(|(key, action)| {
                Row::new([
                    Cell::from(Line::from(key).right_aligned()),
                    Cell::from(action),
                ])
            }),
            [Constraint::Length(key_width), Constraint::Min(0)],
        )
        .column_spacing(4),
        table_area,
    );
    let lines = vec![
        Line::from("40 LINES: clear 40 lines at fixed gravity"),
        Line::from("BLITZ: score within 2 minutes; gravity increases"),
        Line::from("MARATHON: survive; level up every 10 lines (max 20)"),
        Line::from("PRACTICE: unlimited; clear board on topout"),
        Line::from(""),
        key_hints(&[("Esc", "close")], app),
    ];
    frame.render_widget(
        Paragraph::new(lines).centered(),
        Rect::new(area.x + 1, area.y + 11, area.width - 2, 6),
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
mod tests;
