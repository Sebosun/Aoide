//! Media controls through `souvlaki` (macOS, Windows).

use std::sync::mpsc::{channel, Sender};
use std::sync::Weak;

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};

use super::{handle, Control, MediaUpdate};
use crate::core::{Core, PlaybackStatus};

pub fn spawn(core: Weak<Core>) -> Option<Sender<MediaUpdate>> {
    let (tx, rx) = channel::<MediaUpdate>();
    let (ready_tx, ready_rx) = channel::<bool>();
    std::thread::Builder::new()
        .name("aoide-media".into())
        .spawn(move || {
            let config = PlatformConfig {
                display_name: "Aoide",
                dbus_name: "aoide",
                hwnd: None,
            };
            let Ok(mut controls) = MediaControls::new(config) else {
                let _ = ready_tx.send(false);
                return;
            };
            let attached = controls.attach(move |event| {
                let control = match event {
                    MediaControlEvent::Play => Control::Play,
                    MediaControlEvent::Pause => Control::Pause,
                    MediaControlEvent::Toggle => Control::Toggle,
                    MediaControlEvent::Stop => Control::Stop,
                    MediaControlEvent::Next => Control::Next,
                    MediaControlEvent::Previous => Control::Previous,
                    _ => return,
                };
                if let Some(core) = core.upgrade() {
                    handle(core, control);
                }
            });
            let _ = ready_tx.send(attached.is_ok());
            if attached.is_err() {
                return;
            }
            while let Ok(update) = rx.recv() {
                let _ = match update {
                    MediaUpdate::Playback(status) => controls.set_playback(match status {
                        PlaybackStatus::Playing | PlaybackStatus::Buffering => {
                            MediaPlayback::Playing { progress: None }
                        }
                        PlaybackStatus::Paused => MediaPlayback::Paused { progress: None },
                        PlaybackStatus::Stopped => MediaPlayback::Stopped,
                    }),
                    MediaUpdate::Metadata {
                        title,
                        album,
                        artist,
                        cover,
                    } => {
                        let cover_url = cover.map(|c| format!("file://{c}"));
                        controls.set_metadata(MediaMetadata {
                            title: Some(&title),
                            album: Some(&album),
                            artist: Some(&artist),
                            cover_url: cover_url.as_deref(),
                            duration: None,
                        })
                    }
                };
            }
        })
        .ok()?;
    ready_rx.recv().ok().filter(|ok| *ok).map(|_| tx)
}
