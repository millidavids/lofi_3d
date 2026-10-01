//! Things placed on the map: trees, rocks, buildings. Each has an id that is
//! never reused and names its asset by GUID, never by file path, so assets
//! can move and be renamed without breaking maps.

use std::fmt;
use std::sync::Arc;

use bevy_math::Vec3;

/// Most objects a map holds; also the cap on what a file may claim.
pub const MAX_OBJECTS: usize = 1_000_000;
pub const MIN_OBJECT_SCALE: f32 = 0.01;
pub const MAX_OBJECT_SCALE: f32 = 100.0;
/// How far from the origin an object may sit, in metres: far past any map,
/// near enough that tile and grid arithmetic stays exact.
pub const MAX_OBJECT_XZ: f32 = 1.0e6;
pub const MAX_OBJECT_Y: f32 = 1.0e5;

/// Which asset an object shows. Stable across renames and moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetGuid(pub u128);

impl fmt::Display for AssetGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// An object's identity within its map. Allocated in order, never reused,
/// so references from other data (quests, scripts) can't silently retarget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedObject {
    pub id: ObjectId,
    pub asset: AssetGuid,
    /// Where the object's origin (its base) sits, world space.
    pub position: Vec3,
    /// Turn about +Y, radians.
    pub yaw: f32,
    /// Uniform scale, clamped to `MIN_OBJECT_SCALE..=MAX_OBJECT_SCALE`.
    pub scale: f32,
}

/// The objects themselves, shared until written. What undo keeps: never the
/// next id, which only ever grows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObjectList(Arc<Vec<PlacedObject>>);

impl ObjectList {
    /// Whether two lists are the same allocation: cheap "unchanged" for views.
    #[must_use]
    pub fn shares(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Heap bytes, for undo budgets.
    #[must_use]
    pub fn bytes(&self) -> usize {
        size_of_val(self.0.as_slice())
    }
}

/// The map's objects, sorted by id. Cloning shares the list until written,
/// like terrain chunks, so undo snapshots are cheap.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectLayer {
    next_id: u64,
    objects: ObjectList,
    /// Read from a newer object format this build can't edit. Its section
    /// is kept as unknown data and written back untouched.
    foreign: bool,
}

impl Default for ObjectLayer {
    fn default() -> Self {
        Self {
            // 0 is left unused so it can mean "none" in later data.
            next_id: 1,
            objects: ObjectList::default(),
            foreign: false,
        }
    }
}

impl ObjectLayer {
    /// Stands in for objects from a newer format: empty and uneditable.
    pub(crate) fn foreign() -> Self {
        Self {
            foreign: true,
            ..Self::default()
        }
    }

    /// Objects were saved by a newer tool; editing them here would lose data.
    #[must_use]
    pub const fn is_foreign(&self) -> bool {
        self.foreign
    }

    /// The current list, to restore later with [`Self::swap_list`].
    #[must_use]
    pub fn list(&self) -> ObjectList {
        self.objects.clone()
    }

    /// Puts `list` in place and hands back the one it replaced. The next id
    /// stays put, so undoing a placement never lets its id be reused.
    pub fn swap_list(&mut self, list: &mut ObjectList) {
        std::mem::swap(&mut self.objects, list);
    }

