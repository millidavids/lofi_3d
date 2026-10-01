//! Draws the brush as rings lying on the terrain.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lofi_world::Terrain;

use lofi_editor::document::MapDocument;
use lofi_editor::pick::GroundCursor;

use super::brush::{BrushSettings, Tool};

const SEGMENTS: usize = 72;
/// Lift so the ring doesn't dip under flat-shaded facets.
const LIFT: f32 = 0.15;

pub(super) fn draw_brush(
    mut gizmos: Gizmos,
    cursor: Res<GroundCursor>,
    brush: Res<BrushSettings>,
    document: Res<MapDocument>,
) {
    let Some(hit) = cursor.hit else {
        return;
    };
    let terrain = document.terrain();
    let color = brush.tool.cursor_color();
    if brush.tool == Tool::Select {
        // No brush, just where a drag would start.
        let arm = terrain.cell_size();
        gizmos.line(hit - Vec3::X * arm, hit + Vec3::X * arm, color);
        gizmos.line(hit - Vec3::Z * arm, hit + Vec3::Z * arm, color);
        return;
    }
    ring(&mut gizmos, terrain, hit, brush.radius, color);
    if brush.hardness > 0.05 {
        ring(
            &mut gizmos,
            terrain,
            hit,
            brush.radius * brush.hardness,
            color.with_alpha(0.35),
        );
    }
    gizmos.line(hit, hit + Vec3::Y * brush.radius.min(8.0) * 0.5, color);
}

fn ring(gizmos: &mut Gizmos, terrain: &Terrain, center: Vec3, radius: f32, color: Color) {
    let extent = terrain.extent();
    let points = (0..=SEGMENTS).map(|i| {
        let angle = TAU * i as f32 / SEGMENTS as f32;
        let p = (center.xz() + Vec2::from_angle(angle) * radius).clamp(Vec2::ZERO, extent);
        let y = terrain.height_at(p).unwrap_or(center.y);
        Vec3::new(p.x, y + LIFT, p.y)
    });
    gizmos.linestrip(points, color);
}
