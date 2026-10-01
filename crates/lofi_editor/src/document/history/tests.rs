use bevy::math::Vec3;
use lofi_world::{
    AssetGuid, CellFlags, GridSize, MaterialId, Pattern, Triangulation, VertexPaint, VertexRect,
    WorldMap,
};

use super::map_fields::MapFields;
use super::stack::{EditHistory, Reverted};

fn map() -> WorldMap {
    WorldMap::flat("test", GridSize::new(3, 3).expect("valid size"), 2.0, 0.0)
}

const CORNER: VertexRect = VertexRect {
    min_x: 30,
    min_z: 30,
    max_x: 34,
    max_z: 34,
};

/// Raises a square centred on a four-chunk corner.
fn stroke(history: &mut EditHistory, map: &mut WorldMap, amount: f32) {
    history.begin("Raise");
    history.capture(&map.terrain, CORNER);
    for (x, z) in CORNER.iter() {
        let h = map.terrain.height(x, z);
        map.terrain.set_height(x, z, h + amount);
    }
    assert!(history.commit(map));
}

#[test]
fn undo_and_redo_restore_exact_states() {
    let mut m = map();
    let mut history = EditHistory::default();
    let original = m.clone();
    stroke(&mut history, &mut m, 5.0);
    let after_one = m.clone();
    stroke(&mut history, &mut m, 2.0);
    let after_two = m.clone();

    history.undo(&mut m).expect("undo");
    assert_eq!(m, after_one);
    history.undo(&mut m).expect("undo");
    assert_eq!(m, original);
    assert!(history.undo(&mut m).is_none());

    history.redo(&mut m).expect("redo");
    history.redo(&mut m).expect("redo");
    assert_eq!(m, after_two);
}

#[test]
fn undo_reports_the_chunks_it_touched() {
    let mut m = map();
    let mut history = EditHistory::default();
    stroke(&mut history, &mut m, 1.0);
    let reverted = history.undo(&mut m).expect("undo");
    let rect = reverted.heights.expect("heights changed");
    assert_eq!(
        (rect.min_x, rect.min_z, rect.max_x, rect.max_z),
        (0, 0, 64, 64)
    );
    assert_eq!(reverted.terrain(), Some(rect));
    assert!(reverted.cells.is_none() && reverted.surface.is_none());
    assert!(!reverted.map);
}

#[test]
fn undo_reports_only_what_changed() {
    let mut m = map();
    let mut history = EditHistory::default();
    history.begin("Paint");
    history.capture(&m.terrain, CORNER);
    m.terrain.set_tint(31, 31, [9, 9, 9]);
    assert!(history.commit(&m));
    let reverted = history.undo(&mut m).expect("undo");
    assert!(reverted.surface.is_some(), "tint is surface");
    assert!(
        reverted.heights.is_none() && reverted.cells.is_none(),
        "no AO rebake for a tint"
    );
}

#[test]
fn sculpting_keeps_only_heights() {
    let mut m = map();
    let mut history = EditHistory::default();
    stroke(&mut history, &mut m, 1.0);
    // Four chunks' heights, no paint: copy-on-write left the paint shared.
    assert_eq!(history.bytes, 4 * 32 * 32 * size_of::<f32>());
}

#[test]
fn painting_a_border_records_both_chunks() {
    let mut m = map();
    let mut history = EditHistory::default();
    history.begin("Paint");
    let border = VertexRect {
        min_x: 32,
        min_z: 5,
        max_x: 32,
        max_z: 5,
    };
    history.capture(&m.terrain, border);
    m.terrain
        .set_vertex_paint(32, 5, VertexPaint::solid(MaterialId(1)))
        .expect("room");
    assert!(history.commit(&m));
    assert_eq!(history.undo.last().map(|e| e.chunks.len()), Some(2));
    let original = map();
    history.undo(&mut m);
    assert_eq!(m, original);
}

#[test]
fn new_edit_clears_redo() {
    let mut m = map();
    let mut history = EditHistory::default();
    stroke(&mut history, &mut m, 1.0);
    history.undo(&mut m);
    assert!(history.redo_label().is_some());
    stroke(&mut history, &mut m, 1.0);
    assert!(history.redo_label().is_none());
}

#[test]
fn untouched_chunks_are_not_kept() {
    let mut m = map();
    let mut history = EditHistory::default();
    history.begin("Smooth");
    history.capture(&m.terrain, CORNER);
    assert!(!history.commit(&m), "nothing changed");
    history.begin("Raise");
    history.capture(&m.terrain, CORNER);
    m.terrain.set_height(33, 33, 3.0);
    assert!(history.commit(&m));
    assert_eq!(history.undo.last().map(|e| e.chunks.len()), Some(1));
}

