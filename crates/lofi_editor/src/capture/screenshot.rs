//! `--screenshot`: let the scene settle, save the frame, quit.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::launch::LaunchOptions;

/// Frames to wait so meshes upload and shadow cascades settle.
const SETTLE_FRAMES: u32 = 30;

pub(super) fn wants_screenshot(options: Res<LaunchOptions>) -> bool {
    options.screenshot.is_some()
}

pub(super) fn capture_when_settled(
    mut commands: Commands,
    options: Res<LaunchOptions>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames != SETTLE_FRAMES {
        return;
    }
    let Some(path) = options.screenshot.clone() else {
        return;
    };
    let mut save = save_to_disk(path);
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save(captured);
            exit.write(AppExit::Success);
        },
    );
}
