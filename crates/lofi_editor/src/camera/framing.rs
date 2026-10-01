//! Puts the whole map in view: on start, after new/open, and on request.

use bevy::prelude::*;

use super::fly::EditorCamera;
use crate::document::MapDocument;
use crate::launch::LaunchOptions;

#[derive(Message, Clone, Copy, Debug)]
pub struct FrameMapMessage;

pub(super) fn frame_map(
    document: Res<MapDocument>,
    options: Res<LaunchOptions>,
    mut requests: MessageReader<FrameMapMessage>,
    mut framed_generation: Local<Option<u64>>,
    camera: Single<(&mut Transform, &mut EditorCamera)>,
) {
    let requested = requests.read().last().is_some();
    let first = framed_generation.is_none();
    let new_map = *framed_generation != Some(document.generation());
    if !requested && !new_map {
        return;
    }
    *framed_generation = Some(document.generation());
    let (mut transform, mut camera) = camera.into_inner();

    if first && let Some((eye, target)) = options.camera {
        camera.aim(&mut transform, eye, target);
        return;
    }

    let terrain = document.terrain();
    let extent = terrain.extent();
    let (low, high) = terrain.height_range();
    let target = Vec3::new(extent.x * 0.5, (low + high) * 0.5, extent.y * 0.5);
    let reach = extent.max_element();
    let eye = if options.measure {
        // Steeply down from high up: every chunk on screen at once, the
        // worst case for drawing.
        target + Vec3::new(0.0, reach * 1.3 + (high - low), reach * 0.25)
    } else {
        // From the south, about 30 degrees down, far enough back to fit the
        // map at the default 45 degree field of view.
        target + Vec3::new(0.0, reach * 0.55 + (high - low), reach * 0.95)
    };
    camera.aim(&mut transform, eye, target);
}