#[test]
fn undo_is_refused_mid_stroke() {
    let mut m = map();
    let mut history = EditHistory::default();
    stroke(&mut history, &mut m, 1.0);
    history.begin("Raise");
    assert!(history.undo(&mut m).is_none());
    assert!(history.undo_label().is_none());
}

#[test]
fn map_edits_undo_and_merge() {
    let mut m = map();
    let mut history = EditHistory::default();
    let original = m.clone();
    for level in [1.0, 2.0, 3.0] {
        history.record_map_edit("Edit water", MapFields::of(&m));
        m.water.level = level;
    }
    history.record_map_edit("Rename map", MapFields::of(&m));
    m.name = "Elwynn".to_owned();

    let reverted = history.undo(&mut m).expect("undo");
    assert_eq!(
        reverted,
        Reverted {
            map: true,
            ..Reverted::default()
        }
    );
    assert_eq!(m.name, "test");
    history.undo(&mut m);
    assert_eq!(m, original, "three water drags were one step");
    history.redo(&mut m);
    assert_eq!(m.water.level, 3.0);
}

#[test]
fn cell_and_tint_edits_undo() {
    let mut m = map();
    let original = m.clone();
    let mut history = EditHistory::default();
    history.begin("Hole");
    // A cell edit captures the cell's corners, like a brush would.
    history.capture(
        &m.terrain,
        VertexRect {
            min_x: 31,
            min_z: 3,
            max_x: 32,
            max_z: 4,
        },
    );
    m.terrain
        .set_cell(31, 3, CellFlags::default().with_hole(true));
    m.terrain.set_tint(32, 4, [1, 2, 3]);
    assert!(history.commit(&m));
    let edited = m.clone();

    history.undo(&mut m);
    assert_eq!(m, original);
    history.redo(&mut m);
    assert_eq!(m, edited);
}

#[test]
fn undoing_a_placement_never_frees_its_id() {
    let mut m = map();
    let mut history = EditHistory::default();
    let place = |m: &mut WorldMap, history: &mut EditHistory| {
        history.record_map_edit("Place object", MapFields::of(m));
        history.seal();
        m.objects
            .add(AssetGuid(5), Vec3::ONE, 0.0, 1.0)
            .expect("room")
    };
    let first = place(&mut m, &mut history);
    history.undo(&mut m).expect("undo");
    assert!(m.objects.is_empty());
    let second = place(&mut m, &mut history);
    assert!(second > first, "the undone placement's id stays spent");
    history.undo(&mut m).expect("undo");
    history.redo(&mut m).expect("redo");
    assert_eq!(m.objects.get(second).map(|o| o.id), Some(second));
}

#[test]
fn triangulation_is_a_map_setting() {
    let mut m = map();
    let mut history = EditHistory::default();
    assert_eq!(m.terrain.triangulation(), Triangulation::FollowTerrain);
    history.record_map_edit("Diagonals", MapFields::of(&m));
    m.terrain.set_triangulation(Triangulation::Checkerboard);

    let reverted = history.undo(&mut m).expect("undo");
    assert!(reverted.map);
    assert_eq!(m.terrain.triangulation(), Triangulation::FollowTerrain);
    history.redo(&mut m);
    assert_eq!(m.terrain.triangulation(), Triangulation::Checkerboard);
}

#[test]
fn sealing_splits_map_edits() {
    let mut m = map();
    let mut history = EditHistory::default();
    history.record_map_edit("Water", MapFields::of(&m));
    m.water.level = 5.0;
    // Saved here: undo must be able to land on 5.
    history.seal();
    history.record_map_edit("Water", MapFields::of(&m));
    m.water.level = 6.0;

    history.undo(&mut m);
    assert_eq!(m.water.level, 5.0);
}

#[test]
fn one_edit_can_change_terrain_and_settings() {
    let mut m = map();
    let original = m.clone();
    let mut history = EditHistory::default();
    let clay = m
        .materials
        .add("Clay", [0.6, 0.4, 0.3], Pattern::Plain)
        .expect("room");
    m.terrain
        .set_vertex_paint(4, 4, VertexPaint::solid(clay))
        .expect("room");
    let painted = m.clone();

    // Delete clay, repainting it as the base material.
    history.begin("Delete material");
    history.capture(&m.terrain, m.terrain.size().full_rect());
    history.capture_fields(&m);
    let base = m.materials.base();
    m.terrain.replace_material(clay, base);
    m.materials.remove(clay);
    assert!(history.commit(&m));

    history.undo(&mut m);
    assert_eq!(m, painted);
    history.redo(&mut m);
    assert!(m.materials.get(clay).is_none());
    assert_ne!(m, original, "the table's next id moved on");
}
