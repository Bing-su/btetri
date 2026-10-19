use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::{centered, focused, footer, format_duration, header, key_hints};
use crate::app::{App, SaveState, Screen};
use crate::config::Keys;
use crate::game::{ActivePiece, Game, HIDDEN, Mode, Piece, ScoreEvent, Spin, Status};

fn game_areas(area: Rect) -> (Rect, Rect, Rect) {
    let group = centered(Rect::new(area.x, area.y + 3, area.width, 22), 58, 22);
    (
        Rect::new(group.x, group.y, 18, 22),
        Rect::new(group.x + 21, group.y, 22, 22),
        Rect::new(group.x + 46, group.y, 12, 22),
    )
}

/// Render the board, sidebars, and the current gameplay dialog inside the given area.
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(game) = &app.game else {
        return;
    };
    header(frame, area, game.mode().label(), app);
    let (left, board, next) = game_areas(area);
    let dim = app.screen != Screen::Playing;
    playfield(frame, board, game, app, dim);
    sidebars(frame, left, next, game, app, dim);
    match app.screen {
        Screen::Paused => {
            pause_dialog(frame, board, app);
            footer(
                frame,
                area,
                vec![key_hints(
                    &[("↑ ↓", "navigate"), ("Enter", "select"), ("Esc", "resume")],
                    app,
                )],
                app,
            );
        }
        Screen::GameOver => {
            result_dialog(frame, board, game, app);
            footer(
                frame,
                area,
                vec![key_hints(
                    &[
                        ("↑ ↓", "navigate"),
                        ("Enter", "select"),
                        ("Esc", "main menu"),
                    ],
                    app,
                )],
                app,
            );
            if let SaveState::Failed(error) = &app.save {
                frame.render_widget(
                    Paragraph::new(error.as_str())
                        .centered()
                        .wrap(Wrap { trim: true }),
                    Rect::new(area.x, area.bottom().saturating_sub(3), area.width, 2),
                );
            }
        }
        _ => {
            let keys = &app.config.keys;
            footer(
                frame,
                area,
                vec![
                    key_hints(
                        &[
                            (
                                &format!(
                                    "{} {}",
                                    Keys::label(&keys.left),
                                    Keys::label(&keys.right)
                                ),
                                "move",
                            ),
                            (
                                &format!(
                                    "{} {}",
                                    Keys::label(&keys.counter_clockwise),
                                    Keys::label(&keys.clockwise)
                                ),
                                "rotate",
                            ),
                            (&Keys::label(&keys.hard_drop), "hard drop"),
                            (&Keys::label(&keys.rotate_180), "180"),
                        ],
                        app,
                    ),
                    key_hints(
                        &[
                            (
                                &Keys::label(&keys.soft_drop),
                                if game.soft_drop() {
                                    "soft ON"
                                } else {
                                    "soft drop"
                                },
                            ),
                            (&Keys::label(&keys.hold), "hold"),
                            (&Keys::label(&keys.pause), "pause"),
                            (&Keys::label(&keys.help), "help"),
                        ],
                        app,
                    ),
                ],
                app,
            );
        }
    }
}

fn playfield(frame: &mut Frame<'_>, area: Rect, game: &Game, app: &App, dim: bool) {
    let theme = &app.config.theme;
    frame.render_widget(
        Block::bordered().border_style(Style::default().fg(theme.border)),
        area,
    );
    let inner = Rect::new(area.x + 1, area.y + 1, 20, 20);
    for (y, row) in game.board().iter().skip(HIDDEN).enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let x = inner.x + u16::try_from(x * 2).unwrap_or_default();
            let y = inner.y + u16::try_from(y).unwrap_or_default();
            let style = Style::default().fg(cell.map_or(theme.grid, |piece| theme.piece(piece)));
            let style = if dim {
                style.add_modifier(Modifier::DIM)
            } else {
                style
            };
            frame
                .buffer_mut()
                .set_string(x, y, if cell.is_some() { "██" } else { "· " }, style);
        }
    }
    if game.status() == Status::Playing {
        if app.config.ghost_enabled {
            active(
                frame,
                inner,
                game.ghost(),
                "░░",
                Style::default().fg(theme.ghost),
                dim,
            );
        }
        active(
            frame,
            inner,
            game.active(),
            "██",
            Style::default().fg(theme.piece(game.active().kind)),
            dim,
        );
    }
}

fn active(
    frame: &mut Frame<'_>,
    area: Rect,
    piece: ActivePiece,
    symbol: &str,
    style: Style,
    dim: bool,
) {
    let style = if dim {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    };
    for (x, y) in piece.cells() {
        let y = y - i32::try_from(HIDDEN).unwrap_or_default();
        if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y))
            && x < 10
            && y < 20
        {
            frame
                .buffer_mut()
                .set_string(area.x + x * 2, area.y + y, symbol, style);
        }
    }
}

