mod settings;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
pub use settings::{Edit, Settings, rgb};

use crate::config::{Config, Keys, validate_player};
use crate::game::{Action, Game, Mode, Status};
use crate::storage::{Record, Store};

// Keep both player editors consistent, e.g. Ctrl+C must never add a character to a name.
fn edit_name_input(text: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Backspace => {
            text.pop();
        }
        KeyCode::Delete => text.clear(),
        KeyCode::Char(c)
            if !c.is_control()
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && text.chars().count() < 16 =>
        {
            text.push(c)
        }
        _ => {}
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Menu,
    Playing,
    Help(HelpReturn),
    Paused,
    GameOver,
    Statistics,
    Records,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpReturn {
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuItem {
    Player,
    Mode,
    Start,
    Records,
    Settings,
    Quit,
}

impl MenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Player => "PLAYER",
            Self::Mode => "MODE",
            Self::Start => "START GAME",
            Self::Records => "RECORDS",
            Self::Settings => "SETTINGS",
            Self::Quit => "QUIT",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Player => Self::Mode,
            Self::Mode => Self::Start,
            Self::Start => Self::Records,
            Self::Records => Self::Settings,
            Self::Settings => Self::Quit,
            Self::Quit => Self::Player,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Player => Self::Quit,
            Self::Mode => Self::Player,
            Self::Start => Self::Mode,
            Self::Records => Self::Start,
            Self::Settings => Self::Records,
            Self::Quit => Self::Settings,
        }
    }
}

#[derive(Debug, Default)]
pub enum SaveState {
    #[default]
    None,
    Pending,
    Saved,
    Failed(String),
}

impl From<Result<(), String>> for SaveState {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::Saved,
            Err(error) => Self::Failed(error),
        }
    }
}

// Keep rows and errors inside their state so a new request cannot display stale data.
#[derive(Debug)]
pub enum RecordsState {
    Requested,
    Loaded(Vec<Record>),
    Failed(String),
}

#[derive(Debug)]
pub enum PersonalBest {
    Requested,
    Loaded(Option<Record>),
    Failed(String),
}

impl RecordsState {
    pub fn rows(&self) -> &[Record] {
        match self {
            Self::Loaded(rows) => rows,
            _ => &[],
        }
    }
}

pub struct App {
    pub(crate) config: Config,
    pub(crate) settings: Option<Settings>,
    pub(crate) config_save: SaveState,
    pub(crate) screen: Screen,
    pub(crate) menu_selection: MenuItem,
    pub(crate) dialog_selection: usize,
    pub(crate) name_draft: Option<String>,
    pub(crate) game: Option<Game>,
    pub(crate) record: Option<Record>,
    pub(crate) save: SaveState,
    pub(crate) records: RecordsState,
    pub(crate) personal_best: PersonalBest,
    pub(crate) record_mode: Mode,
    pub(crate) record_selection: usize,
    records_back: Screen,
    pub(crate) notice: Option<String>,
    pub(crate) quit: bool,
    pub(crate) keyboard_releases: bool,
    soft_drop_keys: Vec<KeyCode>,
    horizontal_keys: Vec<(KeyCode, Action)>,
    horizontal_wait: Duration,
    pressed_actions: Vec<KeyCode>,
}

impl TryFrom<Config> for App {
    type Error = miette::Report;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        config
            .validate()
            .map_err(|error| miette::miette!("{error}"))?;
        Ok(Self {
            config,
            settings: None,
            config_save: SaveState::None,
            screen: Screen::Menu,
            menu_selection: MenuItem::Start,
            dialog_selection: 0,
            name_draft: None,
            game: None,
            record: None,
            save: SaveState::None,
            records: RecordsState::Loaded(Vec::new()),
            personal_best: PersonalBest::Loaded(None),
            record_mode: Mode::FortyLines,
            record_selection: 0,
            records_back: Screen::Menu,
            notice: None,
            quit: false,
            keyboard_releases: false,
            soft_drop_keys: Vec::new(),
            horizontal_keys: Vec::new(),
            horizontal_wait: Duration::ZERO,
            pressed_actions: Vec::new(),
        })
    }
}

impl App {
    pub fn start(&mut self) {
        self.clear_held_keys();
        self.pressed_actions.clear();
        let mut game = Game::new(self.config.mode, rand::random());
        game.set_practice_speed_percent(self.config.practice_speed_percent);
        self.game = Some(game);
        self.record = None;
        self.personal_best = PersonalBest::Loaded(None);
        self.save = SaveState::None;
        self.screen = Screen::Playing;
        self.dialog_selection = 0;
        self.notice = None;
    }

    pub fn open_records(&mut self, mode: Mode) {
        self.records_back = if self.screen == Screen::GameOver {
            Screen::GameOver
        } else {
            Screen::Menu
        };
        self.screen = Screen::Records;
        self.record_mode = if mode == Mode::Practice {
            Mode::FortyLines
        } else {
            mode
        };
        self.request_records();
    }

    fn request_records(&mut self) {
        self.record_selection = 0;
        self.records = RecordsState::Requested;
    }

    pub fn process_storage(&mut self, store: &Store) {
        if matches!(self.config_save, SaveState::Pending) {
            self.notice = None;
            self.config_save = store
                .save_config(&self.config)
                .map_err(|error| format!("{error:#}"))
                .into();
            if let SaveState::Failed(error) = &self.config_save {
                self.notice = Some(format!("Settings not saved: {error}. R retries."));
            }
        }
        if let Some(settings) = &mut self.settings
            && matches!(settings.save, SaveState::Pending)
        {
            match store.save_config(&settings.config) {
                Ok(()) => {
                    self.config = settings.config.clone();
                    self.config_save = SaveState::Saved;
                    self.settings = None;
                    self.screen = Screen::Menu;
                    self.notice = Some("Settings saved".into());
                }
                Err(error) => {
                    settings.save =
                        SaveState::Failed(format!("Save failed: {error:#}. Select SAVE to retry."));
                }
            }
        }
        if matches!(self.personal_best, PersonalBest::Requested)
            && let Some(record) = &self.record
        {
            self.personal_best = match store.personal_best(record) {
                Ok(best) => PersonalBest::Loaded(best),
                Err(error) => PersonalBest::Failed(format!("{error:#}")),
            };
        }
        if matches!(self.save, SaveState::Pending)
            && let Some(record) = &self.record
        {
            self.save = store
                .save(record)
                .map_err(|error| format!("{error:#}"))
                .into();
        }
        // Complete the synchronous query here, e.g. mode changes cannot leave stale rows visible.
        if matches!(self.records, RecordsState::Requested) {
            self.records = match store.records(self.record_mode) {
                Ok(records) => {
                    self.record_selection = self
                        .record
                        .as_ref()
                        .and_then(|current| records.iter().position(|row| row.id == current.id))
                        .unwrap_or_default();
                    RecordsState::Loaded(records)
                }
                Err(error) => RecordsState::Failed(format!("{error:#}")),
            };
        }
    }

