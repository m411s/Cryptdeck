//! System tray icon with a menu for quick unlock, open and lock.
//!
//! Linux only for now, using the StatusNotifierItem D-Bus protocol (KDE,
//! Xfce, Cinnamon, and GNOME with the AppIndicator extension, which Ubuntu
//! ships by default). It needs no GTK, so the AppImage stays self-contained.
//! On other platforms, and on desktops without a tray, [`subscription`]
//! reports the tray as unavailable and the app works as before.

use std::path::PathBuf;

use iced::Subscription;

/// Something the user chose in the tray.
// Only produced by the Linux tray so far.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone)]
pub enum Event {
    Show,
    Unlock(PathBuf),
    Open(PathBuf),
    Lock(PathBuf),
    LockAll,
    Quit,
}

/// What the tray subscription reports to the app.
// Only produced by the Linux tray so far.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone)]
pub enum Update {
    Ready(Handle),
    Unavailable(String),
    Event(Event),
}

/// A volume as the tray menu shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub path: PathBuf,
    pub unlocked: bool,
    pub can_open: bool,
}

/// Everything the tray displays. The app pushes a new state only when it
/// changes, to avoid needless D-Bus traffic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    pub volumes: Vec<Volume>,
    pub busy: bool,
}

pub fn subscription() -> Subscription<Update> {
    imp::subscription()
}

/// A handle for updating the running tray.
#[derive(Clone)]
pub struct Handle(imp::Handle);

impl Handle {
    pub fn set_state(&self, state: State) {
        imp::set_state(&self.0, state);
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tray::Handle")
    }
}

/// Menu labels treat `_` as an access-key marker; `__` shows a literal `_`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn escape_label(text: &str) -> String {
    text.replace('_', "__")
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::Arc;

    use iced::futures::channel::mpsc;
    use iced::futures::{SinkExt, Stream, StreamExt};
    use iced::Subscription;
    use ksni::blocking::TrayMethods;
    use ksni::menu::{MenuItem, StandardItem};

    use rust_i18n::t;

    use super::{escape_label, Event, State, Update};

    pub type Handle = Arc<ksni::blocking::Handle<Tray>>;

    /// 64×64 RGBA, rendered from `assets/cryptdeck.svg`.
    const ICON_RGBA: &[u8] = include_bytes!("../../../assets/icon-64.rgba");

    pub struct Tray {
        events: mpsc::UnboundedSender<Event>,
        state: State,
    }

    impl Tray {
        fn item(&self, label: String, enabled: bool, event: Event) -> MenuItem<Self> {
            StandardItem {
                label,
                enabled,
                activate: Box::new(move |tray: &mut Self| {
                    let _ = tray.events.unbounded_send(event.clone());
                }),
                ..Default::default()
            }
            .into()
        }
    }

    impl ksni::Tray for Tray {
        fn id(&self) -> String {
            "cryptdeck".into()
        }

        fn title(&self) -> String {
            "Cryptdeck".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            // ksni wants ARGB32 in network byte order.
            let data = ICON_RGBA
                .chunks_exact(4)
                .flat_map(|p| [p[3], p[0], p[1], p[2]])
                .collect();
            vec![ksni::Icon {
                width: 64,
                height: 64,
                data,
            }]
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            let unlocked = self.state.volumes.iter().filter(|v| v.unlocked).count();
            let description = match unlocked {
                0 => t!("notice.all_locked").to_string(),
                1 => t!("tray.one_unlocked").to_string(),
                n => t!("tray.n_unlocked", count = n).to_string(),
            };
            ksni::ToolTip {
                title: "Cryptdeck".into(),
                description,
                ..Default::default()
            }
        }

        /// Left click opens the window.
        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.events.unbounded_send(Event::Show);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let idle = !self.state.busy;
            let mut menu = vec![
                self.item(t!("tray.show").into(), true, Event::Show),
                MenuItem::Separator,
            ];
            for v in &self.state.volumes {
                let name = escape_label(&v.name);
                if v.unlocked {
                    menu.push(self.item(
                        t!("tray.open", name = name).into(),
                        v.can_open,
                        Event::Open(v.path.clone()),
                    ));
                    menu.push(self.item(
                        t!("tray.lock", name = name).into(),
                        idle,
                        Event::Lock(v.path.clone()),
                    ));
                } else {
                    menu.push(self.item(
                        t!("tray.unlock", name = name).into(),
                        idle,
                        Event::Unlock(v.path.clone()),
                    ));
                }
            }
            if !self.state.volumes.is_empty() {
                menu.push(MenuItem::Separator);
            }
            let any_unlocked = self.state.volumes.iter().any(|v| v.unlocked);
            menu.push(self.item(t!("lock_all").into(), idle && any_unlocked, Event::LockAll));
            menu.push(self.item(t!("tray.quit").into(), true, Event::Quit));
            menu
        }
    }

    pub fn subscription() -> Subscription<Update> {
        Subscription::run(worker)
    }

    fn worker() -> impl Stream<Item = Update> {
        iced::stream::channel(32, async |mut output: mpsc::Sender<Update>| {
            let (events, mut incoming) = mpsc::unbounded();
            let tray = Tray {
                events,
                state: State::default(),
            };
            match tray.spawn() {
                Ok(handle) => {
                    let _ = output
                        .send(Update::Ready(super::Handle(Arc::new(handle))))
                        .await;
                    while let Some(event) = incoming.next().await {
                        if output.send(Update::Event(event)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let _ = output.send(Update::Unavailable(e.to_string())).await;
                }
            }
            // Keep the subscription alive so it isn't restarted.
            std::future::pending::<()>().await;
        })
    }

    pub fn set_state(handle: &Handle, state: State) {
        handle.update(|tray| tray.state = state);
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use iced::Subscription;

    use super::{State, Update};

    #[derive(Clone)]
    pub struct Handle;

    pub fn subscription() -> Subscription<Update> {
        Subscription::none()
    }

    pub fn set_state(_handle: &Handle, _state: State) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underscores_are_escaped_for_menus() {
        assert_eq!(escape_label("my_vault.hc"), "my__vault.hc");
    }
}
