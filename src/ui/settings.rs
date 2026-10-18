use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::{footer, header, key_hints};
use crate::app::{App, SaveState};

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
    let selected = settings.selected;
    let field = entries.get(selected).map_or("", |(name, _)| *name);
    let hint = if field.starts_with("keys.") {
        "Keys: comma-separated names, e.g. UP, X or C, SHIFT"
    } else if field.starts_with("theme.") {
        "Colors: name, #RRGGBB, or palette index 0–255"
    } else if field == "mode" {
        "Modes: 40l, blitz, practice"
    } else {
        "Changes apply only after SAVE. Reset also needs SAVE."
    };
    frame.render_widget(
        Paragraph::new(hint)
            .centered()
            .style(Style::default().fg(app.config.theme.muted)),
        Rect::new(area.x, area.y + 3, area.width, 2),
    );
    let rows = usize::from(area.height.saturating_sub(13)).max(1);
    let start = selected.saturating_sub(rows - 1);
    for (index, (name, value)) in entries.iter().enumerate().skip(start).take(rows) {
        let y = area.y + 6 + u16::try_from(index - start).unwrap_or_default();
        let style = Style::default().fg(if index == selected {
            app.config.theme.accent
        } else {
            app.config.theme.foreground
        });
        let label = name
            .replace("keys.", "KEY ")
            .replace("theme.", "COLOR ")
            .replace('_', " ")
            .to_uppercase();
        frame.render_widget(
            Paragraph::new(format!(
                "{} {label}",
                if index == selected { ">" } else { " " }
            ))
            .style(style),
            Rect::new(area.x + 2, y, 30, 1),
        );
        let editing = index == selected && settings.edit.is_some();
        let value = if editing {
            format!("{} ", settings.edit.as_deref().unwrap_or_default())
        } else {
            value.clone()
        };
        let value_area = Rect::new(area.x + 33, y, area.width.saturating_sub(35), 1);
        frame.render_widget(Line::from(value).right_aligned().style(style), value_area);
        if editing {
            frame.set_cursor_position((value_area.right().saturating_sub(1), y));
        }
    }
    frame.render_widget(
        Paragraph::new(format!("{}/{}", selected + 1, entries.len()))
            .centered()
            .style(Style::default().fg(app.config.theme.muted)),
        Rect::new(area.x, area.bottom() - 7, area.width, 1),
    );
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
    let hints = if settings.edit.is_some() {
        vec![
            key_hints(&[("Enter", "accept"), ("Esc", "cancel edit")], app),
            key_hints(&[("Backspace", "delete"), ("Delete", "clear")], app),
        ]
    } else {
        vec![
            key_hints(
                &[("↑ ↓", "navigate"), ("← →", "change"), ("Enter", "edit")],
                app,
            ),
            key_hints(&[("F2", "save"), ("Esc", "cancel")], app),
        ]
    };
    footer(frame, area, hints, app);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::{Screen, Settings};
    use crate::config::Config;

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
        app.settings.as_mut().unwrap().edit = Some("가나다라마바사아자차카타파하가끝".into());
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
