// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use cryptdeck_core::{Backend, MountRequest, MountedVolume, MAX_PIM};
use iced::widget::{
    button, checkbox, column, container, row, rule, scrollable, text, text_input, Column,
};
use iced::{Alignment, Element, Length, Task, Theme};
use secrecy::SecretString;
use zeroize::Zeroizing;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Cryptdeck")
        .theme(App::theme)
        .window_size((860.0, 680.0))
        .run()
}

struct App {
    backend: Arc<dyn Backend>,
    backend_info: Result<String, String>,
    volumes: Vec<MountedVolume>,
    form: MountForm,
    busy: bool,
    status: Option<Status>,
}

#[derive(Default)]
struct MountForm {
    volume: String,
    mount_point: String,
    /// Zeroized when replaced or dropped. iced's text input keeps its own
    /// copy while editing, which we can't wipe; see README.
    password: Zeroizing<String>,
    pim: String,
    keyfiles: Vec<PathBuf>,
    read_only: bool,
}

enum Status {
    Info(String),
    Error(String),
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
    PasswordChanged(String),
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
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let backend = cryptdeck_core::default_backend();
        let app = App {
            backend: backend.clone(),
            backend_info: Ok(String::new()),
            volumes: Vec::new(),
            form: MountForm::default(),
            busy: false,
            status: None,
        };
        let b = backend.clone();
        let check = Task::perform(
            blocking(move || b.check().map_err(|e| e.to_string())),
            Message::Checked,
        );
        (app, Task::batch([check, Task::done(Message::Refresh)]))
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Checked(info) => {
                self.backend_info = info;
                Task::none()
            }
            Message::Refresh => {
                let b = self.backend.clone();
                Task::perform(
                    blocking(move || b.list().map_err(|e| e.to_string())),
                    Message::Refreshed,
                )
            }
            Message::Refreshed(Ok(volumes)) => {
                self.volumes = volumes;
                Task::none()
            }
            Message::Refreshed(Err(e)) => {
                // Don't overwrite a more useful error already on screen.
                if self.backend_info.is_ok() {
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
                self.form.password = Zeroizing::new(p);
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
                self.status = Some(Status::Info("Mounting…".into()));
                let b = self.backend.clone();
                Task::perform(
                    blocking(move || b.mount(&request).map_err(|e| e.to_string())),
                    Message::Mounted,
                )
            }
            Message::Mounted(result) => {
                self.busy = false;
                match result {
                    Ok(()) => {
                        // Clear secrets from the form once they've been used.
                        self.form.password = Zeroizing::default();
                        self.form.pim.clear();
                        self.form.keyfiles.clear();
                        self.status = Some(Status::Info("Volume mounted".into()));
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
                self.status = Some(Status::Info(format!("Dismounting slot {}…", volume.slot)));
                let b = self.backend.clone();
                Task::perform(
                    blocking(move || b.dismount(&volume).map_err(|e| e.to_string())),
                    Message::Dismounted,
                )
            }
            Message::DismountAll => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
                self.status = Some(Status::Info("Dismounting all volumes…".into()));
                let b = self.backend.clone();
                Task::perform(
                    blocking(move || b.dismount_all().map_err(|e| e.to_string())),
                    Message::Dismounted,
                )
            }
            Message::Dismounted(result) => {
                self.busy = false;
                self.status = Some(match result {
                    Ok(()) => Status::Info("Dismounted".into()),
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
            password: SecretString::from(self.form.password.as_str()),
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
            Ok(v) if v.is_empty() => text(self.backend.name()).size(13).into(),
            Ok(v) => text(format!("{}: {v}", self.backend.name()))
                .size(13)
                .into(),
            Err(e) => text(format!("{} unavailable: {e}", self.backend.name()))
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
                .on_input(Message::VolumeChanged),
            button("Browse…")
                .on_press(Message::BrowseVolume)
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mount_point = row![
            label("Mount point"),
            text_input("Optional; VeraCrypt picks one if empty", &f.mount_point)
                .on_input(Message::MountPointChanged),
            button("Browse…")
                .on_press(Message::BrowseMountPoint)
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let password = row![
            label("Password"),
            text_input("Password", &f.password)
                .secure(true)
                .on_input(Message::PasswordChanged)
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
            Some(Status::Info(s)) => text(s.as_str()).style(text::success).into(),
            Some(Status::Error(s)) => text(s.as_str()).style(text::danger).into(),
        }
    }
}

/// Run blocking backend work on a worker thread so the UI stays responsive.
fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl Future<Output = T> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.expect("worker thread panicked") }
}

fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";

    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(drop)
}
