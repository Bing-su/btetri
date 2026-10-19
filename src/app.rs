//! Screen transitions and pending storage work; held movement lives in `input`.

mod input;
mod settings;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use input::{HeldInput, normalized_code};
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
    /// Menu order shared by keyboard navigation and action rendering.
    pub const ALL: [Self; 6] = [
        Self::Player,
        Self::Mode,
        Self::Start,
        Self::Records,
        Self::Settings,
        Self::Quit,
    ];

    /// Return the menu label used by both rendering and navigation.
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

    fn cycle(self, backwards: bool) -> Self {
        let index = Self::ALL
            .iter()
            .position(|&item| item == self)
            .unwrap_or_default();
        let offset = if backwards { Self::ALL.len() - 1 } else { 1 };
        Self::ALL
            .get((index + offset) % Self::ALL.len())
            .copied()
            .unwrap_or(self)
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
    /// Expose loaded records; pending or failed requests have no visible rows.
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
    held_input: HeldInput,
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
            held_input: HeldInput::default(),
            pressed_actions: Vec::new(),
        })
    }
}

impl App {
    /// Start a fresh game with current settings and clear the previous result.
    /// Callers must finish saving a competitive result before replacing it.
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

    /// Request ranked records, remembering whether to return to the menu or result.
    /// Practice falls back to 40 LINES because it has no ranked records.
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

    /// Resolve pending reads and writes synchronously, retaining failures for retry.
    /// Read the previous best before saving this run so it cannot compare against itself.
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

    /// Flush pending writes and report unsaved drafts/results alongside terminal errors.
    /// Failed records remain in memory so callers can retry or report their contents.
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

    /// Advance game time and held movement only on the playing screen.
    /// Returns an error if a finished game cannot be converted into a stored record.
    pub fn advance(&mut self, duration: Duration) -> miette::Result<()> {
        if self.screen != Screen::Playing {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            self.held_input.advance(game, &self.config, duration);
        }
        self.finish_if_needed()
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

    /// Clear held inputs and require explicit resume after resize or focus loss.
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

    /// Route a terminal key to the current screen, including releases and repeat filtering.
    /// Returns an error if an action finishes a game whose result cannot be represented.
    pub fn key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if key.kind == KeyEventKind::Release {
            let released = normalized_code(key.code);
            self.pressed_actions.retain(|&code| code != released);
            self.held_input
                .release(self.game.as_mut(), &self.config, released);
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
            let code = normalized_code(key.code);
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
            KeyCode::Up => self.menu_selection = self.menu_selection.cycle(true),
            KeyCode::Down | KeyCode::Tab => self.menu_selection = self.menu_selection.cycle(false),
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
            if let Some(game) = &mut self.game {
                self.held_input.press(game, &self.config, key, action);
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
            let code = normalized_code(key.code);
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
        self.held_input.clear(self.game.as_mut());
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
                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                self.record_mode = self.record_mode.cycle(delta);
                if self.record_mode == Mode::Practice {
                    self.record_mode = self.record_mode.cycle(delta);
                }
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

#[cfg(test)]
mod tests;
