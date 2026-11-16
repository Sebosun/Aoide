//! MPRIS (`org.mpris.MediaPlayer2`) served over the session bus with zbus. zbus's executor waits
//! on the bus socket and calls the player's methods as they come; what plays is sent from the
//! updates thread as `PropertiesChanged` signals. Neither side polls.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, Weak};

use zbus::blocking::{connection, Connection};
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use super::{handle, Control, MediaUpdate};
use crate::core::{Core, PlaybackStatus};

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

fn lock(shown: &Mutex<Shown>) -> std::sync::MutexGuard<'_, Shown> {
    shown
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the desktop is shown.
#[derive(Default)]
struct Shown {
    status: Option<PlaybackStatus>,
    title: String,
    album: String,
    artist: String,
    cover: Option<String>,
}

impl Shown {
    fn status(&self) -> &'static str {
        match self.status {
            Some(PlaybackStatus::Playing | PlaybackStatus::Buffering) => "Playing",
            Some(PlaybackStatus::Paused) => "Paused",
            Some(PlaybackStatus::Stopped) | None => "Stopped",
        }
    }

    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        let mut put = |key: &str, value: Value<'_>| {
            if let Ok(value) = OwnedValue::try_from(value) {
                map.insert(key.to_string(), value);
            }
        };
        put(
            "mpris:trackid",
            Value::from(ObjectPath::from_static_str_unchecked("/org/aoide/track")),
        );
        put("xesam:title", Value::from(self.title.as_str()));
        put("xesam:album", Value::from(self.album.as_str()));
        put("xesam:artist", Value::from(vec![self.artist.as_str()]));
        if let Some(cover) = &self.cover {
            put("mpris:artUrl", Value::from(format!("file://{cover}")));
        }
        map
    }
}

/// `org.mpris.MediaPlayer2`: who the player is.
struct Root;

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "Aoide".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

/// `org.mpris.MediaPlayer2.Player`: what plays, and the keys that control it.
struct Player {
    core: Weak<Core>,
    shown: Arc<Mutex<Shown>>,
}

impl Player {
    fn control(&self, control: Control) {
        if let Some(core) = self.core.upgrade() {
            handle(core, control);
        }
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn play(&self) {
        self.control(Control::Play);
    }
    fn pause(&self) {
        self.control(Control::Pause);
    }
    fn play_pause(&self) {
        self.control(Control::Toggle);
    }
    fn stop(&self) {
        self.control(Control::Stop);
    }
    fn next(&self) {
        self.control(Control::Next);
    }
    fn previous(&self) {
        self.control(Control::Previous);
    }
    fn seek(&self, _offset: i64) {}
    fn set_position(&self, _track: ObjectPath<'_>, _position: i64) {}
    fn open_uri(&self, _uri: &str) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        lock(&self.shown).status().into()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        lock(&self.shown).metadata()
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

pub fn spawn(core: Weak<Core>) -> Option<Sender<MediaUpdate>> {
    let shown = Arc::new(Mutex::new(Shown::default()));
    // Per-process name (MPRIS convention) so a second instance cannot clash.
    let name = format!(
        "org.mpris.MediaPlayer2.aoide.instance{}",
        std::process::id()
    );
    let player = Player {
        core,
        shown: Arc::clone(&shown),
    };
    let connection = connection::Builder::session()
        .and_then(|b| b.name(name))
        .and_then(|b| b.serve_at(PATH, Root))
        .and_then(|b| b.serve_at(PATH, player))
        .and_then(|b| b.build());
    let connection = match connection {
        Ok(connection) => connection,
        Err(error) => {
            tracing::warn!(event = "media.start", %error, "No media controls: the session bus is unavailable");
            return None;
        }
    };
    let (tx, rx) = channel::<MediaUpdate>();
    std::thread::Builder::new()
        .name("aoide-media".into())
        .spawn(move || {
            while let Ok(update) = rx.recv() {
                let changed = {
                    let mut shown = lock(&shown);
                    match update {
                        MediaUpdate::Playback(status) => {
                            shown.status = Some(status);
                            ("PlaybackStatus", Value::from(shown.status()))
                        }
                        MediaUpdate::Metadata {
                            title,
                            album,
                            artist,
                            cover,
                        } => {
                            *shown = Shown {
                                status: shown.status,
                                title,
                                album,
                                artist,
                                cover,
                            };
                            ("Metadata", Value::from(shown.metadata()))
                        }
                    }
                };
                if let Err(error) = announce(&connection, changed) {
                    tracing::warn!(event = "media.update", %error, "Could not update the media controls");
                }
            }
        })
        .ok()?;
    Some(tx)
}

/// Tells listeners that one of the player's properties changed.
fn announce(connection: &Connection, (name, value): (&str, Value<'_>)) -> zbus::Result<()> {
    let changed = HashMap::from([(name, value)]);
    connection.emit_signal(
        None::<()>,
        PATH,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        &(PLAYER, changed, Vec::<&str>::new()),
    )
}
