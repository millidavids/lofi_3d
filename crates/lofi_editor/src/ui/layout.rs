//! Panel layout and where the pointer may touch the 3D view.
//!
//! egui 0.36 panels live inside a `Ui`, and through bevy_egui they don't count
//! as "pointer over egui". So each panel system claims its edge from
//! [`UiLayout`], and whatever is left is the 3D view.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, egui};

#[derive(Resource)]
pub struct UiLayout {
    /// Unclaimed space this frame, in egui points.
    free: egui::Rect,
    /// The 3D view from last frame's layout, in logical pixels.
    scene: Option<Rect>,
}

impl Default for UiLayout {
    fn default() -> Self {
        Self {
            free: egui::Rect::NOTHING,
            scene: None,
        }
    }
}

impl UiLayout {
    /// A root `Ui` over the unclaimed space. Show one panel in it, then [`Self::claim`].
    pub fn root(&self, ctx: &egui::Context, id: &'static str) -> egui::Ui {
        egui::Ui::new(
            ctx.clone(),
            egui::Id::new(id),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(self.free),
        )
    }

    /// Takes whatever the panel in `root` used.
    pub fn claim(&mut self, root: &egui::Ui) {
        self.free = root.available_rect_before_wrap();
    }
}

/// The pointer, while it's over the 3D view and not over any UI.
#[derive(Resource, Default)]
pub struct ScenePointer {
    pub position: Option<Vec2>,
}

pub(super) fn begin_layout(mut contexts: EguiContexts, mut layout: ResMut<UiLayout>) -> Result {
    layout.free = contexts.ctx_mut()?.viewport_rect();
    Ok(())
}

pub(super) fn finish_layout(mut contexts: EguiContexts, mut layout: ResMut<UiLayout>) -> Result {
    let zoom = contexts.ctx_mut()?.zoom_factor();
    let free = layout.free;
    layout.scene = free.is_positive().then(|| {
        Rect::new(
            free.min.x * zoom,
            free.min.y * zoom,
            free.max.x * zoom,
            free.max.y * zoom,
        )
    });
    Ok(())
}

pub(super) fn update_scene_pointer(
    window: Single<&Window, With<PrimaryWindow>>,
    layout: Res<UiLayout>,
    egui_input: Res<EguiWantsInput>,
    mut pointer: ResMut<ScenePointer>,
) {
    // Windows, menus and drags on widgets are still egui's to report.
    let blocked = egui_input.wants_any_pointer_input();
    pointer.position = window
        .cursor_position()
        .filter(|p| !blocked && layout.scene.is_some_and(|scene| scene.contains(*p)));
}
