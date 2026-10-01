use std::f32::consts::TAU;
use std::time::Duration;

use bevy::prelude::*;
use lofi_editor::document::{EditHistory, MapDocument, StatusLine};
use lofi_editor::pick::GroundCursor;
use lofi_editor::ui::ScenePointer;
use lofi_world::{BUILTIN_ASSETS, GridSize, ObjectId, WorldMap};

use super::*;

/// The mouse system on a flat 2×2-chunk map, the cursor on (64, 0, 64).
fn app() -> App {
    let size = GridSize::new(2, 2).expect("valid size");
    let mut app = App::new();
    let ray = Ray3d::new(Vec3::new(64.0, 50.0, 64.0), Dir3::NEG_Y);
    app.insert_resource(MapDocument::new(WorldMap::flat("t", size, 2.0, 0.0)))
        .insert_resource(ScenePointer {
            position: Some(Vec2::new(400.0, 300.0)),
        })
        .insert_resource(GroundCursor {
            ray: Some(ray),
            hit: Some(Vec3::new(64.0, 0.0, 64.0)),
        })
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<DoodadSettings>()
        .init_resource::<SelectedObject>()
        .init_resource::<ObjectDrag>()
        .init_resource::<EditHistory>()
        .init_resource::<StatusLine>()
        .init_resource::<Time>()
        .add_systems(Update, run_mouse);
    app
}

fn click(app: &mut App, down: bool) {
    let world = app.world_mut();
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_millis(16));
    let mut mouse = world.resource_mut::<ButtonInput<MouseButton>>();
    mouse.clear();
    if down {
        mouse.press(MouseButton::Left);
    } else {
        mouse.release(MouseButton::Left);
    }
    app.update();
}

fn undo(app: &mut App) {
    app.world_mut()
        .resource_scope(|world, mut history: Mut<EditHistory>| {
            let mut document = world.resource_mut::<MapDocument>();
            history.undo(document.map_mut());
        });
}

/// Moves the pointer along x: the ground point, and the window pixel that
/// goes with it.
fn point_at(app: &mut App, x: f32) {
    let world = app.world_mut();
    world.resource_mut::<GroundCursor>().hit = Some(Vec3::new(x, 0.0, 64.0));
    world.resource_mut::<ScenePointer>().position = Some(Vec2::new(400.0 + x * 10.0, 300.0));
}

fn selected(app: &App) -> Option<ObjectId> {
    app.world().resource::<SelectedObject>().0
}

fn placed(app: &App, id: ObjectId) -> Vec3 {
    let objects = &app.world().resource::<MapDocument>().map().objects;
    objects.get(id).expect("there").position
}

/// Clicks once with the Place tool.
fn place(app: &mut App) -> ObjectId {
    click(app, true);
    click(app, false);
    selected(app).expect("placed and selected")
}

/// Places one object, raises it 3 m, and switches to the Select tool.
fn raised_object(app: &mut App) -> ObjectId {
    let id = place(app);
    let world = app.world_mut();
    world
        .resource_mut::<MapDocument>()
        .map_mut()
        .objects
        .update(id, |o| o.position.y = 3.0);
    world.resource_mut::<DoodadSettings>().tool = DoodadTool::Select;
    id
}

#[test]
fn placing_selects_and_undoes() {
    let mut app = app();
    let id = place(&mut app);
    assert_eq!(placed(&app, id), Vec3::new(64.0, 0.0, 64.0));
    undo(&mut app);
    let objects = &app.world().resource::<MapDocument>().map().objects;
    assert!(objects.is_empty());
}

#[test]
fn a_click_selects_without_moving() {
    let mut app = app();
    let id = raised_object(&mut app);
    let revision = app.world().resource::<MapDocument>().revision();
    click(&mut app, true);
    // Held a few frames, the pointer a pixel or two off.
    app.world_mut().resource_mut::<ScenePointer>().position = Some(Vec2::new(402.0, 301.0));
    app.world_mut().resource_mut::<GroundCursor>().hit = Some(Vec3::new(64.2, 0.0, 64.1));
    click(&mut app, true);
    click(&mut app, false);

    assert_eq!(selected(&app), Some(id));
    assert_eq!(placed(&app, id), Vec3::new(64.0, 3.0, 64.0));
    let world = app.world();
    assert_eq!(world.resource::<MapDocument>().revision(), revision);
    assert_eq!(
        world.resource::<EditHistory>().undo_label(),
        Some("Place object"),
        "no move step"
    );
}

