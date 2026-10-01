//! Keyboard shortcuts for menu commands. Brush keys live in `sculpt::adjust`.

use bevy::prelude::*;

use super::dialogs::NewMapDialog;
use crate::camera::FrameMapMessage;
use crate::document::{FileAction, FileActionMessage, HistoryMessage};
use crate::view::{Shading, ViewSettings};

pub(super) fn global_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<ViewSettings>,
    mut new_map: ResMut<NewMapDialog>,
    mut file_actions: MessageWriter<FileActionMessage>,
    mut history: MessageWriter<HistoryMessage>,
    mut frame: MessageWriter<FrameMapMessage>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let pressed = |key| keys.just_pressed(key);

    if ctrl {
        if pressed(KeyCode::KeyN) {
            new_map.open();
        }
        if pressed(KeyCode::KeyO) {
            file_actions.write(FileActionMessage::new(FileAction::Open));
        }
        if pressed(KeyCode::KeyS) {
            let action = if shift {
                FileAction::SaveAs
            } else {
                FileAction::Save
            };
            file_actions.write(FileActionMessage::new(action));
        }
        if pressed(KeyCode::KeyZ) {
            history.write(if shift {
                HistoryMessage::Redo
            } else {
                HistoryMessage::Undo
            });
        }
        if pressed(KeyCode::KeyY) {
            history.write(HistoryMessage::Redo);
        }
        return;
    }

    if pressed(KeyCode::KeyF) {
        view.shading = match view.shading {
            Shading::Flat => Shading::Smooth,
            Shading::Smooth => Shading::Flat,
        };
    }
    if pressed(KeyCode::KeyG) {
        view.wireframe = !view.wireframe;
    }
    if pressed(KeyCode::Home) {
        frame.write(FrameMapMessage);
    }
}
