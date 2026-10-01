//! Placeholder assets until there are real models: a few simple shapes with
//! fixed GUIDs. A map can name GUIDs this catalog doesn't know (from a later
//! catalog); tools keep them and draw a box.

use crate::objects::AssetGuid;

/// How a placeholder asset is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Round canopy on a trunk.
    Tree,
    /// Stacked cones.
    Pine,
    Rock,
    Crate,
    /// A box with a pitched roof.
    Hut,
    Pillar,
    /// What an unknown GUID shows as.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asset {
    pub guid: AssetGuid,
    pub name: &'static str,
    pub shape: Shape,
    /// sRGB.
    pub color: [f32; 3],
    /// Reach from the centre line at scale 1, in metres.
    pub radius: f32,
    /// Height above the base at scale 1, in metres.
    pub height: f32,
}

impl Asset {
    /// A sphere holding the whole shape at `scale`, as an offset of its
    /// centre above the base and a radius: for picking.
    #[must_use]
    pub fn bounds(&self, scale: f32) -> (f32, f32) {
        let half = self.height * 0.5;
        (half * scale, self.radius.hypot(half) * scale)
    }
}

/// The placeholder catalog. GUIDs are fixed forever; never reuse one.
pub const BUILTIN: [Asset; 6] = [
    Asset {
        guid: AssetGuid(0x6c6f_6669_0000_0000_0000_0000_0000_0001),
        name: "Oak",
        shape: Shape::Tree,
        color: [0.30, 0.45, 0.18],
        radius: 2.4,
        height: 6.2,
    },
    Asset {
        guid: AssetGuid(0x6c6f_6669_0000_0000_0000_0000_0000_0002),
        name: "Pine",
        shape: Shape::Pine,
        color: [0.16, 0.32, 0.20],
        radius: 2.0,
        height: 8.0,
    },
    Asset {
        guid: AssetGuid(0x6c6f_6669_0000_0000_0000_0000_0000_0003),
        name: "Boulder",
        shape: Shape::Rock,
        color: [0.48, 0.46, 0.43],
        radius: 1.7,
        height: 1.6,
    },
    Asset {
        guid: AssetGuid(0x6c6f_6669_0000_0000_0000_0000_0000_0004),
        name: "Crate",
        shape: Shape::Crate,
        color: [0.55, 0.40, 0.22],
        radius: 0.85,
        height: 1.2,
    },
    Asset {
        guid: AssetGuid(0x6c6f_6669_0000_0000_0000_0000_0000_0005),
        name: "Hut",
        shape: Shape::Hut,
        color: [0.62, 0.52, 0.38],
        radius: 3.05,
        height: 4.2,
    },
    Asset {
        guid: AssetGuid(0x6c6f_6669_0000_0000_0000_0000_0000_0006),
        name: "Pillar",
        shape: Shape::Pillar,
        color: [0.70, 0.68, 0.62],
        radius: 0.85,
        height: 5.0,
    },
];

/// What an unknown GUID draws as.
pub const UNKNOWN_ASSET: Asset = Asset {
    guid: AssetGuid(0),
    name: "Unknown asset",
    shape: Shape::Unknown,
    color: [0.9, 0.2, 0.8],
    radius: 0.71,
    height: 1.0,
};

/// The catalog entry for a GUID, or the unknown placeholder.
#[must_use]
pub fn asset(guid: AssetGuid) -> Asset {
    BUILTIN
        .iter()
        .find(|a| a.guid == guid)
        .copied()
        .unwrap_or(Asset {
            guid,
            ..UNKNOWN_ASSET
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_are_unique_and_unknowns_keep_theirs() {
        for (i, a) in BUILTIN.iter().enumerate() {
            assert!(BUILTIN[i + 1..].iter().all(|b| b.guid != a.guid));
            assert_eq!(asset(a.guid), *a);
        }
        let stranger = asset(AssetGuid(42));
        assert_eq!(stranger.shape, Shape::Unknown);
        assert_eq!(stranger.guid, AssetGuid(42));
    }
}
