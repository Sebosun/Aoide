//! OS media controls (MPRIS on Linux): media keys, desktop "now playing" widgets.

use crate::core::PlaybackStatus;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Weak};

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};

use crate::core::Core;

pub enum MediaUpdate {
    Playback(PlaybackStatus),
    Metadata {
        title: String,
        album: String,
        artist: String,
        cover: Option<String>,
    },
}

/// Starts the media-controls thread. Returns `None` when the platform service is unavailable.
pub fn spawn(core: &Arc<Core>) -> Option<Sender<MediaUpdate>> {
    let weak: Weak<Core> = Arc::downgrade(core);
    let (tx, rx) = channel::<MediaUpdate>();
    let (ready_tx, ready_rx) = channel::<bool>();
    std::thread::Builder::new()
        .name("aoide-media".into())
        .spawn(move || {
            // Per-process name (MPRIS convention) so a second instance cannot clash.
            let dbus_name = format!("aoide.instance{}", std::process::id());
            let config = PlatformConfig {
                display_name: "Aoide",
                dbus_name: &dbus_name,
                hwnd: None,
            };
            let Ok(mut controls) = MediaControls::new(config) else {
                let _ = ready_tx.send(false);
                return;
            };
            let attached = controls.attach(move |event| {
                if let Some(core) = weak.upgrade() {
                    handle(core, event);
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

fn handle(core: Arc<Core>, event: MediaControlEvent) {
    match event {
        MediaControlEvent::Play => {
            let _ = core.resume();
        }
        MediaControlEvent::Pause => core.pause(),
        MediaControlEvent::Toggle => core.toggle(),
        MediaControlEvent::Stop => core.stop(),
        MediaControlEvent::Next => {
            tauri::async_runtime::spawn(async move {
                let _ = core.skip_chapter(1).await;
            });
        }
        MediaControlEvent::Previous => {
            tauri::async_runtime::spawn(async move {
                let _ = core.skip_chapter(-1).await;
            });
        }
        _ => {}
    }
}
