// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use cryptdeck_core::{Backend, MountRequest, MountedVolume, MAX_PIM};
use iced::event::{self, Event};
use iced::widget::{
    button, checkbox, column, container, row, rule, scrollable, text, text_input, Column,
};
use iced::{window, Alignment, Element, Length, Subscription, Task, Theme};
use secrecy::SecretString;
use zeroize::Zeroizing;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Cryptdeck")
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size((860.0, 680.0))
        .run()
}

struct App {
    backend: Arc<dyn Backend>,
    /// Result of the startup check; `None` until it finishes.
    backend_info: Option<Result<String, String>>,
    volumes: Vec<MountedVolume>,
    form: MountForm,
    busy: bool,
    status: Option<Status>,
}

#[derive(Default)]
struct MountForm {
    volume: String,
    mount_point: String,
    password: Password,
    pim: String,
    keyfiles: Vec<PathBuf>,
    read_only: bool,
}

enum Status {
    Progress(String),
    Success(String),
    Error(String),
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
enum Message {
    Checked(Result<String, String>),
    Refresh,
    Refreshed(Result<Vec<MountedVolume>, String>),

    VolumeChanged(String),
    BrowseVolume,
    VolumePicked(Option<PathBuf>),
    MountPointChanged(String),
    BrowseMountPoint,
    MountPointPicked(Option<PathBuf>),
    PasswordChanged(Password),
    PimChanged(String),
    AddKeyfiles,
    KeyfilesPicked(Vec<PathBuf>),
    RemoveKeyfile(usize),
    ReadOnlyToggled(bool),