    pub fn finish_session(
        &mut self,
        store: &Store,
        result: miette::Result<()>,
    ) -> miette::Result<()> {
        // Preserve a result created just before a terminal failure, e.g. by the final hard drop.
        self.process_storage(store);
        let mut failures = Vec::new();
        if let SaveState::Failed(error) = &self.config_save {
            failures.push(format!("Unsaved settings: {error}"));
        }
        if let Some(settings) = &self.settings
            && let SaveState::Failed(error) = &settings.save
        {
            // Preserve an explicitly saved draft on disconnect, e.g. after SQLite rejected the write.
            failures.push(format!(
                "Unsaved settings: {error}\nUnsaved draft: {:?}",
                settings.config
            ));
        }
        if let SaveState::Failed(error) = &self.save {
            // Keep both failures so a disconnected terminal cannot hide the unsaved score.
            failures.push(format!("{error}\nUnsaved result: {:?}", self.record));
        }
        if !failures.is_empty() {
            let unsaved = failures.join("\n");
            return match result {
                Ok(()) => Err(miette::miette!("{unsaved}")),
                Err(error) => Err(error.wrap_err(unsaved)),
            };
        }
        result
    }

    pub fn advance(&mut self, mut duration: Duration) -> miette::Result<()> {
        if self.screen != Screen::Playing {
            return Ok(());
        }
        let Some(game) = &mut self.game else {
            return Ok(());
        };
        // Interleave repeats with game time, e.g. a slow frame must not drop before moving.
        while !duration.is_zero() && game.status() == Status::Playing {
            let horizontal = self.horizontal_keys.last().map(|&(_, action)| action);
            let mut step = duration.min(Duration::from_millis(10));
            if horizontal.is_some() {
                step = step.min(self.horizontal_wait);
            }
            game.advance(step);
            duration = duration.saturating_sub(step);
            self.horizontal_wait = self.horizontal_wait.saturating_sub(step);
            if let Some(action) = horizontal
                && self.horizontal_wait.is_zero()
                && game.status() == Status::Playing
            {
                repeat_action(game, action, self.config.arr_ms == 0);
                self.horizontal_wait = Duration::from_millis(self.config.arr_ms.max(1));
            }
        }
        self.finish_if_needed()?;
        Ok(())
    }

    fn finish_if_needed(&mut self) -> miette::Result<()> {
        if let Some(game) = &self.game
            && game.status() != Status::Playing
            && self.record.is_none()
        {
            self.record = Some(Record::from_game(game, &self.config.player)?);
            self.save = SaveState::Pending;
            self.personal_best = PersonalBest::Requested;
            self.screen = Screen::GameOver;
            self.dialog_selection = 0;
            self.clear_held_keys();
        }
        Ok(())
    }

