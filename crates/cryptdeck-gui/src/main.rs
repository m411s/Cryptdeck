// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod autolock;
mod store;
mod tray;

use std::ffi::OsStr;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use cryptdeck_core::{Backend, Error, MountRequest, MountedVolume, MAX_PIM};
use iced::event::{self, Event};
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{
    button, checkbox, column, container, operation, row, scrollable, space, text, text_input,
    Column,
};
use iced::{border, window, Alignment, Element, Length, Size, Subscription, Task, Theme};
use rust_i18n::t;
use secrecy::SecretString;
use zeroize::Zeroizing;

const VERACRYPT_DOWNLOAD: &str = "https://www.veracrypt.fr/en/Downloads.html";
const PASSWORD_ID: &str = "password";
/// Matches the desktop file and Flatpak ID, so desktops (especially on
/// Wayland) show the right icon and name for the window.
#[cfg(target_os = "linux")]
const APP_ID: &str = "io.github.m411s.Cryptdeck";
/// 64×64 RGBA, rendered from `assets/cryptdeck.svg`.
const ICON_RGBA: &[u8] = include_bytes!("../../../assets/icon-64.rgba");

fn main() -> iced::Result {
    select_language();
    // A daemon keeps running with no window open, so closing the window can
    // leave Cryptdeck in the system tray. Without a tray, closing quits.
    iced::daemon(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .run()
}

/// Use the system language if there's a translation for it, else English.
/// `CRYPTDECK_LANG` (e.g. `de` or `en`) overrides the system setting.
fn select_language() {
    let wanted = std::env::var("CRYPTDECK_LANG")
        .ok()
        .or_else(sys_locale::get_locale)
        .unwrap_or_default();
    let lang = wanted
        .split(['-', '_', '.'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let available = rust_i18n::available_locales!();
    let chosen = if available.iter().any(|l| *l == lang) {
        lang.as_str()
    } else {
        "en"
    };
    rust_i18n::set_locale(chosen);
}

fn window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(760.0, 620.0),
        min_size: Some(Size::new(560.0, 420.0)),
        icon: window::icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok(),
        // We decide what closing means: hide to the tray, or quit.
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: APP_ID.into(),
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

struct App {
    backend: Arc<dyn Backend>,
    backend_state: BackendState,
    /// Volumes the user has added, remembered between runs.
    saved: Vec<PathBuf>,
    mounted: Vec<MountedVolume>,
    /// The inline unlock form, if one is open.
    unlock: Option<UnlockForm>,
    busy: Option<Busy>,
    notice: Option<Notice>,
    drop_hover: bool,
    theme_override: Option<Theme>,
    /// The main window, while it's open.
    window: Option<window::Id>,
    /// The system tray, if the desktop has one.
    tray: Option<tray::Handle>,
    /// What the tray currently shows.
    tray_state: tray::State,
    settings: store::Settings,
    /// The same settings, shared with the auto-lock watcher threads.
    autolock: Arc<autolock::Settings>,
    autolock_status: AutoLockStatus,
    show_settings: bool,
}

enum AutoLockStatus {
    Starting,
    Ready { screen_lock: bool },
    Unavailable,
}

enum BackendState {
    Checking,
    Ready(String),
    Missing,
    Broken(String),
}

enum Busy {
    Unlocking(PathBuf),
    Locking(PathBuf),
    LockingAll,
}

enum Notice {
    Success(String),
    Error(String),
}

struct UnlockForm {
    volume: PathBuf,
    password: Password,
    show_password: bool,
    advanced: bool,
    pim: String,
    keyfiles: Vec<PathBuf>,
    read_only: bool,
    hidden: bool,
    mount_point: String,
    error: Option<String>,
}

impl UnlockForm {
    fn new(volume: PathBuf) -> Self {
        Self {
            volume,
            password: Password::default(),
            show_password: false,
            advanced: false,
            pim: String::new(),
            keyfiles: Vec::new(),
            read_only: false,
            hidden: false,
            mount_point: String::new(),
            error: None,
        }
    }

    /// Turn the form into a request. The password is copied into a
    /// `SecretString`, which is zeroized on drop.
    fn request(&self) -> Result<MountRequest, String> {
        let pim = match self.pim.trim() {
            "" => None,
            p => match p.parse::<u32>() {
                Ok(n) if n <= MAX_PIM => Some(n),
                _ => return Err(t!("pim_range", max = MAX_PIM).into()),
            },
        };
        let mount_point = match self.mount_point.trim() {
            "" => None,
            m => Some(PathBuf::from(m)),
        };
        let request = MountRequest {
            volume: self.volume.clone(),
            mount_point,
            password: SecretString::from(self.password.0.as_str()),
            pim,
            keyfiles: self.keyfiles.clone(),
            read_only: self.read_only,
            hidden: self.hidden,
        };
        request.validate().map_err(|e| e.to_string())?;
        Ok(request)
    }
}

/// The password as typed. Zeroized when replaced or dropped, and redacted
/// from `Debug` output, since iced's debug tooling formats messages with
/// `{:?}`. iced's text input keeps its own copy while editing, which we
/// can't wipe; see README.
#[derive(Clone, Default)]
struct Password(Zeroizing<String>);

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password([redacted])")
    }
}

#[derive(Debug, Clone)]
enum CheckError {
    Missing,
    Other(String),
}

#[derive(Debug, Clone)]
enum Message {
    Checked(Result<String, CheckError>),
    Recheck,
    Refresh,
    Refreshed(Result<Vec<MountedVolume>, String>),
    WindowFocused,
    FileHovered,
    FilesHoveredLeft,
    FileDropped(PathBuf),

    AddVolume,
    VolumePicked(Option<PathBuf>),
    Forget(PathBuf),

    StartUnlock(PathBuf),
    CancelUnlock,
    PasswordChanged(Password),
    ToggleShowPassword,
    ToggleAdvanced,
    PimChanged(String),
    ReadOnlyToggled(bool),
    HiddenToggled(bool),
    AddKeyfiles,
    KeyfilesPicked(Vec<PathBuf>),
    RemoveKeyfile(usize),
    MountPointChanged(String),
    BrowseMountPoint,
    MountPointPicked(Option<PathBuf>),
    Unlock,
    Unlocked(PathBuf, Result<(), String>),

    Lock(MountedVolume),
    LockAll,
    Locked(Result<(), String>),
    Open(PathBuf),
    GetVeraCrypt,
    DismissNotice,
    WindowOpened,
    CloseRequested(window::Id),
    Tray(tray::Update),
    AutoLock(autolock::Event),
    ToggleSettings,
    LockOnSleepToggled(bool),
    LockOnScreenLockToggled(bool),
}

/// One row in the volume list: a saved volume, a mounted one, or both.
struct Entry<'a> {
    path: &'a Path,
    saved: bool,
    mounted: Option<&'a MountedVolume>,
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let settings = store::load_settings();
        let mut app = App {
            backend: cryptdeck_core::default_backend(),
            backend_state: BackendState::Checking,
            saved: store::load(),
            mounted: Vec::new(),
            unlock: None,
            busy: None,
            notice: None,
            drop_hover: false,
            theme_override: match std::env::var("CRYPTDECK_THEME").as_deref() {
                Ok("dark") => Some(Theme::Dark),
                Ok("light") => Some(Theme::Light),
                _ => None,
            },
            window: None,
            tray: None,
            tray_state: tray::State::default(),
            settings,
            autolock: Arc::new(autolock::Settings::new(
                settings.lock_on_sleep,
                settings.lock_on_screen_lock,
            )),
            autolock_status: AutoLockStatus::Starting,
            show_settings: false,
        };
        let (autolock_tx, autolock_rx) = iced::futures::channel::mpsc::unbounded();
        autolock::start(app.backend.clone(), app.autolock.clone(), autolock_tx);
        let (id, open) = window::open(window_settings());
        app.window = Some(id);
        let tasks = Task::batch([
            open.map(|_| Message::WindowOpened),
            app.check(),
            Task::done(Message::Refresh),
            Task::run(autolock_rx, Message::AutoLock),
        ]);
        (app, tasks)
    }

    fn title(&self, _window: window::Id) -> String {
        "Cryptdeck".into()
    }

    /// Follow the system's light or dark mode, unless `CRYPTDECK_THEME` is
    /// set to `light` or `dark`.
    fn theme(&self, _window: window::Id) -> Option<Theme> {
        self.theme_override.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window_events(),
            window::close_requests().map(Message::CloseRequested),
            tray::subscription().map(Message::Tray),
        ])
    }

    fn check(&self) -> Task<Message> {
        let b = self.backend.clone();
        Task::perform(
            run_blocking(move || {
                b.check().map_err(|e| match e {
                    Error::NotInstalled(_) => CheckError::Missing,
                    e => CheckError::Other(e.to_string()),
                })
            }),
            |r| Message::Checked(r.unwrap_or_else(|e| Err(CheckError::Other(e)))),
        )
    }

    fn refresh(&self) -> Task<Message> {
        let b = self.backend.clone();
        Task::perform(
            background(move || {
                // Canonical paths so they match the saved list.
                Ok(b.list()?
                    .into_iter()
                    .map(|mut v| {
                        v.volume = v.volume.canonicalize().unwrap_or(v.volume);
                        v
                    })
                    .collect())
            }),
            Message::Refreshed,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.sync_tray();
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Checked(result) => {
                self.backend_state = match result {
                    Ok(version) => BackendState::Ready(version),
                    Err(CheckError::Missing) => BackendState::Missing,
                    Err(CheckError::Other(e)) => BackendState::Broken(e),
                };
                Task::none()
            }
            Message::Recheck => {
                self.backend_state = BackendState::Checking;
                Task::batch([self.check(), self.refresh()])
            }
            Message::Refresh | Message::WindowFocused => self.refresh(),
            Message::Refreshed(Ok(volumes)) => {
                self.mounted = volumes;
                Task::none()
            }
            Message::Refreshed(Err(e)) => {
                // When the backend itself is unavailable, the banner explains.
                if matches!(self.backend_state, BackendState::Ready(_)) {
                    self.notice = Some(Notice::Error(t!("list_failed", error = e).into()));
                }
                Task::none()
            }
            Message::FileHovered => {
                self.drop_hover = true;
                Task::none()
            }
            Message::FilesHoveredLeft => {
                self.drop_hover = false;
                Task::none()
            }
            Message::FileDropped(path) => {
                self.drop_hover = false;
                self.add_volume(path)
            }

            Message::AddVolume => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title(t!("dialog.choose_volume"))
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::VolumePicked,
            ),
            Message::VolumePicked(Some(path)) => self.add_volume(path),
            Message::VolumePicked(None) => Task::none(),
            Message::Forget(path) => {
                self.saved.retain(|p| p != &path);
                if self.unlock.as_ref().is_some_and(|f| f.volume == path) {
                    self.unlock = None;
                }
                self.persist();
                Task::none()
            }

            Message::StartUnlock(path) => self.open_unlock_form(path),
            Message::CancelUnlock => {
                if !matches!(self.busy, Some(Busy::Unlocking(_))) {
                    self.unlock = None;
                }
                Task::none()
            }
            Message::PasswordChanged(p) => {
                if let Some(f) = &mut self.unlock {
                    f.password = p;
                    f.error = None;
                }
                Task::none()
            }
            Message::ToggleShowPassword => {
                if let Some(f) = &mut self.unlock {
                    f.show_password = !f.show_password;
                }
                operation::focus(PASSWORD_ID)
            }
            Message::ToggleAdvanced => {
                if let Some(f) = &mut self.unlock {
                    f.advanced = !f.advanced;
                }
                Task::none()
            }
            Message::PimChanged(p) => {
                if let Some(f) = &mut self.unlock {
                    if p.chars().all(|c| c.is_ascii_digit()) && p.len() <= 7 {
                        f.pim = p;
                    }
                }
                Task::none()
            }
            Message::ReadOnlyToggled(ro) => {
                if let Some(f) = &mut self.unlock {
                    f.read_only = ro;
                }
                Task::none()
            }
            Message::HiddenToggled(hidden) => {
                if let Some(f) = &mut self.unlock {
                    f.hidden = hidden;
                }
                Task::none()
            }
            Message::AddKeyfiles => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title(t!("dialog.add_keyfiles"))
                        .pick_files()
                        .await
                        .map(|fs| fs.iter().map(|f| f.path().to_path_buf()).collect())
                        .unwrap_or_default()
                },
                Message::KeyfilesPicked,
            ),
            Message::KeyfilesPicked(files) => {
                if let Some(f) = &mut self.unlock {
                    for file in files {
                        if !f.keyfiles.contains(&file) {
                            f.keyfiles.push(file);
                        }
                    }
                }
                Task::none()
            }
            Message::RemoveKeyfile(i) => {
                if let Some(f) = &mut self.unlock {
                    if i < f.keyfiles.len() {
                        f.keyfiles.remove(i);
                    }
                }
                Task::none()
            }
            Message::MountPointChanged(m) => {
                if let Some(f) = &mut self.unlock {
                    f.mount_point = m;
                }
                Task::none()
            }
            Message::BrowseMountPoint => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title(t!("dialog.choose_folder"))
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::MountPointPicked,
            ),
            Message::MountPointPicked(path) => {
                if let (Some(f), Some(p)) = (&mut self.unlock, path) {
                    f.mount_point = p.display().to_string();
                }
                Task::none()
            }
            Message::Unlock => {
                if self.busy.is_some() {
                    return Task::none();
                }
                let Some(form) = &mut self.unlock else {
                    return Task::none();
                };
                if !self.backend.supports_mount_point() {
                    form.mount_point.clear();
                }
                let request = match form.request() {
                    Ok(r) => r,
                    Err(e) => {
                        form.error = Some(e);
                        return Task::none();
                    }
                };
                form.error = None;
                let path = form.volume.clone();
                self.busy = Some(Busy::Unlocking(path.clone()));
                self.notice = None;
                let b = self.backend.clone();
                Task::perform(background(move || b.mount(&request)), move |r| {
                    Message::Unlocked(path.clone(), r)
                })
            }
            Message::Unlocked(path, result) => {
                self.busy = None;
                let refresh = self.refresh();
                match result {
                    Ok(()) => {
                        // Dropping the form zeroizes the password.
                        if self.unlock.as_ref().is_some_and(|f| f.volume == path) {
                            self.unlock = None;
                        }
                        self.notice = Some(Notice::Success(
                            t!("notice.unlocked", name = display_name(&path)).into(),
                        ));
                        refresh
                    }
                    Err(e) => match self.unlock.as_mut().filter(|f| f.volume == path) {
                        Some(form) => {
                            form.error = Some(e);
                            Task::batch([
                                refresh,
                                operation::focus(PASSWORD_ID),
                                operation::select_all(PASSWORD_ID),
                            ])
                        }
                        None => {
                            self.notice = Some(Notice::Error(e));
                            refresh
                        }
                    },
                }
            }

            Message::Lock(volume) => {
                if self.busy.is_some() {
                    return Task::none();
                }
                self.busy = Some(Busy::Locking(volume.volume.clone()));
                self.notice = None;
                let name = display_name(&volume.volume);
                let b = self.backend.clone();
                Task::perform(background(move || b.dismount(&volume)), move |r| {
                    Message::Locked(
                        r.map_err(|e| t!("notice.lock_failed", name = name, error = e).into()),
                    )
                })
            }
            Message::LockAll => {
                if self.busy.is_some() {
                    return Task::none();
                }
                self.busy = Some(Busy::LockingAll);
                self.notice = None;
                let b = self.backend.clone();
                Task::perform(background(move || b.dismount_all()), Message::Locked)
            }
            Message::Locked(result) => {
                let what = match self.busy.take() {
                    Some(Busy::Locking(path)) => {
                        t!("notice.locked", name = display_name(&path)).into()
                    }
                    _ => t!("notice.all_locked").into(),
                };
                self.notice = Some(match result {
                    Ok(()) => Notice::Success(what),
                    Err(e) => Notice::Error(e),
                });
                self.refresh()
            }
            Message::Open(path) => {
                if let Err(e) = open_external(path.as_os_str()) {
                    self.notice = Some(Notice::Error(
                        t!("notice.open_failed", path = path.display(), error = e).into(),
                    ));
                }
                Task::none()
            }
            Message::GetVeraCrypt => {
                if let Err(e) = open_external(OsStr::new(VERACRYPT_DOWNLOAD)) {
                    self.notice = Some(Notice::Error(
                        t!("notice.browser_failed", error = e, url = VERACRYPT_DOWNLOAD).into(),
                    ));
                }
                Task::none()
            }
            Message::DismissNotice => {
                self.notice = None;
                Task::none()
            }
            Message::WindowOpened => Task::none(),
            Message::CloseRequested(id) => {
                if self.tray.is_none() {
                    return iced::exit();
                }
                // Keep running in the tray. Drop any half-typed password.
                if !matches!(self.busy, Some(Busy::Unlocking(_))) {
                    self.unlock = None;
                }
                self.drop_hover = false;
                self.window = None;
                window::close(id)
            }
            Message::Tray(update) => self.handle_tray(update),
            Message::AutoLock(event) => match event {
                autolock::Event::Ready { screen_lock } => {
                    self.autolock_status = AutoLockStatus::Ready { screen_lock };
                    Task::none()
                }
                autolock::Event::Unavailable(reason) => {
                    eprintln!("cryptdeck: automatic locking unavailable: {reason}");
                    self.autolock_status = AutoLockStatus::Unavailable;
                    Task::none()
                }
                autolock::Event::Locked {
                    reason,
                    count,
                    error,
                } => {
                    self.unlock = None;
                    self.notice = Some(match error {
                        None => Notice::Success(autolock::describe(reason, count)),
                        Some(e) => Notice::Error(t!("notice.autolock_failed", error = e).into()),
                    });
                    self.refresh()
                }
            },
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
                Task::none()
            }
            Message::LockOnSleepToggled(on) => {
                self.settings.lock_on_sleep = on;
                self.autolock.on_sleep.store(on, Ordering::Relaxed);
                self.persist_settings();
                Task::none()
            }
            Message::LockOnScreenLockToggled(on) => {
                self.settings.lock_on_screen_lock = on;
                self.autolock.on_screen_lock.store(on, Ordering::Relaxed);
                self.persist_settings();
                Task::none()
            }
        }
    }

    fn handle_tray(&mut self, update: tray::Update) -> Task<Message> {
        match update {
            tray::Update::Ready(handle) => {
                handle.set_state(self.tray_snapshot());
                self.tray_state = self.tray_snapshot();
                self.tray = Some(handle);
                Task::none()
            }
            tray::Update::Unavailable(reason) => {
                eprintln!("cryptdeck: no system tray ({reason}); closing the window quits");
                Task::none()
            }
            tray::Update::Event(event) => match event {
                tray::Event::Show => self.show_window(),
                tray::Event::Unlock(path) => {
                    let show = self.show_window();
                    let focus = self.open_unlock_form(path);
                    show.chain(focus)
                }
                tray::Event::Open(path) => match self.mounted_volume(&path) {
                    Some(m) => match m.mount_point.clone() {
                        Some(mp) => self.handle(Message::Open(mp)),
                        None => Task::none(),
                    },
                    None => Task::none(),
                },
                tray::Event::Lock(path) => match self.mounted_volume(&path).cloned() {
                    Some(m) => self.handle(Message::Lock(m)),
                    None => Task::none(),
                },
                tray::Event::LockAll => self.handle(Message::LockAll),
                tray::Event::Quit => iced::exit(),
            },
        }
    }

    /// Bring the window to the front, opening it again if it was closed.
    fn show_window(&mut self) -> Task<Message> {
        match self.window {
            Some(id) => Task::batch([window::minimize(id, false), window::gain_focus(id)]),
            None => {
                let (id, open) = window::open(window_settings());
                self.window = Some(id);
                open.map(|_| Message::WindowOpened)
            }
        }
    }

    fn mounted_volume(&self, path: &Path) -> Option<&MountedVolume> {
        self.mounted.iter().find(|m| m.volume == path)
    }

    fn tray_snapshot(&self) -> tray::State {
        tray::State {
            volumes: self
                .entries()
                .iter()
                .map(|e| tray::Volume {
                    name: display_name(e.path),
                    path: e.path.to_path_buf(),
                    unlocked: e.mounted.is_some(),
                    can_open: e.mounted.is_some_and(|m| m.mount_point.is_some()),
                })
                .collect(),
            busy: self.busy.is_some(),
        }
    }

    /// Push the current volumes to the tray, but only when something it
    /// shows has changed (not on every keystroke).
    fn sync_tray(&mut self) {
        let Some(handle) = &self.tray else { return };
        let state = self.tray_snapshot();
        if state != self.tray_state {
            handle.set_state(state.clone());
            self.tray_state = state;
        }
    }

    /// Remember a volume and open its unlock form.
    fn add_volume(&mut self, path: PathBuf) -> Task<Message> {
        if path.is_dir() {
            self.notice = Some(Notice::Error(
                t!("notice.is_folder", name = display_name(&path)).into(),
            ));
            return Task::none();
        }
        let path = path.canonicalize().unwrap_or(path);
        if !self.saved.contains(&path) {
            self.saved.push(path.clone());
            self.persist();
        }
        if self.mounted.iter().any(|m| m.volume == path) {
            return Task::none();
        }
        self.open_unlock_form(path)
    }

    fn open_unlock_form(&mut self, path: PathBuf) -> Task<Message> {
        if self.busy.is_some() {
            return Task::none();
        }
        self.notice = None;
        self.unlock = Some(UnlockForm::new(path));
        operation::focus(PASSWORD_ID)
    }

    fn persist_settings(&mut self) {
        if let Err(e) = store::save_settings(&self.settings) {
            self.notice = Some(Notice::Error(
                t!("notice.settings_save_failed", error = e).into(),
            ));
        }
    }

    fn persist(&mut self) {
        if let Err(e) = store::save(&self.saved) {
            self.notice = Some(Notice::Error(
                t!("notice.list_save_failed", error = e).into(),
            ));
        }
    }

    /// Saved volumes first, in the order they were added, then anything
    /// mounted outside Cryptdeck.
    fn entries(&self) -> Vec<Entry<'_>> {
        let find = |p: &Path| self.mounted.iter().find(|m| m.volume == p);
        let mut entries: Vec<Entry> = self
            .saved
            .iter()
            .map(|p| Entry {
                path: p,
                saved: true,
                mounted: find(p),
            })
            .collect();
        for m in &self.mounted {
            if !self.saved.contains(&m.volume) {
                entries.push(Entry {
                    path: &m.volume,
                    saved: false,
                    mounted: Some(m),
                });
            }
        }
        entries
    }

    // ----- View -----

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        if self.drop_hover {
            return container(
                column![
                    text(t!("drop.title")).size(22),
                    text(t!("drop.body")).style(text::secondary),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .center(Length::Fill)
            .padding(24)
            .style(|theme: &Theme| {
                let p = theme.extended_palette();
                container::Style {
                    background: Some(p.primary.weak.color.scale_alpha(0.25).into()),
                    border: border::rounded(16).color(p.primary.base.color).width(3),
                    ..container::Style::default()
                }
            })
            .into();
        }

        let mut page = Column::new().spacing(16).push(self.header());
        if self.show_settings {
            page = page.push(self.settings_view());
        }
        if let Some(banner) = self.backend_banner() {
            page = page.push(banner);
        }
        if let Some(notice) = self.notice_view() {
            page = page.push(notice);
        }

        let entries = self.entries();
        if entries.is_empty() {
            page = page.push(self.empty_state());
        } else {
            let mut list = Column::new().spacing(8);
            for entry in &entries {
                list = list.push(self.volume_card(entry));
            }
            page = page
                .push(text(t!("list.title")).size(18))
                .push(list)
                .push(self.footer());
        }

        scrollable(
            container(page.padding(24).max_width(820))
                .center_x(Length::Fill)
                .padding(iced::Padding::ZERO),
        )
        .height(Length::Fill)
        .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let status = match &self.backend_state {
            BackendState::Checking => t!("header.checking").into(),
            BackendState::Ready(version) => version.clone(),
            BackendState::Missing | BackendState::Broken(_) => String::new(),
        };
        row![
            text("Cryptdeck").size(28),
            space::horizontal(),
            text(status).size(12).style(text::secondary),
            button(
                text(if self.show_settings {
                    t!("header.close_settings")
                } else {
                    t!("header.settings")
                })
                .size(13)
            )
            .style(button::text)
            .on_press(Message::ToggleSettings),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let (available, screen_lock, status) = match &self.autolock_status {
            AutoLockStatus::Starting => (true, true, None),
            AutoLockStatus::Ready { screen_lock: true } => (true, true, None),
            AutoLockStatus::Ready { screen_lock: false } => {
                (true, false, Some(t!("settings.no_screen_lock").to_string()))
            }
            AutoLockStatus::Unavailable => {
                (false, false, Some(t!("settings.unavailable").to_string()))
            }
        };
        let mut col = column![
            text(t!("settings.autolock")).size(16),
            checkbox(self.settings.lock_on_sleep)
                .label(t!("settings.on_sleep"))
                .on_toggle_maybe(available.then_some(Message::LockOnSleepToggled)),
            checkbox(self.settings.lock_on_screen_lock)
                .label(t!("settings.on_screen_lock"))
                .on_toggle_maybe(
                    (available && screen_lock).then_some(Message::LockOnScreenLockToggled)
                ),
            text(t!("settings.open_files"))
                .size(12)
                .style(text::secondary),
        ]
        .spacing(10);
        if let Some(status) = status {
            col = col.push(text(status).size(12).style(text::secondary));
        }
        container(col)
            .padding(16)
            .width(Length::Fill)
            .style(card_style)
            .into()
    }

    fn backend_banner(&self) -> Option<Element<'_, Message>> {
        let (title, body) = match &self.backend_state {
            BackendState::Missing => (
                t!("banner.missing.title"),
                t!("banner.missing.body").to_string(),
            ),
            BackendState::Broken(e) => (t!("banner.broken.title"), e.clone()),
            _ => return None,
        };
        Some(
            container(
                column![
                    text(title).size(16),
                    text(body).size(14),
                    row![
                        button(text(t!("banner.get"))).on_press(Message::GetVeraCrypt),
                        button(text(t!("banner.check_again")))
                            .style(button::secondary)
                            .on_press(Message::Recheck),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            )
            .padding(16)
            .width(Length::Fill)
            .style(|theme: &Theme| tinted(theme.extended_palette().warning.weak))
            .into(),
        )
    }

    fn notice_view(&self) -> Option<Element<'_, Message>> {
        let (message, success) = match self.notice.as_ref()? {
            Notice::Success(m) => (m, true),
            Notice::Error(m) => (m, false),
        };
        Some(
            container(
                row![
                    text(message.as_str()).width(Length::Fill),
                    button(text(t!("dismiss")).size(13))
                        .style(button::text)
                        .on_press(Message::DismissNotice),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .width(Length::Fill)
            .style(move |theme: &Theme| {
                let p = theme.extended_palette();
                tinted(if success {
                    p.success.weak
                } else {
                    p.danger.weak
                })
            })
            .into(),
        )
    }

    fn empty_state(&self) -> Element<'_, Message> {
        container(
            column![
                text(t!("empty.title")).size(22),
                text(t!("empty.body"))
                    .style(text::secondary)
                    .align_x(Alignment::Center),
                button(text(t!("add_volume")).size(16))
                    .padding([10, 24])
                    .on_press(Message::AddVolume),
            ]
            .spacing(14)
            .align_x(Alignment::Center),
        )
        .padding(40)
        .width(Length::Fill)
        .style(card_style)
        .into()
    }

    fn footer(&self) -> Element<'_, Message> {
        let any_mounted = !self.mounted.is_empty();
        column![
            row![
                button(text(t!("add_volume")))
                    .style(button::secondary)
                    .on_press(Message::AddVolume),
                text(t!("footer.drag")).size(13).style(text::secondary),
                space::horizontal(),
                if any_mounted {
                    Element::from(
                        button(text(t!("lock_all")))
                            .style(button::secondary)
                            .on_press_maybe(self.busy.is_none().then_some(Message::LockAll)),
                    )
                } else {
                    space::horizontal().width(0).into()
                },
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text(if self.tray.is_some() {
                t!("footer.tray")
            } else {
                t!("footer.stay_unlocked")
            })
            .size(12)
            .style(text::secondary),
        ]
        .spacing(10)
        .into()
    }

    fn volume_card<'a>(&'a self, entry: &Entry<'a>) -> Element<'a, Message> {
        let unlocked = entry.mounted.is_some();
        let subtitle = match entry.mounted.and_then(|m| m.mount_point.as_ref()) {
            Some(mp) => t!("card.unlocked_at", path = mp.display()).into(),
            None if unlocked => t!("status.unlocked").into(),
            None => entry.path.parent().map(pretty_dir).unwrap_or_default(),
        };

        let busy_label = match &self.busy {
            Some(Busy::Unlocking(p)) if p == entry.path => Some(t!("busy.unlocking")),
            Some(Busy::Locking(p)) if p == entry.path => Some(t!("busy.locking")),
            Some(Busy::LockingAll) if unlocked => Some(t!("busy.locking")),
            _ => None,
        };
        let form_open = self.unlock.as_ref().filter(|f| f.volume == entry.path);
        let idle = self.busy.is_none();
        let can_unlock = idle && !matches!(self.backend_state, BackendState::Missing);

        let actions: Element<_> = if let Some(label) = busy_label {
            text(label).style(text::secondary).into()
        } else if let Some(m) = entry.mounted {
            row![
                button(text(t!("open"))).on_press_maybe(m.mount_point.clone().map(Message::Open)),
                button(text(t!("lock")))
                    .style(button::secondary)
                    .on_press_maybe(idle.then(|| Message::Lock(m.clone()))),
            ]
            .spacing(8)
            .into()
        } else if form_open.is_some() {
            space::horizontal().width(0).into()
        } else {
            let mut r = row![button(text(t!("unlock"))).on_press_maybe(
                can_unlock.then(|| Message::StartUnlock(entry.path.to_path_buf()))
            )]
            .spacing(4)
            .align_y(Alignment::Center);
            if entry.saved {
                r = r.push(
                    button(text(t!("forget")).size(13))
                        .style(button::text)
                        .on_press_maybe(idle.then(|| Message::Forget(entry.path.to_path_buf()))),
                );
            }
            r.into()
        };

        let summary = row![
            column![
                text(display_name(entry.path)).size(16),
                text(subtitle).size(13).style(text::secondary),
            ]
            .spacing(2)
            .width(Length::Fill),
            status_pill(unlocked),
            actions,
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let mut card = Column::new().spacing(14).push(summary);
        if let Some(form) = form_open {
            card = card.push(self.unlock_form(form));
        }
        container(card)
            .padding(14)
            .width(Length::Fill)
            .style(card_style)
            .into()
    }

    fn unlock_form<'a>(&'a self, f: &'a UnlockForm) -> Element<'a, Message> {
        let busy = self.busy.is_some();

        let password = row![
            text_input(&t!("password"), f.password.0.as_str())
                .id(PASSWORD_ID)
                .secure(!f.show_password)
                .on_input_maybe(
                    (!busy).then_some(|p| Message::PasswordChanged(Password(Zeroizing::new(p))))
                )
                .on_submit(Message::Unlock)
                .padding(10),
            button(text(if f.show_password {
                t!("hide")
            } else {
                t!("show")
            }))
            .style(button::secondary)
            .padding(10)
            .on_press(Message::ToggleShowPassword),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let unlock_label = if busy {
            t!("busy.unlocking")
        } else {
            t!("unlock")
        };
        let buttons = row![
            button(text(unlock_label))
                .padding([8, 20])
                .on_press_maybe((!busy).then_some(Message::Unlock)),
            button(text(t!("cancel")))
                .style(button::text)
                .on_press_maybe((!busy).then_some(Message::CancelUnlock)),
            space::horizontal(),
            button(
                text(if f.advanced {
                    t!("advanced.hide")
                } else {
                    t!("advanced.show")
                })
                .size(13)
            )
            .style(button::text)
            .on_press(Message::ToggleAdvanced),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut form = Column::new().spacing(10).push(password);
        if let Some(error) = &f.error {
            form = form.push(text(error.as_str()).size(14).style(text::danger));
        }
        form = form.push(buttons);
        if f.advanced {
            form = form.push(self.advanced_options(f, busy));
        }
        form.into()
    }

    fn advanced_options<'a>(&'a self, f: &'a UnlockForm, busy: bool) -> Element<'a, Message> {
        let label = |s: String| text(s).width(180);
        let hint = |s: String| text(s).size(12).style(text::secondary);

        let pim = row![
            label("PIM".into()),
            column![
                text_input(&t!("pim.placeholder"), &f.pim)
                    .secure(true)
                    .on_input_maybe((!busy).then_some(Message::PimChanged))
                    .on_submit(Message::Unlock)
                    .width(320),
                hint(t!("pim.hint").into()),
            ]
            .spacing(4),
        ]
        .spacing(8);

        let mut keyfile_list = Column::new().spacing(4);
        for (i, kf) in f.keyfiles.iter().enumerate() {
            keyfile_list = keyfile_list.push(
                row![
                    text(kf.display().to_string()).size(13).width(Length::Fill),
                    button(text(t!("remove")).size(12))
                        .style(button::text)
                        .on_press(Message::RemoveKeyfile(i)),
                ]
                .align_y(Alignment::Center),
            );
        }
        let keyfiles = row![
            label(t!("keyfiles").into()),
            column![
                keyfile_list,
                button(text(t!("add_keyfiles")))
                    .style(button::secondary)
                    .on_press(Message::AddKeyfiles),
            ]
            .spacing(6)
            .width(Length::Fill),
        ]
        .spacing(8);

        let mount_point: Element<_> = if !self.backend.supports_mount_point() {
            row![
                label(t!("folder.label").into()),
                hint(if cfg!(windows) {
                    t!("folder.windows").into()
                } else {
                    t!("folder.system").into()
                })
                .width(Length::Fill),
            ]
            .spacing(8)
            .into()
        } else {
            row![
                label(t!("folder.label").into()),
                column![
                    row![
                        text_input(&t!("folder.placeholder"), &f.mount_point)
                            .on_input_maybe((!busy).then_some(Message::MountPointChanged))
                            .on_submit(Message::Unlock),
                        button(text(t!("choose")))
                            .style(button::secondary)
                            .on_press(Message::BrowseMountPoint),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    hint(t!("folder.hint").into()),
                ]
                .spacing(4)
                .width(Length::Fill),
            ]
            .spacing(8)
            .into()
        };

        let hidden = row![
            label(String::new()),
            column![
                checkbox(f.hidden)
                    .label(t!("hidden.label"))
                    .on_toggle(Message::HiddenToggled),
                hint(t!("hidden.hint").into()),
            ]
            .spacing(4),
        ]
        .spacing(8);

        let read_only = row![
            label(String::new()),
            checkbox(f.read_only)
                .label(t!("read_only"))
                .on_toggle(Message::ReadOnlyToggled),
        ]
        .spacing(8);

        container(column![pim, keyfiles, mount_point, hidden, read_only].spacing(14))
            .padding(iced::Padding::new(0.0).top(6))
            .into()
    }
}

/// Window and keyboard events the app reacts to.
fn window_events() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        // Pick up volumes mounted or dismounted elsewhere.
        Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
        Event::Window(window::Event::FileHovered(_)) => Some(Message::FileHovered),
        Event::Window(window::Event::FilesHoveredLeft) => Some(Message::FilesHoveredLeft),
        Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) => Some(Message::CancelUnlock),
        _ => None,
    })
}

fn status_pill<'a>(unlocked: bool) -> Element<'a, Message> {
    container(
        text(if unlocked {
            t!("status.unlocked")
        } else {
            t!("status.locked")
        })
        .size(12),
    )
    .padding([2, 10])
    .style(move |theme: &Theme| {
        let p = theme.extended_palette();
        let pair = if unlocked {
            p.success.base
        } else {
            p.background.strong
        };
        container::Style {
            background: Some(pair.color.into()),
            text_color: Some(pair.text),
            border: border::rounded(10),
            ..container::Style::default()
        }
    })
    .into()
}

/// A card: the window background with a subtle border and rounded corners.
fn card_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.base.color.into()),
        text_color: Some(p.background.base.text),
        border: border::rounded(12)
            .color(p.background.strong.color)
            .width(1),
        ..container::Style::default()
    }
}

