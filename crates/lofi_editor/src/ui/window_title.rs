use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::document::MapDocument;

pub(super) fn update_window_title(
    document: Res<MapDocument>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    let unsaved = if document.is_dirty() { " *" } else { "" };
    let title = format!("{}{unsaved} — lofi terrain", document.title());
    if window.title != title {
        window.title = title;
    }
}
