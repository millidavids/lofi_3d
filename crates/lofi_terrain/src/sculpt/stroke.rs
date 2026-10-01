//! A stroke: press, drag and release, recorded as one undo step.

use bevy::prelude::*;
use lofi_editor::document::{EditHistory, MapDocument, StatusLine, TerrainChanges};
use lofi_editor::pick::GroundCursor;
use lofi_editor::ui::ScenePointer;

use super::brush::{BrushSettings, Tool};
use super::ops::{Dab, DabOutcome, apply_dab, cell_mode_changes_shape, footprint};
use crate::select::Selection;

#[derive(Resource, Default)]
pub struct ActiveStroke(Option<StrokeState>);

struct StrokeState {
    flatten_height: f32,
    dabs: u32,
    /// A full chunk was reported this stroke; once is enough.
    warned: bool,
}

pub(super) fn run_stroke(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    pointer: Res<ScenePointer>,
    cursor: Res<GroundCursor>,
    brush: Res<BrushSettings>,
    selection: Res<Selection>,
    mut stroke: ResMut<ActiveStroke>,
    mut document: ResMut<MapDocument>,
    mut history: ResMut<EditHistory>,
    mut changes: ResMut<TerrainChanges>,
    mut status: ResMut<StatusLine>,
) {
    // Selecting edits no terrain, so it's no stroke and no undo step.
    let starting = mouse.just_pressed(MouseButton::Left)
        && !mouse.pressed(MouseButton::Right)
        && pointer.position.is_some()
        && brush.tool != Tool::Select;
    if starting
        && stroke.0.is_none()
        && let Some(hit) = cursor.hit
    {
        history.begin(brush.tool.label());
        stroke.0 = Some(StrokeState {
            flatten_height: hit.y,
            dabs: 0,
            warned: false,
        });
    }

    let Some(state) = &mut stroke.0 else {
        return;
    };
    // Released, or the history was cleared under us by opening another map.
    if !mouse.pressed(MouseButton::Left) || !history.is_recording() {
        history.commit(document.map());
        stroke.0 = None;
        return;
    }
    // Over a panel, or off the map: pause, don't end.
    let Some(hit) = cursor.hit else {
        return;
    };
    let center = hit.xz();
    let Some(rect) = footprint(document.terrain(), &brush, center) else {
        return;
    };

    state.dabs = state.dabs.wrapping_add(1);
    let dab = Dab {
        center,
        invert: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        flatten_height: state.flatten_height,
        dt: time.delta_secs(),
        seed: state.dabs,
        base: document.map().materials.base(),
    };
    history.capture(document.terrain(), rect);
    // Flatten on flat ground or paint on painted ground changes nothing, and
    // mustn't mark the map unsaved.
    let mut outcome = DabOutcome::default();
    if document.edit_terrain(|terrain| {
        outcome = apply_dab(terrain, &brush, &selection, &dab, rect);
        outcome.changed
    }) {
        match brush.tool {
            tool if tool.sculpts() => changes.mark_heights(rect),
            Tool::Cells if cell_mode_changes_shape(brush.cell_mode) => changes.mark_shape(rect),
            // Clutter and walkability flags draw as an overlay, not in the mesh.
            Tool::Cells => {}
            _ => changes.mark_surface(rect),
        }
    }
    if let Some(chunk) = outcome.refused
        && !state.warned
    {
        state.warned = true;
        status.error(format!(
            "Chunk {}, {} already blends four materials. Paint with one of those there, \
             or cover one of them with another.",
            chunk.x, chunk.z
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lofi_world::{GridSize, WorldMap};

    use super::*;

    /// Just the stroke system and what it reads, with the brush aimed at vertex (32, 32).
    fn app() -> App {
        let size = GridSize::new(2, 2).expect("valid size");
        let mut app = App::new();
        app.insert_resource(MapDocument::new(WorldMap::flat("test", size, 2.0, 0.0)))
            .insert_resource(ScenePointer {
                position: Some(Vec2::new(400.0, 300.0)),
            })
            .insert_resource(GroundCursor {
                ray: None,
                hit: Some(Vec3::new(64.0, 0.0, 64.0)),
            })
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .init_resource::<BrushSettings>()
            .init_resource::<Selection>()
            .init_resource::<ActiveStroke>()
            .init_resource::<EditHistory>()
            .init_resource::<TerrainChanges>()
            .init_resource::<StatusLine>()
            .add_systems(Update, run_stroke);
        app
    }

    fn frame(app: &mut App, left_down: bool) {
        let world = app.world_mut();
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(16));
        let mut mouse = world.resource_mut::<ButtonInput<MouseButton>>();
        mouse.clear();
        if left_down {
            mouse.press(MouseButton::Left);
        } else {
            mouse.release(MouseButton::Left);
        }
        app.update();
    }

    fn center_height(app: &App) -> f32 {
        app.world()
            .resource::<MapDocument>()
            .terrain()
            .height(32, 32)
    }

    #[test]
    fn a_stroke_is_one_undo_step() {
        let mut app = app();
        for _ in 0..10 {
            frame(&mut app, true);
        }
        assert!(
            app.world().resource::<EditHistory>().undo_label().is_none(),
            "still recording"
        );
        frame(&mut app, false);

        assert!(center_height(&app) > 0.0);
        assert!(app.world().resource::<MapDocument>().is_dirty());
        assert_eq!(
            app.world().resource::<EditHistory>().undo_label(),
            Some("Raise / Lower")
        );

        let world = app.world_mut();
        world.resource_scope(|world, mut history: Mut<EditHistory>| {
            let mut document = world.resource_mut::<MapDocument>();
            history.undo(document.map_mut());
        });
        assert_eq!(center_height(&app), 0.0);
    }

    #[test]
    fn pressing_off_the_terrain_starts_nothing() {
        let mut app = app();
        app.world_mut().resource_mut::<GroundCursor>().hit = None;
        frame(&mut app, true);
        app.world_mut().resource_mut::<GroundCursor>().hit = Some(Vec3::new(64.0, 0.0, 64.0));
        frame(&mut app, true);
        frame(&mut app, false);
        assert_eq!(center_height(&app), 0.0);
        assert!(app.world().resource::<EditHistory>().undo_label().is_none());
    }

    #[test]
    fn a_stroke_that_changes_nothing_leaves_the_map_clean() {
        let mut app = app();
        app.world_mut().resource_mut::<BrushSettings>().tool = super::super::brush::Tool::Flatten;
        for _ in 0..5 {
            frame(&mut app, true);
        }
        frame(&mut app, false);
        assert!(!app.world().resource::<MapDocument>().is_dirty());
        assert!(app.world().resource::<EditHistory>().undo_label().is_none());
    }

    #[test]
    fn cutting_holes_marks_the_shape_and_undoes() {
        let mut app = app();
        app.world_mut().resource_mut::<BrushSettings>().tool = Tool::Cells;
        frame(&mut app, true);
        let pending = app.world_mut().resource_mut::<TerrainChanges>().take();
        assert!(pending.shape.is_some() && pending.heights.is_none());
        frame(&mut app, false);
        let hole = |app: &App| {
            app.world()
                .resource::<MapDocument>()
                .terrain()
                .cell(32, 32)
                .hole()
        };
        assert!(hole(&app));
        let world = app.world_mut();
        world.resource_scope(|world, mut history: Mut<EditHistory>| {
            let mut document = world.resource_mut::<MapDocument>();
            history.undo(document.map_mut());
        });
        assert!(!hole(&app));
    }

    #[test]
    fn tint_marks_only_the_surface() {
        let mut app = app();
        app.world_mut().resource_mut::<BrushSettings>().tool = Tool::Tint;
        frame(&mut app, true);
        let pending = app.world_mut().resource_mut::<TerrainChanges>().take();
        assert!(pending.surface.is_some() && pending.heights.is_none() && pending.shape.is_none());
        frame(&mut app, false);
        let tint = app.world().resource::<MapDocument>().terrain().tint(32, 32);
        assert_ne!(tint, lofi_world::NEUTRAL_TINT);
    }

    #[test]
    fn selecting_is_not_a_stroke() {
        let mut app = app();
        app.world_mut().resource_mut::<BrushSettings>().tool = Tool::Select;
        frame(&mut app, true);
        frame(&mut app, false);
        assert!(app.world().resource::<EditHistory>().undo_label().is_none());
        assert!(!app.world().resource::<MapDocument>().is_dirty());
    }
}
