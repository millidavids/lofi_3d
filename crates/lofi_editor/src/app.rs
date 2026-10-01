//! The window, engine and shared plugins every tool starts from.

use bevy::pbr::wireframe::WireframePlugin;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::window::PresentMode;
use bevy_egui::EguiPlugin;

use crate::launch::LaunchOptions;
use crate::schedule::EditorSystems;
use crate::view::{Shading, ViewSettings};

/// An app with the shell in place. Add the tool's plugins and `run()` it.
pub fn editor_app(title: &str, options: LaunchOptions) -> App {
    let view = ViewSettings {
        shading: if options.flat {
            Shading::Flat
        } else {
            Shading::Smooth
        },
        fog: !options.measure,
        overlays: options.overlays,
        ..default()
    };
    // Measuring needs frames as fast as they come, not paced to the display.
    let present_mode = if options.measure {
        PresentMode::AutoNoVsync
    } else {
        PresentMode::AutoVsync
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: title.into(),
            resolution: (1600, 900).into(),
            present_mode,
            ..default()
        }),
        // Closing asks about unsaved changes first; see `document::close_guard`.
        close_when_requested: false,
        ..default()
    }))
    .add_plugins((EguiPlugin::default(), WireframePlugin::default()));
    if options.measure {
        app.add_plugins(RenderDiagnosticsPlugin);
    }
    app.insert_resource(options)
        .insert_resource(view)
        .configure_sets(Update, EditorSystems::ORDER.chain())
        .add_plugins((
            crate::document::DocumentPlugin,
            crate::view::ViewPlugin,
            crate::camera::CameraPlugin,
            crate::pick::PickPlugin,
            crate::ui::UiPlugin,
            crate::capture::CapturePlugin,
        ));
    app
}
