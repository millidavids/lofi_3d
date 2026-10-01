use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use super::dialogs::{NewMapDialog, confirm_discard, new_map_window};
use super::import_prompt::import_prompt;
use super::layout::{ScenePointer, UiLayout, begin_layout, finish_layout, update_scene_pointer};
use super::menu_bar::menu_bar;
use super::recovery_prompt::recovery_prompt;
use super::shortcuts::global_shortcuts;
use super::status_bar::status_bar;
use super::window_title::update_window_title;
use crate::document::{EditHistory, MapDocument, no_prompt_open};
use crate::schedule::EditorSystems;

/// Order of the egui pass. Panels claim screen space in order, so the menu
/// and status bars go first; prompts go last so they sit on top.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiSystems {
    Begin,
    Bars,
    /// A tool's side panels. Chain them if there are several.
    Panels,
    Finish,
    Windows,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiLayout>()
            .init_resource::<ScenePointer>()
            .init_resource::<NewMapDialog>()
            .configure_sets(
                EguiPrimaryContextPass,
                (
                    UiSystems::Begin,
                    UiSystems::Bars,
                    UiSystems::Panels,
                    UiSystems::Finish,
                    UiSystems::Windows,
                )
                    .chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    begin_layout.in_set(UiSystems::Begin),
                    (menu_bar, status_bar).chain().in_set(UiSystems::Bars),
                    (finish_layout, seal_when_idle)
                        .chain()
                        .in_set(UiSystems::Finish),
                    (
                        new_map_window,
                        confirm_discard,
                        recovery_prompt,
                        import_prompt,
                    )
                        .chain()
                        .in_set(UiSystems::Windows),
                ),
            )
            .add_systems(
                Update,
                (
                    (
                        update_scene_pointer,
                        global_shortcuts
                            .run_if(not(egui_wants_any_keyboard_input))
                            .run_if(no_prompt_open),
                    )
                        .in_set(EditorSystems::Input),
                    update_window_title
                        .run_if(resource_changed::<MapDocument>)
                        .in_set(EditorSystems::View),
                ),
            );
    }
}

/// A drag or a typing session in the panels is one undo step; it ends when
/// the mouse is up and nothing has keyboard focus.
fn seal_when_idle(mut contexts: EguiContexts, mut history: ResMut<EditHistory>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let interacting = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
    if !interacting {
        history.seal();
    }
    Ok(())
}
