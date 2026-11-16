//! Desktop notifications: the freedesktop notification service (`org.freedesktop.Notifications`)
//! over the session bus with zbus, as the media keys are served. Clicking a notification (its
//! `default` action) shows, unminimises and focuses the window and opens the item it is about.
//! Elsewhere nothing is sent yet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use crate::core::{CoreEvent, Notice, Notifier};

/// The window's notifier: knows whether the window is in front from its focus events.
pub struct Desktop {
    app: AppHandle,
    focused: Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    bus: std::sync::OnceLock<Result<linux::Bus, String>>,
}

impl Desktop {
    pub fn new(app: AppHandle) -> Self {
        let focused = Arc::new(AtomicBool::new(true));
        if let Some(window) = app.get_webview_window("main") {
            focused.store(window.is_focused().unwrap_or(true), Ordering::Relaxed);
            let seen = Arc::clone(&focused);
            window.on_window_event(move |event| {
                if let WindowEvent::Focused(now) = event {
                    seen.store(*now, Ordering::Relaxed);
                }
            });
        }
        Self {
            app,
            focused,
            #[cfg(target_os = "linux")]
            bus: std::sync::OnceLock::new(),
        }
    }
}

impl Notifier for Desktop {
    fn focused(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }

    #[cfg(target_os = "linux")]
    fn send(&self, notice: Notice) -> Result<(), String> {
        let app = self.app.clone();
        let bus = self
            .bus
            .get_or_init(|| linux::Bus::connect(Box::new(move |opens| clicked(&app, opens))));
        bus.as_ref().map_err(Clone::clone)?.send(notice)
    }

    #[cfg(not(target_os = "linux"))]
    fn send(&self, _: Notice) -> Result<(), String> {
        Err("desktop notifications are not sent on this system yet".into())
    }
}

