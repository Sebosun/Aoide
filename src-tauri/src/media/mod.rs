//! OS media controls: media keys and the desktop's "now playing" panel. On Linux Aoide serves
//! MPRIS itself over D-Bus ([`mpris`]), waiting on the bus rather than polling it; elsewhere
//! `souvlaki` speaks the platform's API.

use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::core::{Core, PlaybackStatus};

#[cfg(target_os = "linux")]
mod mpris;
#[cfg(not(target_os = "linux"))]
mod souvlaki;

/// What the desktop is told.
pub enum MediaUpdate {
    Playback(PlaybackStatus),
    Metadata {
        title: String,
        album: String,
        artist: String,
        cover: Option<String>,
    },
}

/// What the desktop asks of the player (a media key, a button in its panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
}

/// Starts the media controls. Returns `None` when the platform service is unavailable.
pub fn spawn(core: &Arc<Core>) -> Option<Sender<MediaUpdate>> {
    #[cfg(target_os = "linux")]
    return mpris::spawn(Arc::downgrade(core));
    #[cfg(not(target_os = "linux"))]
    return souvlaki::spawn(Arc::downgrade(core));
}

fn handle(core: Arc<Core>, control: Control) {
    match control {
        Control::Play => {
            let _ = core.resume();
        }
        Control::Pause => core.pause(),
        Control::Toggle => core.toggle(),
        Control::Stop => core.stop(),
        Control::Next | Control::Previous => {
            let step = if control == Control::Next { 1 } else { -1 };
            tauri::async_runtime::spawn(async move {
                let _ = core.skip_chapter(step).await;
            });
        }
    }
}
