use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::{centered, footer, format_duration, header, key_hints};
use crate::app::{App, PersonalBest, SaveState};
use crate::storage::Record;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(game) = &app.game else {
        return;
    };
    header(
        frame,
        area,
        &format!("{} — STATISTICS", game.mode().label()),
        app,
    );
    let panel = centered(area, 58, 20);
    let block = Block::bordered().border_style(Style::default().fg(app.config.theme.border));
    let inner = block.inner(panel);
    frame.render_widget(block, panel);
    let seconds = game.elapsed().as_secs_f64();
    let pps = if seconds > 0.0 {
        game.pieces() as f64 / seconds
    } else {
        0.0
    };
    let mut lines = vec![
        Line::from(format!("PLAYER  {}", app.config.player)),
        Line::from(""),
        Line::from(format!("{:<20}{}", "SCORE", game.score())),
        Line::from(format!("{:<20}{}", "LINES", game.lines())),
        Line::from(format!("{:<20}{}", "TIME", format_duration(game.elapsed()))),
        Line::from(format!("{:<20}{}", "PIECES PLACED", game.pieces())),
        Line::from(format!("{:<20}{pps:.2}", "PPS (pieces/sec)")),
        Line::from(""),
        Line::from("PERSONAL BEST").style(Style::default().fg(app.config.theme.accent)),
    ];
    if let Some(current) = &app.record {
        lines.extend(
            comparison(current, &app.personal_best)
                .into_iter()
                .map(Line::from),
        );
    }
    if !matches!(app.save, SaveState::Saved) {
        lines.push(Line::from("Result not saved; return to retry."));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    let mut hints = vec![("Esc Enter", "back")];
    if matches!(app.personal_best, PersonalBest::Failed(_)) {
        hints.push(("R", "retry best"));
    }
    footer(frame, area, vec![key_hints(&hints, app)], app);
}

fn comparison(current: &Record, best: &PersonalBest) -> Vec<String> {
    let best = match best {
        PersonalBest::Requested => return vec!["Loading previous best...".into()],
        PersonalBest::Failed(error) => {
            return vec![format!("Cannot load previous best: {error}. R retries.")];
        }
        PersonalBest::Loaded(None) => {
            return vec![if current.mode == "40l" {
                "No previous completed run.".into()
            } else {
                "No previous record.".into()
            }];
        }
        PersonalBest::Loaded(Some(best)) => best,
    };
    if current.mode == "40l" {
        let previous = Duration::from_millis(u64::try_from(best.elapsed_ms).unwrap_or_default());
        let mut lines = vec![format!("Previous best  {}", format_duration(previous))];
        if current.outcome != "complete" {
            lines.push("Finish 40 lines to compare times.".into());
            return lines;
        }
        let current = u128::try_from(current.elapsed_ms).unwrap_or_default();
        let previous = previous.as_millis();
        let difference =
            Duration::from_millis(u64::try_from(current.abs_diff(previous)).unwrap_or(u64::MAX));
        lines.push(format!(
            "Difference     {}{} ({})",
            if current < previous { "-" } else { "+" },
            format_duration(difference),
            if current < previous {
                "faster"
            } else if current > previous {
                "slower"
            } else {
                "tied"
            },
        ));
        lines
    } else {
        let difference = i128::from(current.score) - i128::from(best.score);
        vec![
            format!("Previous best  {} points", best.score),
            format!("Difference     {difference:+} points"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::Screen;
    use crate::config::Config;
    use crate::game::{Action, Game, Mode};

    #[rstest::rstest]
    #[case(Mode::FortyLines, "complete", 59000, "-00:01.000 (faster)")]
    #[case(Mode::FortyLines, "complete", 61000, "+00:01.000 (slower)")]
    #[case(Mode::FortyLines, "complete", 60000, "+00:00.000 (tied)")]
    #[case(Mode::FortyLines, "top_out", 1000, "Finish 40 lines to compare times.")]
    #[case(Mode::Blitz, "complete", 1100, "+100 points")]
    #[case(Mode::Blitz, "top_out", 900, "-100 points")]
    #[case(Mode::Blitz, "complete", 1000, "+0 points")]
    #[case(Mode::Marathon, "top_out", 1100, "+100 points")]
    fn compares_the_modes_ranking_metric(
        #[case] mode: Mode,
        #[case] outcome: &str,
        #[case] value: i64,
        #[case] expected: &str,
    ) {
        let mut current = Record::from_game(&Game::new(mode, 42), "Alice").unwrap();
        current.outcome = outcome.into();
        current.elapsed_ms = value;
        current.score = value;
        let previous = Record {
            elapsed_ms: 60000,
            score: 1000,
            ..current.clone()
        };
        let text = comparison(&current, &PersonalBest::Loaded(Some(previous))).join("\n");
        assert!(text.contains(expected), "{text}");
    }

    #[rstest::rstest]
    fn summary_and_statistics_are_separate_and_fit_the_terminal(
        #[values((80, 28), (100, 40))] size: (u16, u16),
        #[values(false, true)] zero_time: bool,
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        if !zero_time {
            app.advance(Duration::from_secs(2)).unwrap();
        }
        app.game.as_mut().unwrap().apply(Action::HardDrop);
        let game = app.game.as_ref().unwrap();
        app.record = Some(Record::from_game(game, &app.config.player).unwrap());
        app.save = SaveState::Saved;
        let summary = [
            format!("SCORE {:06}", game.score()),
            format!("LINES {}", game.lines()),
            format!("TIME  {}", format_duration(game.elapsed())),
        ];
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        app.screen = Screen::GameOver;
        terminal.draw(|frame| crate::ui::draw(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for expected in summary {
            assert!(text.contains(&expected), "{text}");
        }
        assert!(text.contains("STATISTICS"));
        assert!(!text.contains("PPS (pieces/sec)"));
        app.screen = Screen::Statistics;
        terminal.draw(|frame| crate::ui::draw(frame, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("PIECES PLACED       1"), "{text}");
        assert!(
            text.contains(if zero_time {
                "PPS (pieces/sec)    0.00"
            } else {
                "PPS (pieces/sec)    0.50"
            }),
            "{text}"
        );
        assert!(text.contains("No previous completed run."));
        assert!(text.contains("[Esc] [Enter] back"));
        assert!(!text.contains("NaN"));
        assert!(!text.contains("inf"));
    }
}
