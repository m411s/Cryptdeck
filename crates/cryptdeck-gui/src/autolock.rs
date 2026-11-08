//! Lock all volumes automatically when the computer goes to sleep or the
//! screen locks.
//!
//! Linux only for now, using systemd-logind over D-Bus:
//! - Sleep: we hold a "delay" inhibitor, so when logind announces
//!   `PrepareForSleep(true)` the system waits (a few seconds at most) while
//!   we lock, then we release it. After resume we take a new one.
//! - Screen lock: we watch our session's `LockedHint` (set by GNOME, KDE and
//!   others) and its `Lock` signal.
//!
//! Locking happens on the watcher threads, calling the backend directly, so
//! it works even with no window open and doesn't depend on the UI thread.
//! The app is told afterwards, to refresh and show what happened.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use cryptdeck_core::Backend;
use iced::futures::channel::mpsc::UnboundedSender;

/// Which triggers are on. Shared with the watcher threads, so changes in
/// the settings take effect immediately.
#[derive(Debug)]
pub struct Settings {
    pub on_sleep: AtomicBool,
    pub on_screen_lock: AtomicBool,
}

impl Settings {
    pub fn new(on_sleep: bool, on_screen_lock: bool) -> Self {
        Self {
            on_sleep: AtomicBool::new(on_sleep),
            on_screen_lock: AtomicBool::new(on_screen_lock),
        }
    }
}

// Only produced on Linux so far.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Sleep,
    ScreenLock,
}

// Only produced on Linux so far.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone)]
pub enum Event {
    /// Watching for sleep; `screen_lock` says whether screen locks can be
    /// detected too.
    Ready {
        screen_lock: bool,
    },
    Unavailable(String),
    Locked {
        reason: Reason,
        count: usize,
        error: Option<String>,
    },
}

/// Start watching in the background. Results arrive on `events`.
pub fn start(backend: Arc<dyn Backend>, settings: Arc<Settings>, events: UnboundedSender<Event>) {
    imp::start(backend, settings, events);
}