    /// From stored parts. Errors say what's wrong.
    pub(crate) fn from_parts(next_id: u64, mut objects: Vec<PlacedObject>) -> Result<Self, String> {
        if objects.len() > MAX_OBJECTS {
            return Err(format!("{} objects; at most {MAX_OBJECTS}", objects.len()));
        }
        if next_id == 0 {
            return Err("next object id is 0".to_owned());
        }
        objects.sort_by_key(|o| o.id);
        for pair in objects.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(format!("two objects with id {}", pair[0].id.0));
            }
        }
        for o in &objects {
            if o.id.0 == 0 || o.id.0 >= next_id {
                return Err(format!("object id {} outside 1..{next_id}", o.id.0));
            }
            if !in_bounds(o.position) || !o.yaw.is_finite() {
                return Err(format!("object {} is placed out of bounds", o.id.0));
            }
            if !(MIN_OBJECT_SCALE..=MAX_OBJECT_SCALE).contains(&o.scale) {
                return Err(format!("object {} has scale {}", o.id.0, o.scale));
            }
        }
        Ok(Self {
            next_id,
            objects: ObjectList(Arc::new(objects)),
            foreign: false,
        })
    }

    #[must_use]
    pub const fn next_id(&self) -> u64 {
        self.next_id
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlacedObject> {
        self.objects.0.iter()
    }

    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&PlacedObject> {
        self.index_of(id).map(|i| &self.objects.0[i])
    }

    /// Places a new object and returns its id. `None` when the map is full,
    /// out of ids, the place is out of bounds, or the objects are foreign.
    pub fn add(
        &mut self,
        asset: AssetGuid,
        position: Vec3,
        yaw: f32,
        scale: f32,
    ) -> Option<ObjectId> {
        if self.foreign || self.objects.0.len() >= MAX_OBJECTS || !in_bounds(position) {
            return None;
        }
        let id = ObjectId(self.next_id);
        self.next_id = self.next_id.checked_add(1)?;
        // Ids only grow, so pushing keeps the list sorted.
        Arc::make_mut(&mut self.objects.0).push(PlacedObject {
            id,
            asset,
            position,
            yaw: wrap_yaw(yaw),
            scale: clamp_scale(scale),
        });
        Some(id)
    }

    /// Changes one object's placement. Returns whether anything changed;
    /// an unchanged update copies nothing.
    pub fn update(&mut self, id: ObjectId, edit: impl FnOnce(&mut PlacedObject)) -> bool {
        let Some(i) = self.index_of(id) else {
            return false;
        };
        let current = self.objects.0[i];
        let mut next = current;
        edit(&mut next);
        // Identity and asset aren't placement; keep them.
        next.id = id;
        next.asset = current.asset;
        next.yaw = wrap_yaw(next.yaw);
        next.scale = clamp_scale(next.scale);
        if !in_bounds(next.position) || next == current {
            return false;
        }
        Arc::make_mut(&mut self.objects.0)[i] = next;
        true
    }

    /// Returns whether it was there. Its id is never handed out again.
    pub fn remove(&mut self, id: ObjectId) -> bool {
        let Some(i) = self.index_of(id) else {
            return false;
        };
        Arc::make_mut(&mut self.objects.0).remove(i);
        true
    }

    fn index_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.0.binary_search_by_key(&id, |o| o.id).ok()
    }
}

fn in_bounds(p: Vec3) -> bool {
    p.is_finite()
        && p.x.abs() <= MAX_OBJECT_XZ
        && p.z.abs() <= MAX_OBJECT_XZ
        && p.y.abs() <= MAX_OBJECT_Y
}

fn wrap_yaw(yaw: f32) -> f32 {
    if yaw.is_finite() {
        yaw.rem_euclid(std::f32::consts::TAU)
    } else {
        0.0
    }
}