/// A notification was clicked: the window comes to the front, on the item's page.
fn clicked(app: &AppHandle, opens: Option<String>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(id) = opens {
        let event = CoreEvent::OpenItem(id);
        if let Ok(payload) = event.payload() {
            let _ = app.emit(event.name(), payload);
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use zbus::blocking::{Connection, MessageIterator};
    use zbus::zvariant::Value;

    use crate::core::Notice;

    const SERVICE: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    type OnClick = Box<dyn Fn(Option<String>) + Send>;

    /// The session bus, and what each notification shown opens when clicked.
    pub struct Bus {
        connection: Connection,
        shown: Arc<Mutex<HashMap<u32, Option<String>>>>,
    }

    impl Bus {
        /// Connects to the session bus, and listens on a thread of its own for clicks and
        /// closes.
        pub fn connect(on_click: OnClick) -> Result<Self, String> {
            Self::over(Connection::session().map_err(|e| e.to_string())?, on_click)
        }

        /// The notification service on `connection`'s bus.
        pub fn over(connection: Connection, on_click: OnClick) -> Result<Self, String> {
            let rule = format!("type='signal',interface='{SERVICE}',path='{PATH}'");
            let signals = MessageIterator::for_match_rule(rule.as_str(), &connection, Some(64))
                .map_err(|e| e.to_string())?;
            let shown: Arc<Mutex<HashMap<u32, Option<String>>>> = Arc::default();
            let seen = Arc::clone(&shown);
            std::thread::Builder::new()
                .name("aoide-notices".into())
                .spawn(move || listen(signals, &seen, &on_click))
                .map_err(|e| e.to_string())?;
            Ok(Self { connection, shown })
        }

        pub fn send(&self, notice: Notice) -> Result<(), String> {
            let hints: HashMap<&str, Value<'_>> = HashMap::new();
            let reply = self
                .connection
                .call_method(
                    Some(SERVICE),
                    PATH,
                    Some(SERVICE),
                    "Notify",
                    &(
                        "Aoide",
                        0u32,
                        "",
                        notice.title.as_str(),
                        notice.body.as_str(),
                        vec!["default", "Open"],
                        hints,
                        -1i32,
                    ),
                )
                .map_err(|e| e.to_string())?;
            let id: u32 = reply.body().deserialize().map_err(|e| e.to_string())?;
            self.shown
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(id, notice.opens);
            Ok(())
        }
    }

    /// Opens what a clicked notification is about, and forgets the ones closed.
    fn listen(
        signals: MessageIterator,
        shown: &Mutex<HashMap<u32, Option<String>>>,
        on_click: &OnClick,
    ) {
        for message in signals.flatten() {
            let header = message.header();
            let member = header.member().map(|m| m.as_str().to_owned());
            let forget = |id: u32| shown.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            match member.as_deref() {
                Some("ActionInvoked") => {
                    if let Ok((id, action)) = message.body().deserialize::<(u32, String)>() {
                        if action == "default" {
                            if let Some(opens) = forget(id) {
                                on_click(opens);
                            }
                        }
                    }
                }
                Some("NotificationClosed") => {
                    if let Ok((id, _reason)) = message.body().deserialize::<(u32, u32)>() {
                        forget(id);
                    }
                }
                _ => {}
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::collections::HashMap;
        use std::io::BufRead;
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::Duration;

        use zbus::blocking::connection::Builder;
        use zbus::zvariant::OwnedValue;

        use super::*;

        /// A notification service that keeps what it was asked to show.
        struct Service(Shown);

        /// Each notification's title, body and actions.
        type Shown = Arc<Mutex<Vec<(String, String, Vec<String>)>>>;

        #[zbus::interface(name = "org.freedesktop.Notifications")]
        impl Service {
            #[allow(clippy::too_many_arguments)]
            fn notify(
                &self,
                _app: &str,
                _replaces: u32,
                _icon: &str,
                summary: &str,
                body: &str,
                actions: Vec<String>,
                _hints: HashMap<String, OwnedValue>,
                _timeout: i32,
            ) -> u32 {
                let mut shown = self.0.lock().unwrap();
                shown.push((summary.into(), body.into(), actions));
                shown.len() as u32
            }
        }

        /// A bus of its own, so nothing reaches the desktop; `None` without `dbus-daemon`.
        fn private_bus() -> Option<(std::process::Child, String)> {
            let mut daemon = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .ok()?;
            let mut address = String::new();
            std::io::BufReader::new(daemon.stdout.take()?)
                .read_line(&mut address)
                .ok()?;
            Some((daemon, address.trim().to_owned()))
        }

        #[test]
        fn a_notice_is_shown_and_clicking_it_opens_its_item() {
            let Some((mut daemon, address)) = private_bus() else {
                eprintln!("no dbus-daemon: skipped");
                return;
            };
            let shown = Arc::default();
            let service = Builder::address(address.as_str())
                .unwrap()
                .name(SERVICE)
                .unwrap()
                .serve_at(PATH, Service(Arc::clone(&shown)))
                .unwrap()
                .build()
                .unwrap();
            let client = Builder::address(address.as_str()).unwrap().build().unwrap();
            let (clicks, clicked) = mpsc::channel();
            let bus =
                Bus::over(client, Box::new(move |opens| clicks.send(opens).unwrap())).unwrap();

            for (title, opens) in [("Quay Lights", Some("b1")), ("Books imported", None)] {
                bus.send(Notice {
                    title: title.into(),
                    body: "Prepared.".into(),
                    opens: opens.map(str::to_owned),
                })
                .unwrap();
            }
            assert_eq!(
                shown.lock().unwrap()[0],
                (
                    "Quay Lights".to_string(),
                    "Prepared.".to_string(),
                    vec!["default".to_string(), "Open".to_string()]
                )
            );
            let signal = |member: &str, body: &(u32, &str)| {
                service
                    .emit_signal(None::<&str>, PATH, SERVICE, member, body)
                    .unwrap()
            };
            signal("ActionInvoked", &(1, "default"));
            assert_eq!(
                clicked.recv_timeout(Duration::from_secs(5)).unwrap(),
                Some("b1".to_string())
            );
            // A second click on the same one, now gone, opens nothing.
            signal("ActionInvoked", &(1, "default"));
            signal("ActionInvoked", &(2, "default"));
            assert_eq!(clicked.recv_timeout(Duration::from_secs(5)).unwrap(), None);
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
    }
}