    Mount,
    Mounted(Result<(), String>),
    Dismount(MountedVolume),
    DismountAll,
    Dismounted(Result<(), String>),
    Open(PathBuf),
    WindowFocused,
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let backend = cryptdeck_core::default_backend();
        let app = App {
            backend: backend.clone(),
            backend_info: None,
            volumes: Vec::new(),
            form: MountForm::default(),
            busy: false,
            status: None,
        };
        let check = Task::perform(background(move || backend.check()), Message::Checked);
        (app, Task::batch([check, Task::done(Message::Refresh)]))
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    /// Refresh the list when the window regains focus, so volumes mounted or
    /// dismounted elsewhere show up.
    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, _window| match event {
            Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
            _ => None,
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Checked(info) => {
                self.backend_info = Some(info);
                Task::none()
            }
            Message::Refresh | Message::WindowFocused => {
                let b = self.backend.clone();
                Task::perform(background(move || b.list()), Message::Refreshed)
            }
            Message::Refreshed(Ok(volumes)) => {
                self.volumes = volumes;
                Task::none()
            }
            Message::Refreshed(Err(e)) => {
                // If the backend itself is unavailable, the header already
                // says so; don't repeat it.
                if !matches!(self.backend_info, Some(Err(_))) {
                    self.status = Some(Status::Error(format!("Could not list volumes: {e}")));
                }
                Task::none()
            }

            Message::VolumeChanged(v) => {
                self.form.volume = v;
                Task::none()
            }
            Message::BrowseVolume => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose a VeraCrypt volume")
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::VolumePicked,
            ),
            Message::VolumePicked(path) => {
                if let Some(p) = path {
                    self.form.volume = p.display().to_string();
                }
                Task::none()
            }
            Message::MountPointChanged(v) => {
                self.form.mount_point = v;
                Task::none()
            }
            Message::BrowseMountPoint => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose a mount point")
                        .pick_folder()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                Message::MountPointPicked,
            ),
            Message::MountPointPicked(path) => {
                if let Some(p) = path {
                    self.form.mount_point = p.display().to_string();
                }
                Task::none()
            }
            Message::PasswordChanged(p) => {
                self.form.password = p;
                Task::none()
            }
            Message::PimChanged(p) => {
                if p.chars().all(|c| c.is_ascii_digit()) && p.len() <= 7 {
                    self.form.pim = p;
                }
                Task::none()
            }
            Message::AddKeyfiles => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Add keyfiles")
                        .pick_files()
                        .await
                        .map(|fs| fs.iter().map(|f| f.path().to_path_buf()).collect())
                        .unwrap_or_default()
                },
                Message::KeyfilesPicked,
            ),
            Message::KeyfilesPicked(files) => {
                for f in files {
                    if !self.form.keyfiles.contains(&f) {
                        self.form.keyfiles.push(f);
                    }
                }
                Task::none()
            }
            Message::RemoveKeyfile(i) => {
                if i < self.form.keyfiles.len() {
                    self.form.keyfiles.remove(i);
                }
                Task::none()
            }
            Message::ReadOnlyToggled(ro) => {
                self.form.read_only = ro;
                Task::none()
            }

            Message::Mount => {
                if self.busy {
                    return Task::none();
                }
                let request = match self.build_request() {
                    Ok(r) => r,
                    Err(e) => {
                        self.status = Some(Status::Error(e));
                        return Task::none();
                    }
                };
                self.busy = true;
                self.status = Some(Status::Progress("Mounting…".into()));
                let b = self.backend.clone();
                Task::perform(background(move || b.mount(&request)), Message::Mounted)
            }
            Message::Mounted(result) => {
                self.busy = false;
                match result {
                    Ok(()) => {
                        // Clear secrets from the form once they've been used.
                        self.form.password = Password::default();
                        self.form.pim.clear();
                        self.form.keyfiles.clear();
                        self.status = Some(Status::Success("Volume mounted".into()));
                    }
                    Err(e) => self.status = Some(Status::Error(e)),
                }
                Task::done(Message::Refresh)
            }
            Message::Dismount(volume) => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
                self.status = Some(Status::Progress(format!(
                    "Dismounting slot {}…",
                    volume.slot
                )));
                let b = self.backend.clone();
                Task::perform(background(move || b.dismount(&volume)), Message::Dismounted)
            }
            Message::DismountAll => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
                self.status = Some(Status::Progress("Dismounting all volumes…".into()));
                let b = self.backend.clone();
                Task::perform(background(move || b.dismount_all()), Message::Dismounted)
            }
            Message::Dismounted(result) => {
                self.busy = false;
                self.status = Some(match result {
                    Ok(()) => Status::Success("Dismounted".into()),
                    Err(e) => Status::Error(e),
                });
                Task::done(Message::Refresh)
            }
            Message::Open(path) => {
                if let Err(e) = open_in_file_manager(&path) {
                    self.status = Some(Status::Error(format!(
                        "Could not open {}: {e}",
                        path.display()
                    )));
                }
                Task::none()
            }
        }
    }

    /// Turn the form into a request. The password is copied into a
    /// `SecretString`, which is zeroized on drop.
    fn build_request(&self) -> Result<MountRequest, String> {
        let pim = match self.form.pim.trim() {
            "" => None,
            p => match p.parse::<u32>() {
                Ok(n) if n <= MAX_PIM => Some(n),
                _ => return Err(format!("PIM must be a number from 0 to {MAX_PIM}")),
            },
        };
        let mount_point = match self.form.mount_point.trim() {
            "" => None,
            m => Some(PathBuf::from(m)),
        };
        let request = MountRequest {
            volume: PathBuf::from(self.form.volume.trim()),
            mount_point,
            password: SecretString::from(self.form.password.0.as_str()),
            pim,
            keyfiles: self.form.keyfiles.clone(),
            read_only: self.form.read_only,
        };
        request.validate().map_err(|e| e.to_string())?;
        Ok(request)
    }

    fn view(&self) -> Element<'_, Message> {
        let header = row![
            text("Cryptdeck").size(28),
            container(self.backend_label())
                .width(Length::Fill)
                .align_right(Length::Fill),
        ]
        .align_y(Alignment::Center);

        let content = column![
            header,
            rule::horizontal(1),
            self.volumes_view(),
            rule::horizontal(1),
            self.mount_view(),
            self.status_view(),
        ]
        .spacing(16)
        .padding(20);

        scrollable(content).into()
    }

    fn backend_label(&self) -> Element<'_, Message> {
        match &self.backend_info {
            None => text(self.backend.name()).size(13).into(),
            Some(Ok(v)) => text(format!("{}: {v}", self.backend.name()))
                .size(13)
                .into(),
            Some(Err(e)) => text(format!("{} unavailable: {e}", self.backend.name()))
                .size(13)
                .style(text::danger)
                .into(),
        }
    }

    fn volumes_view(&self) -> Element<'_, Message> {
        let title = row![
            text("Mounted volumes").size(20).width(Length::Fill),
            button("Refresh")
                .on_press(Message::Refresh)
                .style(button::secondary),
            button("Dismount all")
                .on_press_maybe(
                    (!self.busy && !self.volumes.is_empty()).then_some(Message::DismountAll)
                )
                .style(button::danger),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let body: Element<_> = if self.volumes.is_empty() {
            text("No volumes mounted").style(text::secondary).into()
        } else {
            Column::with_children(self.volumes.iter().map(|v| self.volume_row(v)))
                .spacing(6)
                .into()
        };

        column![title, body].spacing(10).into()
    }

    fn volume_row<'a>(&self, v: &'a MountedVolume) -> Element<'a, Message> {
        let mount_point = v
            .mount_point
            .as_ref()
            .map(|m| m.display().to_string())
            .unwrap_or_else(|| "(not mounted)".into());

        let open = button("Open")
            .style(button::secondary)
            .on_press_maybe(v.mount_point.clone().map(Message::Open));
        let dismount = button("Dismount")
            .style(button::danger)
            .on_press_maybe((!self.busy).then(|| Message::Dismount(v.clone())));

        container(
            row![
                text(format!("#{}", v.slot)).width(36),
                column![
                    text(v.volume.display().to_string()),
                    text(format!(
                        "{}  →  {}",
                        v.virtual_device.display(),
                        mount_point
                    ))
                    .size(13)
                    .style(text::secondary),
                ]
                .width(Length::Fill),
                open,
                dismount,
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(10)
        .style(container::rounded_box)
        .into()
    }

    fn mount_view(&self) -> Element<'_, Message> {
        let f = &self.form;
        let label = |s| text(s).width(120);

        let volume = row![
            label("Volume"),
            text_input("/path/to/volume.hc or /dev/sdX", &f.volume)
                .on_input(Message::VolumeChanged)
                .on_submit(Message::Mount),
            button("Browse…")
                .on_press(Message::BrowseVolume)
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mount_point = row![
            label("Mount point"),
            text_input("Optional; VeraCrypt picks one if empty", &f.mount_point)
                .on_input(Message::MountPointChanged)
                .on_submit(Message::Mount),
            button("Browse…")
                .on_press(Message::BrowseMountPoint)
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let password = row![
            label("Password"),
            text_input("Password", f.password.0.as_str())
                .secure(true)
                .on_input(|p| Message::PasswordChanged(Password(Zeroizing::new(p))))
                .on_submit(Message::Mount),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let pim = row![
            label("PIM"),
            row![
                text_input("Optional; leave empty for default", &f.pim)
                    .secure(true)
                    .on_input(Message::PimChanged)
                    .on_submit(Message::Mount)
                    .width(280),
                checkbox(f.read_only)
                    .label("Mount read-only")
                    .on_toggle(Message::ReadOnlyToggled),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut keyfiles = Column::new().spacing(4);
        for (i, kf) in f.keyfiles.iter().enumerate() {
            keyfiles = keyfiles.push(
                row![
                    text(kf.display().to_string()).size(13).width(Length::Fill),
                    button(text("Remove").size(12))
                        .style(button::text)
                        .on_press(Message::RemoveKeyfile(i)),
                ]
                .align_y(Alignment::Center),
            );
        }
        let keyfiles = row![
            label("Keyfiles"),
            column![
                keyfiles,
                button("Add keyfiles…")
                    .on_press(Message::AddKeyfiles)
                    .style(button::secondary),
            ]
            .spacing(6)
            .width(Length::Fill),
        ]
        .spacing(8);

        let mount = button(if self.busy { "Working…" } else { "Mount" })
            .padding([8, 24])
            .on_press_maybe((!self.busy).then_some(Message::Mount));

        column![
            text("Mount a volume").size(20),
            volume,
            mount_point,
            password,
            pim,
            keyfiles,
            mount,
        ]
        .spacing(10)
        .into()
    }

    fn status_view(&self) -> Element<'_, Message> {
        match &self.status {
            None => column![].into(),
            Some(Status::Progress(s)) => text(s.as_str()).style(text::secondary).into(),
            Some(Status::Success(s)) => text(s.as_str()).style(text::success).into(),
            Some(Status::Error(s)) => text(s.as_str()).style(text::danger).into(),
        }
    }
}

/// Run blocking backend work on a worker thread so the UI stays responsive.
/// Errors become display strings; a panic in the worker becomes an error
/// instead of taking the window down with it.
fn background<T: Send + 'static>(
    f: impl FnOnce() -> cryptdeck_core::Result<T> + Send + 'static,
) -> impl Future<Output = Result<T, String>> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f().map_err(|e| e.to_string()));
    });
    async move {
        rx.await
            .unwrap_or_else(|_| Err("Internal error: the background task crashed".into()))
    }
}

fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";

    let mut child = std::process::Command::new(program).arg(path).spawn()?;
    // Reap the child when it exits so it doesn't linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}