fn clamp_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_OBJECT_SCALE, MAX_OBJECT_SCALE)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: AssetGuid = AssetGuid(0xabc);

    #[test]
    fn ids_only_grow_even_after_removal() {
        let mut layer = ObjectLayer::default();
        let a = layer.add(TREE, Vec3::ZERO, 0.0, 1.0).expect("room");
        let b = layer.add(TREE, Vec3::X, 0.0, 1.0).expect("room");
        assert!(layer.remove(b));
        let c = layer.add(TREE, Vec3::Y, 0.0, 1.0).expect("room");
        assert!(a < b && b < c, "{a:?} {b:?} {c:?}");
        assert!(layer.get(b).is_none());
        assert_eq!(layer.len(), 2);
    }

    #[test]
    fn updates_clamp_and_keep_identity() {
        let mut layer = ObjectLayer::default();
        let id = layer.add(TREE, Vec3::ZERO, 0.0, 1.0).expect("room");
        assert!(layer.update(id, |o| {
            o.scale = 1000.0;
            o.yaw = -1.0;
            o.id = ObjectId(99);
            o.asset = AssetGuid(1);
        }));
        let o = layer.get(id).expect("still there");
        assert_eq!(o.scale, MAX_OBJECT_SCALE);
        assert!((o.yaw - (std::f32::consts::TAU - 1.0)).abs() < 1e-5);
        assert_eq!(o.asset, TREE);
        assert!(!layer.update(id, |_| {}), "no change");
        assert!(!layer.update(id, |o| o.position = Vec3::NAN));
    }

    #[test]
    fn clones_share_until_written() {
        let mut layer = ObjectLayer::default();
        layer.add(TREE, Vec3::ZERO, 0.0, 1.0);
        let snapshot = layer.clone();
        assert!(snapshot.list().shares(&layer.list()));
        layer.add(TREE, Vec3::ONE, 0.0, 1.0);
        assert_eq!(snapshot.len(), 1);
    }

    #[test]
    fn restoring_a_list_never_rewinds_ids() {
        // Place, undo the placement, place again: a fresh id.
        let mut layer = ObjectLayer::default();
        let mut before = layer.list();
        let first = layer.add(TREE, Vec3::ZERO, 0.0, 1.0).expect("room");
        layer.swap_list(&mut before);
        assert!(layer.is_empty());
        let second = layer.add(TREE, Vec3::ZERO, 0.0, 1.0).expect("room");
        assert!(second > first);
    }

    #[test]
    fn foreign_objects_refuse_edits() {
        let mut layer = ObjectLayer::foreign();
        assert!(layer.add(TREE, Vec3::ZERO, 0.0, 1.0).is_none());
        assert!(layer.is_foreign());
    }

    #[test]
    fn placements_stay_in_bounds() {
        let mut layer = ObjectLayer::default();
        assert!(
            layer
                .add(TREE, Vec3::new(2.0e6, 0.0, 0.0), 0.0, 1.0)
                .is_none()
        );
        let id = layer.add(TREE, Vec3::ZERO, 0.0, 1.0).expect("room");
        assert!(!layer.update(id, |o| o.position.y = 1.0e7));
    }

    #[test]
    fn stored_parts_are_validated() {
        let object = |id: u64, scale: f32| PlacedObject {
            id: ObjectId(id),
            asset: TREE,
            position: Vec3::ZERO,
            yaw: 0.0,
            scale,
        };
        assert!(ObjectLayer::from_parts(3, vec![object(2, 1.0), object(1, 1.0)]).is_ok());
        assert!(ObjectLayer::from_parts(3, vec![object(1, 1.0), object(1, 1.0)]).is_err());
        assert!(
            ObjectLayer::from_parts(2, vec![object(2, 1.0)]).is_err(),
            "id past next"
        );
        assert!(
            ObjectLayer::from_parts(2, vec![object(0, 1.0)]).is_err(),
            "id 0"
        );
        assert!(
            ObjectLayer::from_parts(2, vec![object(1, 0.0)]).is_err(),
            "scale"
        );
        assert!(ObjectLayer::from_parts(0, Vec::new()).is_err(), "no ids");
    }

    #[test]
    fn running_out_of_ids_refuses_rather_than_wrapping() {
        let mut layer = ObjectLayer::from_parts(u64::MAX, Vec::new()).expect("loads");
        assert!(layer.add(TREE, Vec3::ZERO, 0.0, 1.0).is_none());
        assert_eq!(layer.next_id(), u64::MAX);
    }

    #[test]
    fn guids_print_as_hex() {
        assert_eq!(AssetGuid(0xff).to_string(), format!("{:0>32}", "ff"));
    }
}
