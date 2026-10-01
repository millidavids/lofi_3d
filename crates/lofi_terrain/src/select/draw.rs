//! Draws the selection's outlines on the ground.

use bevy::prelude::*;
use lofi_editor::document::MapDocument;
use lofi_world::Terrain;

use super::selection::{Op, Selection, Shape};
use super::tool::{SelectDrag, SelectSettings};

const ADD_COLOR: Color = Color::srgb(0.4, 0.9, 1.0);
const SUBTRACT_COLOR: Color = Color::srgb(1.0, 0.45, 0.4);
const DRAG_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
/// Lift so outlines don't sink between samples.
const LIFT: f32 = 0.25;
/// Points per outline at most; long edges are sampled more coarsely.
const MAX_SAMPLES: usize = 2048;

pub(super) fn draw_selection(
    selection: Res<Selection>,
    drag: Res<SelectDrag>,
    settings: Res<SelectSettings>,
    document: Res<MapDocument>,
    mut gizmos: Gizmos,
) {
    let terrain = document.terrain();
    for (shape, op) in selection.shapes() {
        let color = match op {
            Op::Add => ADD_COLOR,
            Op::Subtract => SUBTRACT_COLOR,
        };
        outline(&mut gizmos, terrain, shape, color);
    }
    if let Some(shape) = drag.shape(settings.mode) {
        outline(&mut gizmos, terrain, &shape, DRAG_COLOR);
    }
}

fn outline(gizmos: &mut Gizmos, terrain: &Terrain, shape: &Shape, color: Color) {
    let corners: Vec<Vec2> = match shape {
        Shape::Rect { min, max } => {
            vec![*min, Vec2::new(max.x, min.y), *max, Vec2::new(min.x, max.y)]
        }
        Shape::Lasso(points) => points.clone(),
    };
    if corners.len() < 2 {
        return;
    }
    let perimeter: f32 = corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .map(|(a, b)| a.distance(*b))
        .sum();
    // About one sample per cell, within the budget.
    let spacing = terrain.cell_size().max(perimeter / MAX_SAMPLES as f32);
    let extent = terrain.extent();
    let mut points = Vec::new();
    for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)) {
        let steps = (a.distance(*b) / spacing).ceil().max(1.0) as usize;
        for i in 0..steps {
            points.push(a.lerp(*b, i as f32 / steps as f32));
        }
    }
    points.push(corners[0]);
    gizmos.linestrip(
        points.into_iter().map(|p| {
            let p = p.clamp(Vec2::ZERO, extent);
            let y = terrain.height_at(p).unwrap_or(0.0);
            Vec3::new(p.x, y + LIFT, p.y)
        }),
        color,
    );
}