fn sidebars(frame: &mut Frame<'_>, left: Rect, next: Rect, game: &Game, app: &App, dim: bool) {
    let theme = &app.config.theme;
    let border = Style::default().fg(theme.border);
    let hold = Rect::new(left.x, left.y, 14, 5);
    frame.render_widget(
        Block::bordered().title_top(" HOLD ").border_style(border),
        hold,
    );
    if let Some(piece) = game.held() {
        preview(
            frame,
            Rect::new(hold.x + 1, hold.y + 1, hold.width - 2, 3),
            piece,
            app,
            dim,
        );
    }
    let time = game.remaining().unwrap_or_else(|| game.elapsed());
    let (progress, value) = match game.mode() {
        Mode::FortyLines => ("TO GO", 40_u32.saturating_sub(game.lines())),
        Mode::Blitz | Mode::Marathon => ("LEVEL", game.level()),
        Mode::Practice => ("RESETS", game.practice_resets()),
    };
    for (offset, label, value) in [
        (7, "SCORE", format!("{:06}", game.score())),
        (11, progress, format!("{value:02}")),
        (14, "LINES", format!("{:03}", game.lines())),
        (18, "TIME", format_duration(time)),
    ] {
        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(theme.muted)),
            Rect::new(left.x, left.y + offset, left.width, 1),
        );
        frame.render_widget(
            Paragraph::new(value),
            Rect::new(left.x, left.y + offset + 1, left.width, 1),
        );
    }
    if let Some(event) = game.last_score().filter(|event| event.points > 0) {
        frame.render_widget(
            score_feedback(event).style(Style::default().fg(theme.accent)),
            Rect::new(left.x, left.y + 9, left.width, 2),
        );
    }
    frame.render_widget(
        Block::bordered().title_top(" NEXT ").border_style(border),
        next,
    );
    for (index, piece) in game.next().take(app.config.next_queue_length).enumerate() {
        let y = next.y + 1 + u16::try_from(index).unwrap_or_default() * 3;
        preview(
            frame,
            Rect::new(next.x + 1, y, next.width - 2, 2),
            piece,
            app,
            dim,
        );
        if index + 1 < app.config.next_queue_length {
            frame.render_widget(
                Paragraph::new("─".repeat(usize::from(next.width - 2))).style(border),
                Rect::new(next.x + 1, y + 2, next.width - 2, 1),
            );
        }
    }
}

fn score_feedback(event: ScoreEvent) -> Paragraph<'static> {
    let label = if event.perfect_clear {
        "PERFECT CLEAR".to_owned()
    } else {
        match event.spin {
            Spin::Full => format!("T-SPIN {}", event.lines),
            Spin::Mini => format!("MINI SPIN {}", event.lines),
            Spin::None if event.lines == 4 => "TETRIS".into(),
            Spin::None => format!("{} LINE", event.lines),
        }
    };
    let combo = event
        .combo
        .filter(|&count| count > 0)
        .map_or_else(String::new, |count| format!("C{count} "));
    Paragraph::new(vec![
        Line::from(label),
        Line::from(format!(
            "{}{combo}+{}",
            if event.back_to_back { "B2B " } else { "" },
            event.points
        )),
    ])
}

fn preview(frame: &mut Frame<'_>, area: Rect, piece: Piece, app: &App, dim: bool) {
    let cells = piece.cells(0);
    let min_x = cells.iter().map(|(x, _)| *x).min().unwrap_or_default();
    let max_x = cells.iter().map(|(x, _)| *x).max().unwrap_or_default();
    let min_y = cells.iter().map(|(_, y)| *y).min().unwrap_or_default();
    let width = u16::try_from((max_x - min_x + 1) * 2).unwrap_or_default();
    let start = area.x + area.width.saturating_sub(width) / 2;
    let style = Style::default().fg(app.config.theme.piece(piece));
    let style = if dim {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    };
    for (x, y) in cells {
        frame.buffer_mut().set_string(
            start + u16::try_from((x - min_x) * 2).unwrap_or_default(),
            area.y + u16::try_from(y - min_y).unwrap_or_default(),
            "██",
            style,
        );
    }
}

fn popup(frame: &mut Frame<'_>, area: Rect, height: u16, app: &App) -> Rect {
    let area = centered(area, 22, height);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::bordered()
            .border_style(Style::default().fg(app.config.theme.border))
            .style(Style::default().bg(app.config.theme.background)),
        area,
    );
    Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2)
}