/// A rounded box tinted with a palette colour, for banners and notices.
fn tinted(pair: iced::theme::palette::Pair) -> container::Style {
    container::Style {
        background: Some(pair.color.into()),
        text_color: Some(pair.text),
        border: border::rounded(8),
        ..container::Style::default()
    }
}

/// A short, human-friendly name for a volume: the file name, or the full
/// path for devices like `/dev/sdb1`.
fn display_name(path: &Path) -> String {
    if path.starts_with("/dev") {
        return path.display().to_string();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// A folder path for display, with the home folder shown as `~`.
fn pretty_dir(dir: &Path) -> String {
    match dirs::home_dir().and_then(|home| dir.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => dir.display().to_string(),
    }
}

/// Run blocking backend work on a worker thread so the UI stays responsive.
/// Errors become display strings; a panic in the worker becomes an error
/// instead of taking the window down with it.
fn background<T: Send + 'static>(
    f: impl FnOnce() -> cryptdeck_core::Result<T> + Send + 'static,
) -> impl Future<Output = Result<T, String>> {
    let fut = run_blocking(move || f().map_err(|e| e.to_string()));
    async move { fut.await.and_then(|r| r) }
}

/// Run `f` on a worker thread. `Err` means the worker panicked.
fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = Result<T, String>> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.map_err(|_| t!("internal_error").to_string()) }
}