/// Lock everything that's unlocked, and report it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn lock_all(backend: &dyn Backend, reason: Reason, events: &UnboundedSender<Event>) {
    let count = match backend.list() {
        Ok(volumes) => volumes.len(),
        Err(e) => {
            let _ = events.unbounded_send(Event::Locked {
                reason,
                count: 0,
                error: Some(e.to_string()),
            });
            return;
        }
    };
    if count == 0 {
        return;
    }
    let error = backend.dismount_all().err().map(|e| e.to_string());
    let _ = events.unbounded_send(Event::Locked {
        reason,
        count,
        error,
    });
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use cryptdeck_core::Backend;
    use iced::futures::channel::mpsc::UnboundedSender;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedFd, OwnedObjectPath};

    use super::{lock_all, Event, Reason, Settings};

    const LOGIN1: &str = "org.freedesktop.login1";
    const MANAGER_PATH: &str = "/org/freedesktop/login1";
    const MANAGER: &str = "org.freedesktop.login1.Manager";
    const SESSION: &str = "org.freedesktop.login1.Session";

    pub fn start(
        backend: Arc<dyn Backend>,
        settings: Arc<Settings>,
        events: UnboundedSender<Event>,
    ) {
        std::thread::spawn(move || {
            let conn = match connect() {
                Ok(c) => c,
                Err(e) => {
                    let _ = events.unbounded_send(Event::Unavailable(e.to_string()));
                    return;
                }
            };
            {
                let (conn, backend, settings, events) = (
                    conn.clone(),
                    backend.clone(),
                    settings.clone(),
                    events.clone(),
                );
                std::thread::spawn(move || {
                    if let Err(e) = watch_sleep(&conn, &*backend, &settings, &events) {
                        let _ = events.unbounded_send(Event::Unavailable(e.to_string()));
                    }
                });
            }
            let screen_lock = match session_path(&conn) {
                Ok(path) => {
                    watch_screen_lock(conn, path, backend, settings, events.clone());
                    true
                }
                Err(_) => false,
            };
            let _ = events.unbounded_send(Event::Ready { screen_lock });
        });
    }

    fn connect() -> zbus::Result<Connection> {
        // Tests point this at a stand-in logind on the session bus.
        match std::env::var("CRYPTDECK_LOGIN1_BUS").as_deref() {
            Ok("session") => Connection::session(),
            _ => Connection::system(),
        }
    }

    fn manager(conn: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(conn, LOGIN1, MANAGER_PATH, MANAGER)
    }

    /// Ask logind to delay sleep until we release the returned descriptor.
    fn take_inhibitor(manager: &Proxy) -> Option<OwnedFd> {
        manager
            .call(
                "Inhibit",
                &(
                    "sleep",
                    "Cryptdeck",
                    "Locking encrypted volumes before sleep",
                    "delay",
                ),
            )
            .ok()
    }

    fn watch_sleep(
        conn: &Connection,
        backend: &dyn Backend,
        settings: &Settings,
        events: &UnboundedSender<Event>,
    ) -> zbus::Result<()> {
        let manager = manager(conn)?;
        // Subscribe before taking the inhibitor, so no signal is missed.
        let signals = manager.receive_signal("PrepareForSleep")?;
        let mut inhibitor = take_inhibitor(&manager);
        for message in signals {
            let going_to_sleep: bool = message.body().deserialize()?;
            if going_to_sleep {
                if settings.on_sleep.load(Ordering::Relaxed) {
                    lock_all(backend, Reason::Sleep, events);
                }
                // Let the system go to sleep.
                drop(inhibitor.take());
            } else {
                inhibitor = take_inhibitor(&manager);
            }
        }
        Ok(())
    }

    /// The object path of our login session. Apps started from a desktop
    /// often run outside the session itself, so use logind's "auto" alias
    /// (our session, or else the user's graphical one) and resolve it to the
    /// real path, which is where signals are sent from.
    fn session_path(conn: &Connection) -> zbus::Result<OwnedObjectPath> {
        let auto = Proxy::new(
            conn,
            LOGIN1,
            "/org/freedesktop/login1/session/auto",
            SESSION,
        )?;
        let id: String = auto.get_property("Id")?;
        manager(conn)?.call("GetSession", &(id,))
    }

    fn watch_screen_lock(
        conn: Connection,
        path: OwnedObjectPath,
        backend: Arc<dyn Backend>,
        settings: Arc<Settings>,
        events: UnboundedSender<Event>,
    ) {
        let on_lock = {
            let (backend, settings, events) = (backend.clone(), settings.clone(), events.clone());
            move || {
                if settings.on_screen_lock.load(Ordering::Relaxed) {
                    lock_all(&*backend, Reason::ScreenLock, &events);
                }
            }
        };

        // LockedHint turning true: what desktops set when the screen locks.
        {
            let (conn, path, on_lock) = (conn.clone(), path.clone(), on_lock.clone());
            std::thread::spawn(move || -> zbus::Result<()> {
                let session = Proxy::new(&conn, LOGIN1, path, SESSION)?;
                let mut was_locked = session.get_property::<bool>("LockedHint").unwrap_or(false);
                for change in session.receive_property_changed::<bool>("LockedHint") {
                    let locked = change.get()?;
                    if locked && !was_locked {
                        on_lock();
                    }
                    was_locked = locked;
                }
                Ok(())
            });
        }

        // The Lock signal: sent when something asks the session to lock
        // (e.g. `loginctl lock-session`), sometimes before LockedHint changes.
        // Locking twice is harmless: the second time there's nothing to lock.
        std::thread::spawn(move || -> zbus::Result<()> {
            let session = Proxy::new(&conn, LOGIN1, path, SESSION)?;
            for _ in session.receive_signal("Lock")? {
                on_lock();
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::sync::Arc;

    use cryptdeck_core::Backend;
    use iced::futures::channel::mpsc::UnboundedSender;

    use super::{Event, Settings};

    pub fn start(
        _backend: Arc<dyn Backend>,
        _settings: Arc<Settings>,
        events: UnboundedSender<Event>,
    ) {
        let _ = events.unbounded_send(Event::Unavailable(
            "not available on this platform yet".into(),
        ));
    }
}

/// A short description of what happened, for the notice banner.
pub fn describe(reason: Reason, count: usize) -> String {
    let what = match count {
        1 => "1 volume".to_string(),
        n => format!("{n} volumes"),
    };
    let why = match reason {
        Reason::Sleep => "the computer went to sleep",
        Reason::ScreenLock => "the screen was locked",
    };
    format!("Locked {what} because {why}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn describes_what_happened() {
        assert_eq!(
            describe(Reason::Sleep, 1),
            "Locked 1 volume because the computer went to sleep"
        );
        assert_eq!(
            describe(Reason::ScreenLock, 3),
            "Locked 3 volumes because the screen was locked"
        );
    }

    #[test]
    fn settings_start_as_given() {
        let s = Settings::new(true, false);
        assert!(s.on_sleep.load(Ordering::Relaxed));
        assert!(!s.on_screen_lock.load(Ordering::Relaxed));
    }
}
