//! One-step changes to objects, from keys and from the panel.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lofi_editor::document::{EditHistory, MapDocument, MapFields};
use lofi_world::{ObjectId, ObjectLayer};

use super::settings::{DoodadSettings, DoodadTool, SelectedObject};

/// Turn per R press.
const ROTATE_STEP: f32 = TAU / 24.0;
/// Scale factor per `[` or `]` press.
const SCALE_STEP: f32 = 1.1;

/// Runs `edit` on a copy of the objects. If it reports a change, that's one
/// undo step and the copy is written back; if not, the map isn't touched.
pub(crate) fn edit_objects<T>(
    document: &mut MapDocument,
    history: &mut EditHistory,
    label: &'static str,
    edit: impl FnOnce(&mut ObjectLayer) -> Option<T>,
) -> Option<T> {
    let before = MapFields::of(document.map());
    let mut objects = document.map().objects.clone();
    let changed = edit(&mut objects)?;
    history.record_map_edit(label, before);
    document.map_mut().objects = objects;
    Some(changed)
}

/// Keys that act on the selected object.
pub(crate) fn object_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<DoodadSettings>,
    mut selected: ResMut<SelectedObject>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
) {
    if keys.just_pressed(KeyCode::Digit1) {
        settings.tool = DoodadTool::Place;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        settings.tool = DoodadTool::Select;
    }
    if keys.just_pressed(KeyCode::Escape) {
        selected.0 = None;
    }
    let Some(id) = selected.0 else {
        return;
    };
    // Mid-drag the recording owns the history.
    if history.is_recording() || document.map().objects.is_foreign() {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let edit = if keys.just_pressed(KeyCode::KeyR) {
        let step = if shift { -ROTATE_STEP } else { ROTATE_STEP };
        Some(("Rotate object", Edit::Turn(step)))
    } else if keys.just_pressed(KeyCode::BracketRight) {
        Some(("Scale object", Edit::Scale(SCALE_STEP)))
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        Some(("Scale object", Edit::Scale(1.0 / SCALE_STEP)))
    } else if keys.just_pressed(KeyCode::End) {
        Some(("Drop to ground", Edit::Drop))
    // Not Backspace: it's what you press to fix a number just typed.
    } else if keys.just_pressed(KeyCode::Delete) {
        Some(("Delete object", Edit::Delete))
    } else {
        None
    };
    if let Some((label, edit)) = edit {
        apply_edit(&mut document, &mut history, id, label, edit);
        if matches!(edit, Edit::Delete) {
            selected.0 = None;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Edit {
    Turn(f32),
    Scale(f32),
    Drop,
    Delete,
}

/// One undo step, recorded only if it changes something.
pub(crate) fn apply_edit(
    document: &mut MapDocument,
    history: &mut EditHistory,
    id: ObjectId,
    label: &'static str,
    edit: Edit,
) {
    let ground = document
        .map()
        .objects
        .get(id)
        .and_then(|o| document.terrain().height_at(o.position.xz()));
    edit_objects(document, history, label, |objects| {
        let changed = match edit {
            Edit::Turn(step) => objects.update(id, |o| o.yaw += step),
            Edit::Scale(factor) => objects.update(id, |o| o.scale *= factor),
            Edit::Drop => objects.update(id, |o| o.position.y = ground.unwrap_or(o.position.y)),
            Edit::Delete => objects.remove(id),
        };
        changed.then_some(())
    });
    history.seal();
}

/// An undo can take the selected object away. So can another map: ids start
/// over in each, so the same id there is some other object.
pub(crate) fn forget_vanished(
    document: Res<MapDocument>,
    mut selected: ResMut<SelectedObject>,
    mut generation: Local<Option<u64>>,
) {
    let current = document.generation();
    let swapped = generation.replace(current).is_some_and(|g| g != current);
    if selected
        .0
        .is_some_and(|id| swapped || document.map().objects.get(id).is_none())
    {
        selected.0 = None;
    }
}
