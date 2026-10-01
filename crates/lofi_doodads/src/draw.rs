//! Rings on the ground: the selected object, the one under the pointer, and
//! where a placement would land.

use bevy::prelude::*;
use lofi_editor::document::MapDocument;
use lofi_editor::pick::GroundCursor;
use lofi_world::{PlacedObject, asset};

use crate::pick::pick_object;
use crate::tool::{DoodadSettings, DoodadTool, SelectedObject};

const SELECTED: Color = Color::srgb(1.0, 0.85, 0.3);
const HOVER: Color = Color::srgba(1.0, 1.0, 1.0, 0.5);
const PLACE: Color = Color::srgb(0.4, 0.9, 1.0);

pub(crate) fn draw_object_rings(
    mut gizmos: Gizmos,
    document: Res<MapDocument>,
    cursor: Res<GroundCursor>,
    settings: Res<DoodadSettings>,
    selected: Res<SelectedObject>,
) {
    let objects = &document.map().objects;
    if let Some(o) = selected.0.and_then(|id| objects.get(id)) {
        ring(&mut gizmos, o, SELECTED);
        let (up, _) = asset(o.asset).bounds(o.scale);
        gizmos.line(o.position, o.position + Vec3::Y * up * 2.2, SELECTED);
    }
    match settings.tool {
        DoodadTool::Select => {
            let Some(ray) = cursor.ray else {
                return;
            };
            let ground = cursor.hit.map(|h| h.distance(ray.origin));
            if let Some(o) = pick_object(objects, ray, ground).and_then(|id| objects.get(id))
                && Some(o.id) != selected.0
            {
                ring(&mut gizmos, o, HOVER);
            }
        }
        DoodadTool::Place => {
            if let Some(hit) = cursor.hit {
                let radius = asset(settings.asset).radius * settings.scale;
                circle(&mut gizmos, hit, radius, PLACE);
            }
        }
    }
}

fn ring(gizmos: &mut Gizmos, o: &PlacedObject, color: Color) {
    circle(gizmos, o.position, asset(o.asset).radius * o.scale, color);
}

fn circle(gizmos: &mut Gizmos, at: Vec3, radius: f32, color: Color) {
    let lift = Vec3::Y * 0.2;
    gizmos.circle(
        Isometry3d::new(
            at + lift,
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        ),
        radius,
        color,
    );
}
