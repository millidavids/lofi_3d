//! Turns the window's close button into a quit that checks for unsaved work.

use bevy::prelude::*;
use bevy::window::WindowCloseRequested;

use super::file_actions::{FileAction, FileActionMessage};

pub(super) fn quit_on_close_request(
    mut requests: MessageReader<WindowCloseRequested>,
    mut actions: MessageWriter<FileActionMessage>,
) {
    if requests.read().last().is_some() {
        actions.write(FileActionMessage::new(FileAction::Quit));
    }
}
