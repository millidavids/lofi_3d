use bevy::prelude::*;

use super::measure::{measure_frames, wants_measure};
use super::screenshot::{capture_when_settled, wants_screenshot};

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                capture_when_settled.run_if(wants_screenshot),
                measure_frames.run_if(wants_measure),
            ),
        );
    }
}
