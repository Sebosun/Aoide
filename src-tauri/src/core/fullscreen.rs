//! Whether the window fills the screen. The window says so whenever it may have changed (F11,
//! Esc or the window manager's own shortcut); the reader is told only when it did change, so
//! Esc and the window manager stay in step. Aoide always starts windowed, and the choice is not
//! saved.

use std::sync::Mutex;

use super::{lock, CoreEvent};

/// What [`Fullscreen`] needs from the rest of the app.
pub(super) trait FullscreenHost {
    fn emit(&self, event: CoreEvent);
}

#[derive(Default)]
pub(super) struct Fullscreen {
    /// As the window last said.
    on: Mutex<bool>,
}

impl Fullscreen {
    /// The window says whether it fills the screen now.
    pub fn seen(&self, host: &impl FullscreenHost, on: bool) {
        {
            let mut was = lock(&self.on);
            if *was == on {
                return;
            }
            tracing::info!(
                event = "window.fullscreen",
                from = *was,
                to = on,
                "The window changed between fullscreen and windowed"
            );
            *was = on;
        }
        host.emit(CoreEvent::Fullscreen(on));
    }
}

impl FullscreenHost for super::Core {
    fn emit(&self, event: CoreEvent) {
        super::Core::emit(self, event);
    }
}

impl super::Core {
    /// The window says whether it fills the screen now (after a resize, or a change of its
    /// state); the reader hears of a change.
    pub fn window_fullscreen(&self, on: bool) {
        self.fullscreen.seen(self, on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Window {
        told: Mutex<Vec<bool>>,
    }

    impl FullscreenHost for Window {
        fn emit(&self, event: CoreEvent) {
            if let CoreEvent::Fullscreen(on) = event {
                lock(&self.told).push(on);
            }
        }
    }

    #[test]
    fn the_reader_is_told_only_when_the_window_changes() {
        let fullscreen = Fullscreen::default();
        let window = Window::default();
        // Windowed at the start: a resize that is not a change says nothing.
        fullscreen.seen(&window, false);
        fullscreen.seen(&window, true);
        fullscreen.seen(&window, true);
        fullscreen.seen(&window, false);
        assert_eq!(*lock(&window.told), [true, false]);
    }
}