/// Open a folder in the file manager, or a URL in the browser.
fn open_external(target: &OsStr) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";

    let mut child = std::process::Command::new(program).arg(target).spawn()?;
    // Reap the child when it exits so it doesn't linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_are_short_but_devices_stay_whole() {
        assert_eq!(
            display_name(Path::new("/home/me/My Vault.hc")),
            "My Vault.hc"
        );
        assert_eq!(display_name(Path::new("/dev/sdb1")), "/dev/sdb1");
    }

    #[test]
    fn password_debug_is_redacted() {
        let p = Password(Zeroizing::new("hunter2".into()));
        let shown = format!("{:?}", Message::PasswordChanged(p));
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}

#[cfg(test)]
mod translation_tests {
    /// Every `t!("key")` in the source must exist in English and German;
    /// a missing key would show up in the UI as the raw key.
    #[test]
    fn every_key_is_translated() {
        let sources = [
            include_str!("main.rs"),
            include_str!("tray.rs"),
            include_str!("autolock.rs"),
        ];
        let mut keys: Vec<&str> = Vec::new();
        for src in sources {
            // Skip this test module, whose comments mention `t!`.
            let src = src.split("mod translation_tests").next().unwrap_or(src);
            for (i, _) in src.match_indices("t!(\"") {
                // A standalone `t!(`, not the end of e.g. `format!(`.
                let before = src[..i].chars().next_back().unwrap_or(' ');
                if before.is_alphanumeric() || before == '_' {
                    continue;
                }
                let rest = &src[i + 4..];
                keys.extend(rest.split('"').next());
            }
        }
        keys.sort();
        keys.dedup();
        assert!(keys.len() > 60, "found only {} keys", keys.len());
        for locale in ["en", "de"] {
            for key in &keys {
                let text = rust_i18n::t!(*key, locale = locale);
                assert_ne!(&*text, *key, "{key} has no {locale} translation");
            }
        }
    }
}