    pub fn pause_for_resize(&mut self) {
        self.clear_held_keys();
        self.pressed_actions.clear();
        match self.screen {
            Screen::Playing => {
                self.screen = Screen::Paused;
                self.dialog_selection = 0;
            }
            // Closing help after a resize must still require an explicit resume.
            Screen::Help(_) => {
                self.screen = Screen::Help(HelpReturn::Paused);
                self.dialog_selection = 0;
            }
            _ => {}
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if key.kind == KeyEventKind::Release {
            let released = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                code => code,
            };
            self.pressed_actions.retain(|&code| code != released);
            let previous = self.horizontal_keys.last().copied();
            self.horizontal_keys.retain(|&(code, _)| code != released);
            if self.horizontal_keys.last().copied() != previous {
                // Releasing the newest direction recharges DAS for the remaining key.
                self.horizontal_wait = Duration::from_millis(self.config.das_ms);
            }
            self.soft_drop_keys.retain(|&code| match (code, key.code) {
                (KeyCode::Char(held), KeyCode::Char(released)) => {
                    !held.eq_ignore_ascii_case(&released)
                }
                _ => code != key.code,
            });
            if let Some(game) = &mut self.game {
                game.set_soft_drop(!self.soft_drop_keys.is_empty());
            }
            return Ok(());
        }
        if matches!(self.screen, Screen::GameOver | Screen::Statistics)
            && matches!(key.code, KeyCode::Enter | KeyCode::Esc)
        {
            if key.kind == KeyEventKind::Repeat
                || self.keyboard_releases && self.pressed_actions.contains(&key.code)
            {
                return Ok(());
            }
            // Keep a held Enter from opening and then closing statistics, even on repeated Press.
            if self.keyboard_releases {
                self.pressed_actions.push(key.code);
            }
        }
        match self.screen {
            Screen::Menu => self.menu_key(key),
            Screen::Playing => self.playing_key(key)?,
            Screen::Help(return_to) => {
                if key.code == KeyCode::Esc || Keys::matches(&self.config.keys.help, key) {
                    self.screen = match return_to {
                        HelpReturn::Playing => Screen::Playing,
                        HelpReturn::Paused => Screen::Paused,
                    };
                }
            }
            Screen::Paused => self.pause_key(key),
            Screen::GameOver => self.result_key(key),
            Screen::Statistics => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.screen = Screen::GameOver,
                KeyCode::Char('r') if matches!(self.personal_best, PersonalBest::Failed(_)) => {
                    self.personal_best = PersonalBest::Requested;
                }
                _ => {}
            },
            Screen::Records => self.records_key(key),
            Screen::Settings => self.settings_key(key),
        }
        Ok(())
    }

    fn settings_key(&mut self, key: KeyEvent) {
        let capturing = self.settings.as_ref().is_some_and(|settings| {
            matches!(
                settings.edit,
                Some(Edit::Keys {
                    capturing: true,
                    ..
                })
            )
        });
        let once = capturing
            || matches!(
                key.code,
                KeyCode::Enter | KeyCode::Esc | KeyCode::F(2) | KeyCode::Tab | KeyCode::Delete
            );
        if once && key.kind == KeyEventKind::Repeat {
            return;
        }
        if self.keyboard_releases && key.kind == KeyEventKind::Press {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                code => code,
            };
            let held = self.pressed_actions.contains(&code);
            if !held {
                self.pressed_actions.push(code);
            }
            // Some terminals repeat Press, e.g. holding F2 must not accept and then save a draft.
            if once && held {
                return;
            }
        }
        if let Some(settings) = &mut self.settings
            && settings.key(key)
        {
            self.settings = None;
            self.screen = Screen::Menu;
            self.notice = None;
        }
    }

    fn menu_key(&mut self, key: KeyEvent) {
        if self.name_draft.is_some() {
            self.edit_name(key);
            return;
        }
        match key.code {
            KeyCode::Char('r') if matches!(self.config_save, SaveState::Failed(_)) => {
                self.config_save = SaveState::Pending;
                self.notice = None;
            }
            KeyCode::Up => self.menu_selection = self.menu_selection.previous(),
            KeyCode::Down | KeyCode::Tab => self.menu_selection = self.menu_selection.next(),
            KeyCode::Left => self.change_setting(-1),
            KeyCode::Right => self.change_setting(1),
            KeyCode::Esc => self.quit = true,
            KeyCode::Enter => match self.menu_selection {
                MenuItem::Player => self.name_draft = Some(self.config.player.clone()),
                MenuItem::Mode => self.change_setting(1),
                MenuItem::Start => self.start(),
                MenuItem::Records => self.open_records(self.config.mode),
                MenuItem::Settings => {
                    self.settings = Some(Settings::new(self.config.clone()));
                    self.screen = Screen::Settings;
                    self.notice = None;
                    self.pressed_actions.clear();
                    if self.keyboard_releases {
                        self.pressed_actions.push(KeyCode::Enter);
                    }
                }
                MenuItem::Quit => self.quit = true,
            },
            _ => {}
        }
    }

    fn edit_name(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.name_draft = None;
            self.notice = None;
            return;
        }
        if key.code == KeyCode::Enter {
            let Some(draft) = &self.name_draft else {
                return;
            };
            if let Err(error) = validate_player(draft) {
                self.notice = Some(error);
                return;
            }
            self.config.player = draft.trim().to_owned();
            self.config_save = SaveState::Pending;
            self.name_draft = None;
            self.notice = None;
            self.menu_selection = MenuItem::Mode;
        } else if let Some(draft) = &mut self.name_draft {
            edit_name_input(draft, key);
        }
    }

    fn change_setting(&mut self, delta: i32) {
        if self.menu_selection == MenuItem::Mode {
            self.config.mode = self.config.mode.cycle(delta);
            self.config_save = SaveState::Pending;
        }
    }

    fn playing_key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if Keys::matches(&self.config.keys.pause, key) {
            self.clear_held_keys();
            self.screen = Screen::Paused;
            self.dialog_selection = 0;
            return Ok(());
        }
        if Keys::matches(&self.config.keys.help, key) {
            self.clear_held_keys();
            self.screen = Screen::Help(HelpReturn::Playing);
            return Ok(());
        }
        let Some(action) = self.config.keys.action(key) else {
            return Ok(());
        };
        if self.keyboard_releases
            && matches!(action, Action::Left | Action::Right | Action::SoftDrop)
        {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                code => code,
            };
            if key.kind != KeyEventKind::Press
                || self.horizontal_keys.iter().any(|&(held, _)| held == code)
                || self.soft_drop_keys.contains(&code)
            {
                return Ok(());
            }
            if action == Action::SoftDrop && !self.soft_drop_keys.is_empty() {
                self.soft_drop_keys.push(code);
                return Ok(());
            }
            if action == Action::SoftDrop {
                self.soft_drop_keys.push(code);
            } else {
                self.horizontal_keys.push((code, action));
                self.horizontal_wait = Duration::from_millis(self.config.das_ms);
            }
            if action == Action::SoftDrop
                && let Some(game) = &mut self.game
            {
                game.set_soft_drop_interval(Duration::from_millis(self.config.soft_drop_ms));
                game.set_soft_drop(true);
            }
            if let Some(game) = &mut self.game {
                repeat_action(
                    game,
                    action,
                    action == Action::SoftDrop && self.config.soft_drop_ms == 0
                        || action != Action::SoftDrop
                            && self.config.das_ms == 0
                            && self.config.arr_ms == 0,
                );
            }
            return self.finish_if_needed();
        }
        if key.kind == KeyEventKind::Repeat
            && !matches!(action, Action::Left | Action::Right | Action::SoftDrop)
        {
            return Ok(());
        }
        if self.keyboard_releases
            && !matches!(action, Action::Left | Action::Right | Action::SoftDrop)
        {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                code => code,
            };
            // Windows reports auto-repeat as Press too; e.g. held Space must lock only once.
            if self.pressed_actions.contains(&code) {
                return Ok(());
            }
            self.pressed_actions.push(code);
        }
        let Some(game) = &mut self.game else {
            return Ok(());
        };
        game.apply(action);
        self.finish_if_needed()
    }

    fn clear_held_keys(&mut self) {
        // Focus loss or a dialog can swallow releases, e.g. Alt-Tab while holding Down.
        self.soft_drop_keys.clear();
        self.horizontal_keys.clear();
        self.horizontal_wait = Duration::ZERO;
        if let Some(game) = &mut self.game {
            game.set_soft_drop(false);
        }
    }

    fn pause_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Down => self.dialog_selection = 1 - self.dialog_selection,
            KeyCode::Esc => self.screen = Screen::Playing,
            KeyCode::Enter if self.dialog_selection == 0 => self.screen = Screen::Playing,
            KeyCode::Enter => self.return_to_menu(),
            _ if Keys::matches(&self.config.keys.pause, key) => self.screen = Screen::Playing,
            _ => {}
        }
    }

    fn result_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('r') && matches!(self.save, SaveState::Failed(_)) {
            self.save = SaveState::Pending;
            self.notice = None;
            return;
        }
        match key.code {
            KeyCode::Up => self.dialog_selection = (self.dialog_selection + 3) % 4,
            KeyCode::Down => self.dialog_selection = (self.dialog_selection + 1) % 4,
            KeyCode::Enter if self.dialog_selection == 1 => {
                self.screen = Screen::Statistics;
                self.notice = None;
            }
            KeyCode::Enter | KeyCode::Esc if !matches!(self.save, SaveState::Saved) => {
                self.notice =
                    Some("Keep this result open until saved. R retries a failed save.".into());
            }
            KeyCode::Esc => self.return_to_menu(),
            KeyCode::Enter => match self.dialog_selection {
                0 => self.start(),
                2 => self.open_records(self.config.mode),
                _ => self.return_to_menu(),
            },
            _ => {}
        }
    }

    fn return_to_menu(&mut self) {
        self.screen = Screen::Menu;
        self.menu_selection = MenuItem::Start;
        self.game = None;
        self.record = None;
        self.save = SaveState::None;
        self.notice = None;
    }

    fn records_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.screen = self.records_back,
            KeyCode::Left | KeyCode::Right => {
                self.record_mode = if self.record_mode == Mode::FortyLines {
                    Mode::Blitz
                } else {
                    Mode::FortyLines
                };
                self.request_records();
            }
            KeyCode::Up => self.record_selection = self.record_selection.saturating_sub(1),
            KeyCode::Down => {
                self.record_selection =
                    (self.record_selection + 1).min(self.records.rows().len().saturating_sub(1))
            }
            KeyCode::PageUp => self.record_selection = self.record_selection.saturating_sub(10),
            KeyCode::PageDown => {
                self.record_selection =
                    (self.record_selection + 10).min(self.records.rows().len().saturating_sub(1))
            }
            KeyCode::Char('r') if matches!(self.records, RecordsState::Failed(_)) => {
                self.request_records()
            }
            _ => {}
        }
    }
}

