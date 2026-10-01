//! The map-wide settings edited from the side panel, as one undoable value.

use lofi_world::{Lighting, MaterialTable, ObjectList, Triangulation, Water, WorldMap};

#[derive(Clone, Debug, PartialEq)]
pub struct MapFields {
    name: String,
    materials: MaterialTable,
    water: Water,
    lighting: Lighting,
    /// Lives on the terrain, but it's a map-wide setting like the rest.
    triangulation: Triangulation,
    /// The placed objects; never the next id, which only grows, so an undone
    /// placement's id is never handed out again.
    objects: ObjectList,
}

impl MapFields {
    pub fn of(map: &WorldMap) -> Self {
        Self {
            name: map.name.clone(),
            materials: map.materials.clone(),
            water: map.water,
            lighting: map.lighting,
            triangulation: map.terrain.triangulation(),
            objects: map.objects.list(),
        }
    }

    pub(super) fn swap(&mut self, map: &mut WorldMap) {
        std::mem::swap(&mut self.name, &mut map.name);
        std::mem::swap(&mut self.materials, &mut map.materials);
        std::mem::swap(&mut self.water, &mut map.water);
        std::mem::swap(&mut self.lighting, &mut map.lighting);
        let previous = map.terrain.triangulation();
        map.terrain.set_triangulation(self.triangulation);
        self.triangulation = previous;
        map.objects.swap_list(&mut self.objects);
    }

    /// What this adds to an undo step that already holds `earlier`: nothing
    /// for an object list the two share.
    pub(super) fn bytes_after(&self, earlier: Option<&Self>) -> usize {
        let shared = earlier.is_some_and(|e| e.objects.shares(&self.objects));
        size_of::<Self>() + if shared { 0 } else { self.objects.bytes() }
    }

    /// Heap bytes this holds that `map` doesn't share, for undo budgets.
    pub(super) fn unshared_bytes(&self, map: &WorldMap) -> usize {
        let objects = if self.objects.shares(&map.objects.list()) {
            0
        } else {
            self.objects.bytes()
        };
        size_of::<Self>() + objects
    }
}