fn pause_dialog(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let inner = popup(frame, area, 11, app);
    let lines = vec![
        Line::from(""),
        Line::from("PAUSED"),
        Line::from(""),
        focused("RESUME", app.dialog_selection == 0, app),
        focused("MAIN MENU", app.dialog_selection == 1, app),
        Line::from(""),
        Line::from("Main menu ends").style(Style::default().fg(app.config.theme.muted)),
        Line::from("this game.").style(Style::default().fg(app.config.theme.muted)),
    ];
    frame.render_widget(Paragraph::new(lines).centered(), inner);
}

fn result_dialog(frame: &mut Frame<'_>, area: Rect, game: &Game, app: &App) {
    let inner = popup(frame, area, if app.notice.is_some() { 20 } else { 16 }, app);
    let save = match &app.save {
        SaveState::Saved => "Record saved",
        SaveState::Failed(_) => "Save failed: R retry",
        _ => "Saving record...",
    };
    let mut lines = vec![
        Line::from(if game.status() == Status::Complete {
            "COMPLETE"
        } else {
            "GAME OVER"
        }),
        Line::from(""),
    ];
    // Use the widest summary so long runs, e.g. 1000:00.000, keep labels in one column.
    let summary = [
        format!("SCORE {:06}", game.score()),
        format!("LINES {}", game.lines()),
        format!("TIME  {}", format_duration(game.elapsed())),
    ];
    let width = summary.iter().map(String::len).max().unwrap_or_default();
    lines.extend(
        summary
            .into_iter()
            .map(|text| Line::from(format!("{text:<width$}"))),
    );
    lines.extend([
        Line::from(""),
        Line::from(save).style(Style::default().fg(app.config.theme.muted)),
        Line::from(""),
    ]);
    for (index, label) in ["PLAY AGAIN", "STATISTICS", "RECORDS", "MAIN MENU"]
        .into_iter()
        .enumerate()
    {
        lines.push(focused(
            label,
            app.dialog_selection == index && (index == 1 || matches!(app.save, SaveState::Saved)),
            app,
        ));
    }
    frame.render_widget(Paragraph::new(lines).centered(), inner);
    if let Some(notice) = &app.notice {
        // Keep save warnings inside the dialog so a 28-row terminal has room for the error below it.
        frame.render_widget(
            Paragraph::new(notice.as_str())
                .centered()
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(app.config.theme.muted)),
            Rect::new(inner.x, inner.y + 13, inner.width, inner.height - 13),
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::config::Config;

    #[rstest::rstest]
    #[case(80, 28)]
    #[case(80, 29)]
    #[case(100, 40)]
    fn key_hints_and_save_errors_do_not_overwrite_the_playfield(
        #[case] width: u16,
        #[case] height: u16,
        #[values(false, true)] failed_save_notice: bool,
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        if failed_save_notice {
            app.screen = Screen::GameOver;
            app.save = SaveState::Failed("Database unavailable".into());
            app.key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Enter,
                crossterm::event::KeyModifiers::NONE,
            ))
            .unwrap();
            assert!(app.notice.is_some());
        }
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| super::super::draw(frame, &app))
            .unwrap();
        let shell = centered(
            Rect::new(0, 0, width, height),
            width.min(94),
            height.min(30),
        );
        let (_, board, _) = game_areas(shell);
        let buffer = terminal.backend().buffer();
        let bottom = board.bottom() - 1;
        assert_eq!(buffer[(board.x, bottom)].symbol(), "└");
        assert_eq!(buffer[(board.right() - 1, bottom)].symbol(), "┘");
        if !failed_save_notice {
            for x in [board.x, board.right() - 1] {
                assert_eq!(buffer[(x, bottom - 1)].symbol(), "│");
            }
        }
        for x in board.x + 1..board.right() - 1 {
            assert_eq!(buffer[(x, bottom)].symbol(), "─");
        }
    }

    #[rstest::rstest]
    #[case(false, "MINI SPIN 2", 650)]
    #[case(true, "PERFECT CLEAR", 4750)]
    fn score_feedback_fits_the_sidebar_with_combo_and_b2b(
        #[case] perfect_clear: bool,
        #[case] label: &str,
        #[case] points: u64,
    ) {
        let mut terminal = Terminal::new(TestBackend::new(18, 2)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    score_feedback(ScoreEvent {
                        spin: if perfect_clear {
                            Spin::None
                        } else {
                            Spin::Mini
                        },
                        lines: if perfect_clear { 4 } else { 2 },
                        points,
                        combo: Some(1),
                        back_to_back: true,
                        perfect_clear,
                    }),
                    frame.area(),
                )
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains(label));
        assert!(text.contains(&format!("B2B C1 +{points}")));
    }
}
