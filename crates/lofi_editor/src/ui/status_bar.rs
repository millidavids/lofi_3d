//! Bottom bar: last message on the left, cursor and map facts on the right.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use lofi_world::{Ground, GroundAt, Terrain};

use super::layout::UiLayout;
use crate::document::{MapDocument, StatusLine};
use crate::pick::GroundCursor;

pub(super) fn status_bar(
    mut contexts: EguiContexts,
    mut layout: ResMut<UiLayout>,
    document: Res<MapDocument>,
    cursor: Res<GroundCursor>,
    status: Res<StatusLine>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = layout.root(ctx, "status_bar_root");

    egui::Panel::bottom("status_bar").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            if status.is_error() {
                ui.colored_label(ui.visuals().error_fg_color, status.text());
            } else {
                ui.label(status.text());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let terrain = document.terrain();
                let extent = terrain.extent();
                ui.label(format!(
                    "{} × {} cells · {:.0} × {:.0} m",
                    terrain.size().cells_x(),
                    terrain.size().cells_z(),
                    extent.x,
                    extent.y,
                ));
                ui.separator();
                match cursor.hit {
                    Some(p) => ui.monospace(format!(
                        "x {:7.1}  z {:7.1}  h {:6.1}  {}",
                        p.x,
                        p.z,
                        p.y,
                        ground_note(terrain, p.xz())
                    )),
                    None => ui.monospace("off map"),
                };
            });
        });
    });

    layout.claim(&root);
    Ok(())
}

/// What the game's collision will make of the ground under the pointer.
/// Fixed width, so the bar doesn't jitter as the pointer moves.
fn ground_note(terrain: &Terrain, xz: Vec2) -> String {
    match terrain.ground_at(xz) {
        GroundAt::Surface(point) => {
            let slope = point.normal.y.clamp(-1.0, 1.0).acos().to_degrees();
            let walk = if point.walkable { "walk" } else { "no walk" };
            format!("slope {slope:2.0}°  {walk:7}")
        }
        GroundAt::Hole => format!("{:18}", "hole"),
        GroundAt::Unknown => format!("{:18}", ""),
    }
}

#[cfg(test)]
mod tests {
    use lofi_world::{CellFlags, GridSize, MaterialId};

    use super::*;

    #[test]
    fn the_note_says_what_collision_will_see() {
        let size = GridSize::new(1, 1).expect("valid size");
        let mut terrain = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
        // A wall: 6 m up across one 2 m cell.
        for z in 0..=32 {
            for x in 10..=32 {
                terrain.set_height(x, z, 6.0);
            }
        }
        terrain.set_cell(20, 20, CellFlags::default().with_hole(true));
        let note = |x: f32, z: f32| ground_note(&terrain, Vec2::new(x, z));
        assert_eq!(note(5.0, 5.0), "slope  0°  walk   ");
        assert_eq!(note(19.0, 5.0), "slope 72°  no walk");
        assert_eq!(note(41.0, 41.0).trim_end(), "hole");
        // Every note is as wide as the others.
        assert_eq!(note(41.0, 41.0).chars().count(), 18);
        assert_eq!(note(5.0, 5.0).chars().count(), 18);
    }
}
