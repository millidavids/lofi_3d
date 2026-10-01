//! Which tool is active, what it places, and what's selected.

use bevy::prelude::*;
use lofi_world::{AssetGuid, BUILTIN_ASSETS, ObjectId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DoodadTool {
    #[default]
    Place,
    Select,
}

impl DoodadTool {
    pub const ALL: [Self; 2] = [Self::Place, Self::Select];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Place => "Place",
            Self::Select => "Select / move",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::Place => "Click the ground to place the chosen asset.",
            Self::Select => {
                "Click an object to select it; drag to move it. R / Shift+R turns, \
                 [ ] scales, End drops to the ground, Delete removes."
            }
        }
    }
}

#[derive(Resource)]
pub struct DoodadSettings {
    pub tool: DoodadTool,
    pub asset: AssetGuid,
    /// Each placement gets its own turn, so woods don't look stamped.
    pub random_yaw: bool,
    pub scale: f32,
    /// Placements so far: the seed for the next random turn.
    pub(super) placed: u32,
}

impl Default for DoodadSettings {
    fn default() -> Self {
        Self {
            tool: DoodadTool::Place,
            asset: BUILTIN_ASSETS[0].guid,
            random_yaw: true,
            scale: 1.0,
            placed: 0,
        }
    }
}

#[derive(Resource, Default)]
pub struct SelectedObject(pub Option<ObjectId>);
