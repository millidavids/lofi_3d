//! The Select tool: drag out a rectangle or lasso on the ground.
//!
//! Plain drag replaces the selection, Shift adds, Ctrl subtracts. A click
//! without a drag clears it. Esc cancels a drag, then clears; so does Ctrl+D.

use bevy::prelude::*;
use lofi_editor::document::MapDocument;
use lofi_editor::pick::GroundCursor;
use lofi_editor::ui::ScenePointer;

use super::selection::{MAX_LASSO_POINTS, Op, Selection, Shape};
use crate::sculpt::{BrushSettings, Tool};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectMode {
    #[default]
    Rectangle,
    Lasso,
}

impl SelectMode {
    pub const ALL: [Self; 2] = [Self::Rectangle, Self::Lasso];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Lasso => "Lasso",
        }
    }
}

#[derive(Resource, Default)]
pub struct SelectSettings {
    pub mode: SelectMode,
}

/// A drag in progress, in world XZ.
#[derive(Resource, Default)]
pub struct SelectDrag(pub(super) Option<Drag>);

pub(super) struct Drag {
    pub start: Vec2,
    pub current: Vec2,
    pub points: Vec<Vec2>,
}

impl SelectDrag {
    pub(super) fn shape(&self, mode: SelectMode) -> Option<Shape> {
        let drag = self.0.as_ref()?;
        Some(match mode {
            SelectMode::Rectangle => Shape::rect(drag.start, drag.current),
            SelectMode::Lasso => Shape::Lasso(drag.points.clone()),
        })
    }
}

/// Another map, another selection.
pub(super) fn forget_on_new_map(
    document: Res<MapDocument>,
    mut selection: ResMut<Selection>,
    mut drag: ResMut<SelectDrag>,
) {
    if selection.generation != Some(document.generation()) {
        selection.clear();
        selection.generation = Some(document.generation());
        drag.0 = None;
    }
}

pub(super) fn run_select(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointer: Res<ScenePointer>,
    cursor: Res<GroundCursor>,
    brush: Res<BrushSettings>,
    settings: Res<SelectSettings>,
    document: Res<MapDocument>,
    mut drag: ResMut<SelectDrag>,
    mut selection: ResMut<Selection>,
) {
    if brush.tool != Tool::Select {
        drag.0 = None;
        return;
    }
    let starting = mouse.just_pressed(MouseButton::Left)
        && !mouse.pressed(MouseButton::Right)
        && pointer.position.is_some();
    if starting && let Some(hit) = cursor.hit {
        drag.0 = Some(Drag {
            start: hit.xz(),
            current: hit.xz(),
            points: vec![hit.xz()],
        });
    }
    let terrain = document.terrain();
    let cell = terrain.cell_size();
    let Some(current) = &mut drag.0 else {
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        if let Some(hit) = cursor.hit {
            let p = hit.xz();
            current.current = p;
            let moved = current
                .points
                .last()
                .is_none_or(|last| last.distance(p) >= cell);
            if moved && current.points.len() < MAX_LASSO_POINTS {
                current.points.push(p);
            }
        }
        return;
    }

    let Some(shape) = drag.shape(settings.mode) else {
        return;
    };
    drag.0 = None;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if !shape.has_area(cell) {
        // A click: plain clears, a modified one was probably a slip.
        if !shift && !ctrl {
            selection.clear();
        }
        return;
    }
    let op = if ctrl { Op::Subtract } else { Op::Add };
    selection.apply(shape, op, !shift && !ctrl, terrain);
}

/// Esc cancels a drag, or else clears the selection; Ctrl+D clears it.
pub(super) fn select_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut drag: ResMut<SelectDrag>,
    mut selection: ResMut<Selection>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if keys.just_pressed(KeyCode::Escape) {
        if drag.0.is_some() {
            drag.0 = None;
        } else {
            selection.clear();
        }
    }
    if ctrl && keys.just_pressed(KeyCode::KeyD) {
        drag.0 = None;
        selection.clear();
    }
}
