use std::time::Duration;

use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};

use super::{centered, footer, format_duration, header, key_hints};
use crate::app::{App, RecordsState};
use crate::game::Mode;

/// Render ranked records and loading/failure feedback for the selected mode.
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    header(frame, area, "RECORDS", app);
    frame.render_widget(
        Paragraph::new(format!("MODE     < {} >", app.record_mode.label())).centered(),
        Rect::new(area.x, area.y + 4, area.width, 1),
    );
    let message = match &app.records {
        RecordsState::Requested => Some("Loading records...".to_owned()),
        RecordsState::Failed(error) => Some(format!("{error}\nR retry   Esc back")),
        RecordsState::Loaded(rows) if rows.is_empty() => {
            Some("No records for this mode.\nComplete a game to add your first record.".to_owned())
        }
        RecordsState::Loaded(_) => None,
    };
    if let Some(text) = message {
        frame.render_widget(
            Paragraph::new(text).centered().wrap(Wrap { trim: true }),
            centered(area, area.width.saturating_sub(4), 5),
        );
    } else {
        records_table(frame, area, app);
    }
    footer(
        frame,
        area,
        vec![key_hints(
            &[("↑ ↓", "browse"), ("← →", "mode"), ("Esc", "back")],
            app,
        )],
        app,
    );
}

fn records_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    // Align headings with their values, e.g. SCORE ends at the score's units column.
    let aligned_cell = |column, value: String| {
        Cell::from(Text::from(value).alignment(if column == 1 {
            Alignment::Left
        } else {
            Alignment::Right
        }))
    };
    let table_area = Rect::new(
        area.x,
        area.y + 7,
        area.width,
        area.height.saturating_sub(13),
    );
    let rows = app.records.rows().iter().enumerate().map(|(index, row)| {
        Row::new(
            vec![
                (index + 1).to_string(),
                row.player.clone(),
                format!("{:06}", row.score),
                format!("{:02}", row.level),
                format!("{:03}", row.lines),
                format_duration(Duration::from_millis(
                    u64::try_from(row.elapsed_ms).unwrap_or_default(),
                )),
                format_date(row.completed_at),
            ]
            .into_iter()
            .enumerate()
            .map(|(column, value)| aligned_cell(column, value)),
        )
        .height(2)
    });
    let columns = [
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(8),
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    let table = Table::new(rows, columns)
        .column_spacing(2)
        .header(
            Row::new(
                ["#", "PLAYER", "SCORE", "LV", "LINES", "TIME", "DATE"]
                    .into_iter()
                    .enumerate()
                    .map(|(column, value)| aligned_cell(column, value.to_owned())),
            )
            .style(Style::default().fg(app.config.theme.muted))
            .bottom_margin(1),
        )
        .block(
            Block::default()
                .borders(Borders::TOP | Borders::BOTTOM)
                .border_style(Style::default().fg(app.config.theme.border)),
        )
        .row_highlight_style(Style::default().fg(app.config.theme.accent))
        .highlight_symbol("> ");
    let visible = usize::from(table_area.height.saturating_sub(4) / 2).max(1);
    let mut state = TableState::default()
        .with_selected(Some(app.record_selection))
        .with_offset(app.record_selection.saturating_sub(visible - 1));
    frame.render_stateful_widget(table, table_area, &mut state);
    let ordering = if app.record_mode == Mode::FortyLines {
        "Completed runs, fastest first"
    } else {
        "Score: highest first"
    };
    frame.render_widget(
        Paragraph::new(format!("{} records   {ordering}", app.records.rows().len()))
            .style(Style::default().fg(app.config.theme.muted)),
        Rect::new(area.x, table_area.bottom() + 1, area.width, 1),
    );
}

fn format_date(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0).map_or_else(
        || "invalid".into(),
        |date| date.with_timezone(&Local).format("%Y-%m-%d").to_string(),
    )
}