#[test]
fn dragging_moves_along_the_ground_as_one_step() {
    let mut app = app();
    let id = place(&mut app);
    app.world_mut().resource_mut::<DoodadSettings>().tool = DoodadTool::Select;

    click(&mut app, true);
    assert!(app.world().resource::<EditHistory>().is_recording());
    for x in [70.0, 80.0, 90.0] {
        point_at(&mut app, x);
        click(&mut app, true);
    }
    click(&mut app, false);
    assert_eq!(placed(&app, id).x, 90.0);
    assert_eq!(
        app.world().resource::<EditHistory>().undo_label(),
        Some("Move object")
    );
    undo(&mut app);
    assert_eq!(placed(&app, id).x, 64.0, "one step");
}

#[test]
fn a_dragged_object_keeps_its_height_above_the_ground() {
    let mut app = app();
    let id = raised_object(&mut app);
    click(&mut app, true);
    point_at(&mut app, 80.0);
    click(&mut app, true);
    click(&mut app, false);
    assert_eq!(placed(&app, id), Vec3::new(80.0, 3.0, 64.0));
}

#[test]
fn clicking_nothing_deselects() {
    let mut app = app();
    place(&mut app);
    app.world_mut().resource_mut::<DoodadSettings>().tool = DoodadTool::Select;
    let ray = Ray3d::new(Vec3::new(10.0, 50.0, 10.0), Dir3::NEG_Y);
    {
        let mut cursor = app.world_mut().resource_mut::<GroundCursor>();
        cursor.ray = Some(ray);
        cursor.hit = Some(Vec3::new(10.0, 0.0, 10.0));
    }
    click(&mut app, true);
    click(&mut app, false);
    assert!(selected(&app).is_none());
    assert_eq!(
        app.world().resource::<EditHistory>().undo_label(),
        Some("Place object"),
        "no empty move step"
    );
}

#[test]
fn each_turn_is_its_own_step() {
    let mut app = app();
    let id = place(&mut app);
    let world = app.world_mut();
    world.resource_scope(|world, mut history: Mut<EditHistory>| {
        let mut document = world.resource_mut::<MapDocument>();
        let turn = Edit::Turn(1.0);
        apply_edit(&mut document, &mut history, id, "Rotate object", turn);
        apply_edit(&mut document, &mut history, id, "Rotate object", turn);
    });
    let yaw = |app: &App| {
        let objects = &app.world().resource::<MapDocument>().map().objects;
        objects.get(id).expect("there").yaw
    };
    let turned = yaw(&app);
    undo(&mut app);
    let back = (turned - 1.0).rem_euclid(TAU);
    assert!((yaw(&app) - back).abs() < 1e-4, "one turn back");
}

#[test]
fn an_edit_that_changes_nothing_is_no_step() {
    let mut app = app();
    let id = place(&mut app);
    let world = app.world_mut();
    world.resource_scope(|world, mut history: Mut<EditHistory>| {
        let mut document = world.resource_mut::<MapDocument>();
        let grow = Edit::Scale(1000.0);
        apply_edit(&mut document, &mut history, id, "Scale object", grow);
        let revision = document.revision();
        // Already as big as it gets, and already on the ground.
        apply_edit(&mut document, &mut history, id, "Scale object", grow);
        apply_edit(&mut document, &mut history, id, "Drop", Edit::Drop);
        assert_eq!(document.revision(), revision, "map left clean");
        assert!(history.undo(document.map_mut()).is_some());
        assert_eq!(history.undo_label(), Some("Place object"));
    });
}

#[test]
fn another_map_clears_the_selection() {
    let mut app = app();
    app.add_systems(Update, forget_vanished.after(run_mouse));
    let id = place(&mut app);

    // Its first object has the same id.
    let mut other = WorldMap::flat("other", GridSize::new(1, 1).expect("valid size"), 2.0, 0.0);
    let same = other
        .objects
        .add(BUILTIN_ASSETS[0].guid, Vec3::ZERO, 0.0, 1.0);
    assert_eq!(same, Some(id));
    app.world_mut()
        .resource_mut::<MapDocument>()
        .replace(other, None, default());
    click(&mut app, false);
    assert!(selected(&app).is_none());
}
