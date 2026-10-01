//! Frame order for `Update`. Each stage reads what the one before it wrote.

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorSystems {
    /// Where the pointer is and what the keys mean.
    Input,
    /// Camera movement.
    Camera,
    /// What terrain is under the cursor.
    Cursor,
    /// Brush strokes. Before `Edit`, so a stroke released this frame is
    /// committed before an undo in the same frame looks for it.
    Stroke,
    /// Other changes to the document: undo, file actions, autosave.
    Edit,
    /// Bring meshes and scene objects in line with the document.
    View,
}

impl EditorSystems {
    pub const ORDER: (Self, Self, Self, Self, Self, Self) = (
        Self::Input,
        Self::Camera,
        Self::Cursor,
        Self::Stroke,
        Self::Edit,
        Self::View,
    );
}