fn repeat_action(game: &mut Game, action: Action, instant: bool) {
    let limit = if instant { crate::game::HEIGHT } else { 1 };
    let board = instant.then(|| *game.board());
    for _ in 0..limit {
        let before = game.active();
        game.apply(action);
        let after = game.active();
        // Stop at an obstacle or a newly spawned piece, e.g. an exhausted lock delay.
        let moved = match action {
            Action::Left => after.x == before.x - 1 && after.y == before.y,
            Action::Right => after.x == before.x + 1 && after.y == before.y,
            Action::SoftDrop => after.y == before.y + 1 && after.x == before.x,
            _ => false,
        };
        if !moved
            || game.status() != Status::Playing
            || board.as_ref().is_some_and(|before| before != game.board())
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case(Mode::Practice, 1)]
    #[case(Mode::FortyLines, 0)]
    #[case(Mode::Blitz, 0)]
    fn starts_and_restarts_apply_speed_only_to_practice(#[case] mode: Mode, #[case] rows: i32) {
        let mut app = App::try_from(Config {
            mode,
            practice_speed_percent: 200,
            ..Config::default()
        })
        .unwrap();
        for _ in 0..2 {
            app.start();
            let y = app.game.as_ref().unwrap().active().y;
            app.advance(Duration::from_millis(500)).unwrap();
            assert_eq!(app.game.as_ref().unwrap().active().y, y + rows);
        }
    }

    fn release(app: &mut App, code: KeyCode) {
        app.key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
        .unwrap();
    }

    #[rstest::rstest]
    #[case(KeyCode::Left, -1)]
    #[case(KeyCode::Right, 1)]
    fn horizontal_repeat_obeys_das_arr_and_release(#[case] code: KeyCode, #[case] delta: i32) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = true;
        app.start();
        let x = app.game.as_ref().unwrap().active().x;
        app.key(key(code)).unwrap();
        for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
            app.key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind))
                .unwrap();
        }
        assert_eq!(app.game.as_ref().unwrap().active().x, x + delta);
        app.advance(Duration::from_millis(149)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x + delta);
        app.advance(Duration::from_millis(1)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x + 2 * delta);
        app.advance(Duration::from_millis(29)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x + 2 * delta);
        app.advance(Duration::from_millis(1)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x + 3 * delta);
        release(&mut app, code);
        app.advance(Duration::from_millis(300)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x + 3 * delta);
    }

    #[test]
    fn latest_direction_wins_and_release_recharges_the_remaining_binding() {
        let mut config = Config::default();
        config.set("keys.left", "left, j").unwrap();
        let mut app = App::try_from(config).unwrap();
        app.keyboard_releases = true;
        app.start();
        let x = app.game.as_ref().unwrap().active().x;
        app.key(key(KeyCode::Left)).unwrap();
        app.key(key(KeyCode::Right)).unwrap();
        app.advance(Duration::from_millis(150)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x + 1);
        app.key(key(KeyCode::Char('j'))).unwrap();
        release(&mut app, KeyCode::Left);
        app.advance(Duration::from_millis(150)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x - 1);
        release(&mut app, KeyCode::Char('J'));
        app.advance(Duration::from_millis(149)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x - 1);
        app.advance(Duration::from_millis(1)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x);
    }

    #[rstest::rstest]
    #[case(KeyCode::Left, 0)]
    #[case(KeyCode::Right, 9)]
    fn zero_arr_moves_to_the_wall_after_das(#[case] code: KeyCode, #[case] edge: i32) {
        let mut app = App::try_from(Config {
            arr_ms: 0,
            ..Config::default()
        })
        .unwrap();
        app.keyboard_releases = true;
        app.start();
        app.key(key(code)).unwrap();
        app.advance(Duration::from_millis(149)).unwrap();
        assert!(
            !app.game
                .as_ref()
                .unwrap()
                .active()
                .cells()
                .iter()
                .any(|&(x, _)| x == edge)
        );
        app.advance(Duration::from_millis(1)).unwrap();
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .active()
                .cells()
                .iter()
                .any(|&(x, _)| x == edge)
        );
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .board()
                .iter()
                .flatten()
                .all(Option::is_none)
        );
    }

    #[test]
    fn zero_das_and_arr_move_to_the_wall_on_press() {
        let mut app = App::try_from(Config {
            das_ms: 0,
            arr_ms: 0,
            ..Config::default()
        })
        .unwrap();
        app.keyboard_releases = true;
        app.start();
        app.key(key(KeyCode::Left)).unwrap();
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .active()
                .cells()
                .iter()
                .any(|&(x, _)| x == 0)
        );
        app.advance(Duration::from_millis(20)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
    }

    #[rstest::rstest]
    fn soft_drop_repeats_at_its_interval_without_das_or_os_repeats(
        #[values(400, 820)] initial_ms: u64,
    ) {
        let mut app = App::try_from(Config {
            das_ms: 1000,
            soft_drop_ms: 20,
            ..Config::default()
        })
        .unwrap();
        app.keyboard_releases = true;
        app.start();
        // Press between gravity ticks, e.g. holding Down must not inherit an old timer.
        app.advance(Duration::from_millis(initial_ms)).unwrap();
        let y = app.game.as_ref().unwrap().active().y;
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))
        .unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, y + 1);
        app.advance(Duration::from_millis(19)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, y + 1);
        app.advance(Duration::from_millis(1)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, y + 2);
        app.advance(Duration::from_millis(40)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, y + 4);
        assert_eq!(app.game.as_ref().unwrap().score(), 4);
        release(&mut app, KeyCode::Down);
        app.advance(Duration::from_millis(100)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, y + 4);
    }

    #[test]
    fn held_soft_drop_keeps_natural_gravity_and_awards_each_descended_cell() {
        let mut app = App::try_from(Config {
            soft_drop_ms: 1000,
            ..Config::default()
        })
        .unwrap();
        app.keyboard_releases = true;
        app.start();
        let y = app.game.as_ref().unwrap().active().y;
        app.key(key(KeyCode::Down)).unwrap();
        app.advance(Duration::from_millis(900)).unwrap();
        let game = app.game.as_ref().unwrap();
        assert_eq!(game.active().y, y + 2);
        assert_eq!(game.score(), 2);
    }

    #[test]
    fn zero_soft_drop_reaches_floor_and_preserves_lock_delay_and_points() {
        let mut app = App::try_from(Config {
            soft_drop_ms: 0,
            ..Config::default()
        })
        .unwrap();
        app.keyboard_releases = true;
        app.start();
        let game = app.game.as_ref().unwrap();
        let initial = game.active();
        let floor = game.ghost();
        app.key(key(KeyCode::Down)).unwrap();
        let game = app.game.as_ref().unwrap();
        assert_eq!(game.active(), floor);
        assert_eq!(game.score(), u64::try_from(floor.y - initial.y).unwrap());
        assert!(game.board().iter().flatten().all(Option::is_none));
        release(&mut app, KeyCode::Down);
        app.advance(Duration::from_millis(499)).unwrap();
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .board()
                .iter()
                .flatten()
                .all(Option::is_none)
        );
        app.advance(Duration::from_millis(1)).unwrap();
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .board()
                .iter()
                .flatten()
                .any(Option::is_some)
        );
    }

    #[test]
    fn repeat_timing_is_preserved_across_frames_with_both_axes_held() {
        let make_app = || {
            let mut app = App::try_from(Config::default()).unwrap();
            app.keyboard_releases = true;
            app.start();
            app.game = Some(Game::new(Mode::FortyLines, 42));
            app.key(key(KeyCode::Right)).unwrap();
            app.key(key(KeyCode::Down)).unwrap();
            app
        };
        let mut slow = make_app();
        let mut fast = make_app();
        slow.advance(Duration::from_millis(300)).unwrap();
        for _ in 0..300 {
            fast.advance(Duration::from_millis(1)).unwrap();
        }
        let a = slow.game.as_ref().unwrap();
        let b = fast.game.as_ref().unwrap();
        assert_eq!(a.active(), b.active());
        assert_eq!(a.board(), b.board());
        assert_eq!(a.score(), b.score());
        assert_eq!(a.elapsed(), b.elapsed());
    }

    #[test]
    fn legacy_horizontal_input_keeps_os_repeat_without_latching() {
        let mut app = App::try_from(Config {
            das_ms: 0,
            arr_ms: 0,
            ..Config::default()
        })
        .unwrap();
        app.start();
        let x = app.game.as_ref().unwrap().active().x;
        app.key(key(KeyCode::Left)).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Left,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))
        .unwrap();
        app.advance(Duration::from_millis(300)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x - 2);
        assert!(app.horizontal_keys.is_empty());
    }

    #[rstest::rstest]
    #[case(false, KeyEventKind::Repeat)]
    #[case(true, KeyEventKind::Press)]
    fn settings_one_shot_keys_ignore_auto_repeat(
        #[case] releases: bool,
        #[case] repeat_kind: KeyEventKind,
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = releases;
        app.menu_selection = MenuItem::Settings;
        app.key(key(KeyCode::Enter)).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            repeat_kind,
        ))
        .unwrap();
        assert!(app.settings.as_ref().unwrap().edit.is_none());
        app.key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
        .unwrap();
        app.settings.as_mut().unwrap().selected = 8;
        app.key(key(KeyCode::Enter)).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            repeat_kind,
        ))
        .unwrap();
        assert!(matches!(
            app.settings.as_ref().unwrap().edit,
            Some(Edit::Keys {
                capturing: false,
                ..
            })
        ));
        app.key(key(KeyCode::F(2))).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::F(2),
            KeyModifiers::NONE,
            repeat_kind,
        ))
        .unwrap();
        assert!(matches!(
            app.settings.as_ref().unwrap().save,
            SaveState::None
        ));
        app.key(KeyEvent::new_with_kind(
            KeyCode::F(2),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
        .unwrap();
        app.key(key(KeyCode::F(2))).unwrap();
        assert!(matches!(
            app.settings.as_ref().unwrap().save,
            SaveState::Pending
        ));
    }

    #[rstest::rstest]
    fn failed_settings_draft_is_reported_after_terminal_failure(
        #[values(false, true)] already_processed: bool,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let store = Store::open(&path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.menu_selection = MenuItem::Settings;
        app.key(key(KeyCode::Enter)).unwrap();
        app.settings.as_mut().unwrap().config.player = "DRAFT".into();
        app.key(key(KeyCode::F(2))).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute("DROP TABLE app_settings", []).unwrap();
        if already_processed {
            app.process_storage(&store);
        }
        let error = app
            .finish_session(&store, Err(miette::miette!("terminal failed")))
            .unwrap_err();
        let message = format!("{error:#}");
        for expected in ["Unsaved settings", "DRAFT", "terminal failed"] {
            assert!(message.contains(expected), "{message}");
        }
        assert_eq!(app.config.player, "PLAYER");
    }

    #[test]
    fn menu_choices_persist_and_shutdown_reports_both_failed_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let store = Store::open(&path).unwrap();
        let mut app = App::try_from(store.load_config().unwrap()).unwrap();
        app.menu_selection = MenuItem::Player;
        for code in [
            KeyCode::Enter,
            KeyCode::Delete,
            KeyCode::Char('김'),
            KeyCode::Enter,
            KeyCode::Right,
        ] {
            app.key(key(code)).unwrap();
        }
        app.process_storage(&store);
        assert_eq!(store.load_config().unwrap(), app.config);
        assert_eq!(app.config.player, "김");
        assert_eq!(app.config.mode, Mode::Blitz);
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        app.config_save = SaveState::Pending;
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("DROP TABLE app_settings; DROP TABLE game_records;")
            .unwrap();
        let error = app
            .finish_session(&store, Err(miette::miette!("terminal failed")))
            .unwrap_err();
        let message = format!("{error:#}");
        for expected in ["Unsaved settings", "Unsaved result", "terminal failed"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn settings_save_cancel_and_failure_keep_the_correct_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let store = Store::open(&path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.menu_selection = MenuItem::Settings;
        app.key(key(KeyCode::Enter)).unwrap();
        let draft = app.settings.as_mut().unwrap();
        draft.config.ghost_enabled = false;
        draft.config.keys.left = draft.config.keys.right.clone();
        app.key(key(KeyCode::F(2))).unwrap();
        assert!(app.settings.as_ref().unwrap().error.is_some());
        assert!(!matches!(
            app.settings.as_ref().unwrap().save,
            SaveState::Pending
        ));
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.config, Config::default());
        app.key(key(KeyCode::Enter)).unwrap();
        app.settings.as_mut().unwrap().config.ghost_enabled = false;
        app.key(key(KeyCode::F(2))).unwrap();
        app.process_storage(&store);
        assert_eq!(app.screen, Screen::Menu);
        assert!(!app.config.ghost_enabled);
        assert_eq!(store.load_config().unwrap(), app.config);
        app.key(key(KeyCode::Enter)).unwrap();
        app.settings.as_mut().unwrap().config.next_queue_length = 2;
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute("ALTER TABLE app_settings RENAME TO unavailable", [])
            .unwrap();
        app.key(key(KeyCode::F(2))).unwrap();
        app.process_storage(&store);
        assert_eq!(app.screen, Screen::Settings);
        assert_eq!(app.config.next_queue_length, 6);
        assert_eq!(app.settings.as_ref().unwrap().config.next_queue_length, 2);
        assert!(matches!(
            app.settings.as_ref().unwrap().save,
            SaveState::Failed(_)
        ));
        db.execute("ALTER TABLE unavailable RENAME TO app_settings", [])
            .unwrap();
        app.key(key(KeyCode::F(2))).unwrap();
        app.process_storage(&store);
        assert_eq!(store.load_config().unwrap().next_queue_length, 2);
    }

    #[test]
    fn practice_continues_without_save_and_records_skip_practice() {
        let mut app = App::try_from(Config {
            mode: Mode::Practice,
            ..Config::default()
        })
        .unwrap();
        app.start();
        for _ in 0..100 {
            app.key(key(KeyCode::Char(' '))).unwrap();
        }
        assert!(app.game.as_ref().unwrap().practice_resets() > 0);
        assert_eq!(app.screen, Screen::Playing);
        assert!(app.record.is_none());
        assert!(matches!(app.save, SaveState::None));
        app.open_records(Mode::Practice);
        assert_eq!(app.record_mode, Mode::FortyLines);
        app.key(key(KeyCode::Right)).unwrap();
        assert_eq!(app.record_mode, Mode::Blitz);
        app.key(key(KeyCode::Right)).unwrap();
        assert_eq!(app.record_mode, Mode::FortyLines);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[rstest::rstest]
    #[case(false)]
    #[case(true)]
    fn ctrl_c_holds_without_quitting(#[case] keyboard_releases: bool) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = keyboard_releases;
        app.start();
        let initial_piece = app.game.as_ref().unwrap().active().kind;

        // Ctrl rotation must allow a following C hold without exiting the game.
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();

        assert!(!app.quit);
        assert_eq!(app.screen, Screen::Playing);
        assert_eq!(app.game.as_ref().unwrap().held(), Some(initial_piece));
    }

    #[test]
    fn held_soft_drop_stops_only_after_all_bindings_release() {
        let mut config = Config::default();
        config.set("keys.soft_drop", "down, s").unwrap();
        let mut app = App::try_from(config).unwrap();
        app.keyboard_releases = true;
        app.start();
        let initial_y = app.game.as_ref().unwrap().active().y;
        app.key(key(KeyCode::Down)).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))
        .unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, initial_y + 1);
        app.key(key(KeyCode::Char('s'))).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ))
        .unwrap();
        assert!(!app.soft_drop_keys.is_empty());
        assert!(app.game.as_ref().unwrap().soft_drop());
        app.advance(Duration::from_millis(50)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, initial_y + 2);
        app.key(KeyEvent::new_with_kind(
            KeyCode::Char('S'),
            KeyModifiers::SHIFT,
            KeyEventKind::Release,
        ))
        .unwrap();
        assert!(app.soft_drop_keys.is_empty());
        assert!(!app.game.as_ref().unwrap().soft_drop());
    }

    #[test]
    fn legacy_soft_drop_steps_on_press_and_repeat_without_latching() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        let initial_y = app.game.as_ref().unwrap().active().y;
        for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
            app.key(KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::NONE,
                kind,
            ))
            .unwrap();
        }
        let game = app.game.as_ref().unwrap();
        assert_eq!(game.active().y, initial_y + 2);
        assert!(!game.soft_drop());
    }

    #[rstest::rstest]
    fn rapid_half_turns_preserve_gravity(
        #[values(false, true)] keyboard_releases: bool,
        #[values(Mode::FortyLines, Mode::Blitz, Mode::Practice)] mode: Mode,
    ) {
        for seed in 0..32 {
            let mut app = App::try_from(Config {
                mode,
                ..Config::default()
            })
            .unwrap();
            app.keyboard_releases = keyboard_releases;
            app.start();
            app.game = Some(Game::new(mode, seed));
            let mut idle = Game::new(mode, seed);
            // Compare equal elapsed time, e.g. 100 taps must not postpone the next gravity tick.
            for _ in 0..100 {
                app.key(key(KeyCode::Char('a'))).unwrap();
                if keyboard_releases {
                    release(&mut app, KeyCode::Char('a'));
                }
                app.advance(Duration::from_millis(20)).unwrap();
                idle.advance(Duration::from_millis(20));
                let actual = app.game.as_ref().unwrap();
                assert_eq!(actual.active().y, idle.active().y, "seed {seed}");
                assert_eq!(actual.elapsed(), idle.elapsed());
            }
            let actual = app.game.as_ref().unwrap();
            assert_eq!(actual.active(), idle.active(), "seed {seed}");
            assert!(actual.active().y > Game::new(mode, seed).active().y);
        }
    }

    #[test]
    fn repeated_press_does_not_hard_drop_again_until_release() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = true;
        app.start();
        app.key(key(KeyCode::Char(' '))).unwrap();
        let board = *app.game.as_ref().unwrap().board();
        let active = app.game.as_ref().unwrap().active();
        app.key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.game.as_ref().unwrap().board(), &board);
        assert_eq!(app.game.as_ref().unwrap().active(), active);
        app.key(KeyEvent::new_with_kind(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
        .unwrap();
        app.key(key(KeyCode::Char(' '))).unwrap();
        assert_ne!(app.game.as_ref().unwrap().board(), &board);
    }

    #[rstest::rstest]
    #[case(Some(KeyCode::Esc))]
    #[case(Some(KeyCode::Char('?')))]
    #[case(None)]
    fn leaving_play_clears_held_soft_drop(#[case] dialog: Option<KeyCode>) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = true;
        app.start();
        app.key(key(KeyCode::Down)).unwrap();
        assert!(!app.soft_drop_keys.is_empty());
        assert!(app.game.as_ref().unwrap().soft_drop());
        app.key(key(KeyCode::Left)).unwrap();
        let x = app.game.as_ref().unwrap().active().x;
        if let Some(code) = dialog {
            app.key(key(code)).unwrap();
        } else {
            app.pause_for_resize();
        }
        assert!(app.soft_drop_keys.is_empty());
        assert!(!app.game.as_ref().unwrap().soft_drop());
        assert!(app.horizontal_keys.is_empty());
        app.key(key(KeyCode::Esc)).unwrap();
        app.advance(Duration::from_millis(300)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().x, x);
        app.key(key(KeyCode::Down)).unwrap();
        assert!(!app.soft_drop_keys.is_empty());
        assert!(app.game.as_ref().unwrap().soft_drop());
    }

    #[rstest::rstest]
    #[case(KeyCode::Up)]
    #[case(KeyCode::Char('z'))]
    #[case(KeyCode::Char('a'))]
    #[case(KeyCode::Char(' '))]
    #[case(KeyCode::Char('c'))]
    fn one_shot_actions_ignore_reported_repeats(#[case] code: KeyCode) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        let before = app.game.as_ref().unwrap().active();
        app.key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))
        .unwrap();
        assert_eq!(app.game.as_ref().unwrap().active(), before);
        assert_eq!(app.game.as_ref().unwrap().held(), None);
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .board()
                .iter()
                .flatten()
                .all(Option::is_none)
        );
    }

    #[rstest::rstest]
    #[case(KeyCode::Down)]
    #[case(KeyCode::Tab)]
    fn menu_navigation_wraps_and_up_reverses_it(#[case] forward: KeyCode) {
        let mut app = App::try_from(Config::default()).unwrap();
        for expected in [
            MenuItem::Records,
            MenuItem::Settings,
            MenuItem::Quit,
            MenuItem::Player,
            MenuItem::Mode,
            MenuItem::Start,
        ] {
            let previous = app.menu_selection;
            app.key(key(forward)).unwrap();
            assert_eq!(app.menu_selection, expected);
            app.key(key(KeyCode::Up)).unwrap();
            assert_eq!(app.menu_selection, previous);
            app.key(key(forward)).unwrap();
        }
    }

    #[test]
    fn menu_settings_and_commands_follow_the_selected_item() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.menu_selection = MenuItem::Player;
        app.key(key(KeyCode::Enter)).unwrap();
        app.name_draft = Some(" Alice ".into());
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.config.player, "Alice");
        assert_eq!(app.menu_selection, MenuItem::Mode);

        app.key(key(KeyCode::Right)).unwrap();
        assert_eq!(app.config.mode, Mode::Blitz);
        app.key(key(KeyCode::Left)).unwrap();
        assert_eq!(app.config.mode, Mode::FortyLines);
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
        assert_eq!(app.game.as_ref().unwrap().mode(), Mode::FortyLines);

        app.key(key(KeyCode::Esc)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Menu);
        assert_eq!(app.menu_selection, MenuItem::Start);
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Records);
        assert_eq!(app.record_mode, Mode::FortyLines);
        assert!(matches!(app.records, RecordsState::Requested));
        app.key(key(KeyCode::Esc)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert!(app.quit);
    }

    #[test]
    fn pause_freezes_time_and_resume_keeps_board() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        app.advance(Duration::from_millis(200)).unwrap();
        let before = app.game.as_ref().unwrap().active();
        app.key(key(KeyCode::Esc)).unwrap();
        app.advance(Duration::from_secs(20)).unwrap();
        assert_eq!(
            app.game.as_ref().unwrap().elapsed(),
            Duration::from_millis(200)
        );
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active(), before);
        assert_eq!(app.screen, Screen::Playing);
    }

    #[test]
    fn finished_game_cannot_be_replaced_before_save() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
        let id = app.record.as_ref().unwrap().id.clone();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.record.as_ref().unwrap().id, id);
        app.save = Err("disk full".into()).into();
        app.key(key(KeyCode::Char('r'))).unwrap();
        assert!(matches!(app.save, SaveState::Pending));
        assert_eq!(app.record.as_ref().unwrap().id, id);
        app.save = Ok(()).into();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
    }

    #[test]
    fn statistics_keep_the_unsaved_result_and_return_to_the_summary() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        let record = app.record.clone();
        // A random stack can top out early; statistics must preserve the actual finish time.
        let elapsed = app.game.as_ref().unwrap().elapsed();
        app.save = SaveState::Failed("disk full".into());
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Statistics);
        app.advance(Duration::from_secs(30)).unwrap();
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
        assert_eq!(app.record, record);
        assert_eq!(app.game.as_ref().unwrap().elapsed(), elapsed);
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
        app.save = SaveState::Saved;
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Records);
        app.key(key(KeyCode::Esc)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Menu);
    }

    #[rstest::rstest]
    fn result_navigation_waits_for_a_fresh_press(
        #[values(false, true)] keyboard_releases: bool,
        #[values(KeyCode::Enter, KeyCode::Esc)] back: KeyCode,
    ) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = keyboard_releases;
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        app.save = SaveState::Saved;
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        let repeated = |code| {
            KeyEvent::new_with_kind(
                code,
                KeyModifiers::NONE,
                if keyboard_releases {
                    KeyEventKind::Press
                } else {
                    KeyEventKind::Repeat
                },
            )
        };
        app.key(repeated(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Statistics);
        release(&mut app, KeyCode::Enter);
        app.key(key(back)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
        app.key(repeated(back)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
        release(&mut app, back);
        app.key(key(back)).unwrap();
        assert_eq!(
            app.screen,
            if back == KeyCode::Enter {
                Screen::Statistics
            } else {
                Screen::Menu
            }
        );
    }

    #[test]
    fn previous_best_survives_saving_and_query_retries_exclude_the_current_run() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.key(key(KeyCode::Char(' '))).unwrap();
        app.advance(Duration::from_secs(120)).unwrap();
        let previous = Record {
            id: "previous".into(),
            score: 0,
            ..app.record.clone().unwrap()
        };
        store.save(&previous).unwrap();
        app.process_storage(&store);
        assert!(
            matches!(&app.personal_best, PersonalBest::Loaded(Some(best)) if best == &previous)
        );
        app.process_storage(&store);
        assert!(
            matches!(&app.personal_best, PersonalBest::Loaded(Some(best)) if best == &previous)
        );
        db.execute("ALTER TABLE game_records RENAME TO unavailable", [])
            .unwrap();
        app.personal_best = PersonalBest::Requested;
        app.process_storage(&store);
        assert!(matches!(app.personal_best, PersonalBest::Failed(_)));
        db.execute("ALTER TABLE unavailable RENAME TO game_records", [])
            .unwrap();
        app.screen = Screen::Statistics;
        app.key(key(KeyCode::Char('r'))).unwrap();
        app.process_storage(&store);
        assert!(
            matches!(&app.personal_best, PersonalBest::Loaded(Some(best)) if best == &previous)
        );
        app.start();
        assert!(matches!(app.personal_best, PersonalBest::Loaded(None)));
    }

    #[test]
    fn unicode_name_edit_can_be_cancelled() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.menu_selection = MenuItem::Player;
        app.key(key(KeyCode::Enter)).unwrap();
        app.key(key(KeyCode::Delete)).unwrap();
        app.key(key(KeyCode::Char('김'))).unwrap();
        app.key(key(KeyCode::Backspace)).unwrap();
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.config.player, "PLAYER");
    }

    #[test]
    fn help_freezes_game_and_resize_requires_explicit_resume() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        app.advance(Duration::from_millis(200)).unwrap();
        app.key(key(KeyCode::Char('?'))).unwrap();
        app.advance(Duration::from_secs(10)).unwrap();
        assert_eq!(
            app.game.as_ref().unwrap().elapsed(),
            Duration::from_millis(200)
        );
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
        app.key(key(KeyCode::Char('?'))).unwrap();
        app.pause_for_resize();
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::Paused);
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
    }

    #[test]
    fn records_load_once_and_mode_changes_replace_old_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.start();
        let row = Record::from_game(app.game.as_ref().unwrap(), "Alice").unwrap();
        store.save(&row).unwrap();
        app.open_records(Mode::FortyLines);
        app.process_storage(&store);
        assert_eq!(app.records.rows(), std::slice::from_ref(&row));
        // A later frame must not query again, e.g. after another connection changes the table.
        db.execute("DROP TABLE game_records", []).unwrap();
        app.process_storage(&store);
        assert_eq!(app.records.rows(), [row]);
        app.key(key(KeyCode::Right)).unwrap();
        assert!(app.records.rows().is_empty());
        app.process_storage(&store);
        assert!(matches!(app.records, RecordsState::Failed(_)));
    }

    #[test]
    fn records_retry_clears_failure_and_selects_the_current_result() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Blitz;
        app.start();
        app.advance(Duration::from_secs(120)).unwrap();
        let current = app.record.clone().unwrap();
        app.process_storage(&store);
        let higher = Record {
            id: "higher".into(),
            score: current.score + 1,
            ..current.clone()
        };
        store.save(&higher).unwrap();
        // Hide the table temporarily to exercise the real query failure and retry path.
        db.execute("ALTER TABLE game_records RENAME TO unavailable", [])
            .unwrap();
        app.open_records(Mode::Blitz);
        app.process_storage(&store);
        assert!(matches!(app.records, RecordsState::Failed(_)));
        db.execute("ALTER TABLE unavailable RENAME TO game_records", [])
            .unwrap();
        app.key(key(KeyCode::Char('r'))).unwrap();
        assert!(matches!(app.records, RecordsState::Requested));
        assert!(app.records.rows().is_empty());
        app.process_storage(&store);
        assert_eq!(app.record_selection, 1);
        assert_eq!(app.records.rows().get(app.record_selection), Some(&current));
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
    }
}
