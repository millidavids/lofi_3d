//! Terrain paint: each chunk blends up to four materials, per vertex.
//!
//! A chunk stores weights for every vertex its mesh uses, including the ones
//! on its borders, so it renders from its own data alone. [`crate::Terrain`]
//! writes a shared vertex into every chunk that has it, so neighbours never
//! disagree, and refuses paint a chunk has no slot for.

use crate::grid::CHUNK_CELLS;
use crate::layers::paint_toward;
use crate::materials::MaterialId;

/// Materials a chunk can blend: WoW's per-chunk limit.
pub const SLOTS: usize = 4;
const SIDE: usize = CHUNK_CELLS as usize + 1;
/// Vertices whose weights a chunk stores.
pub const CHUNK_PAINT_VERTICES: usize = SIDE * SIDE;

type Slots = [Option<MaterialId>; SLOTS];
type SlotWeights = [u8; SLOTS];

/// What one vertex shows: up to four materials whose weights sum to 255.
///
/// Kept canonical (materials ascending, no zero weights), so equal paint
/// compares equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexPaint {
    entries: [Option<(MaterialId, u8)>; SLOTS],
}

impl VertexPaint {
    #[must_use]
    pub const fn solid(material: MaterialId) -> Self {
        Self {
            entries: [Some((material, 255)), None, None, None],
        }
    }

    /// Rescales to sum to 255 and drops zero weights. `None` for more than
    /// four materials or nothing at all.
    #[must_use]
    pub fn new(pairs: &[(MaterialId, u8)]) -> Option<Self> {
        let mut merged: Vec<(MaterialId, u32)> = Vec::with_capacity(SLOTS);
        for &(m, w) in pairs.iter().filter(|(_, w)| *w > 0) {
            match merged.iter_mut().find(|(id, _)| *id == m) {
                Some(entry) => entry.1 += u32::from(w),
                None => merged.push((m, u32::from(w))),
            }
        }
        if merged.is_empty() || merged.len() > SLOTS {
            return None;
        }
        merged.sort_by_key(|(m, _)| *m);
        let mut weights = [0u8; SLOTS];
        let total: u32 = merged.iter().map(|(_, w)| w).sum();
        for (slot, (_, w)) in weights.iter_mut().zip(&merged) {
            *slot = (w * 255 / total) as u8;
        }
        let assigned: u32 = weights.iter().map(|&w| u32::from(w)).sum();
        let largest = (0..merged.len()).max_by_key(|&i| merged[i].1).unwrap_or(0);
        weights[largest] += (255 - assigned) as u8;

        let mut entries = [None; SLOTS];
        for (entry, (&(m, _), &w)) in entries.iter_mut().zip(merged.iter().zip(&weights)) {
            *entry = (w > 0).then_some((m, w));
        }
        Some(Self::canonical(entries))
    }

    #[must_use]
    pub fn weight_of(&self, material: MaterialId) -> u8 {
        self.iter()
            .find(|(m, _)| *m == material)
            .map_or(0, |(_, w)| w)
    }

    /// Materials with their weights, ascending by id.
    pub fn iter(&self) -> impl Iterator<Item = (MaterialId, u8)> + '_ {
        self.entries.iter().flatten().copied()
    }

    /// The material with the most weight.
    #[must_use]
    pub fn dominant(&self) -> MaterialId {
        self.iter()
            .max_by_key(|(_, w)| *w)
            .map_or(MaterialId(0), |(m, _)| m)
    }

    /// Moves toward all-`material` by `amount` (0..1), rounding with `dither`
    /// (see [`crate::paint_toward`]). `None` if the vertex already blends four
    /// other materials.
    #[must_use]
    pub fn toward(&self, material: MaterialId, amount: f32, dither: f32) -> Option<Self> {
        let mut ids: Slots = [None; SLOTS];
        let mut weights: SlotWeights = [0; SLOTS];
        for (i, (m, w)) in self.iter().enumerate() {
            ids[i] = Some(m);
            weights[i] = w;
        }
        let target = match ids.iter().position(|id| *id == Some(material)) {
            Some(i) => i,
            None => {
                let free = ids.iter().position(Option::is_none)?;
                ids[free] = Some(material);
                free
            }
        };
        let painted = paint_toward(weights, target, amount, dither);
        let mut entries = [None; SLOTS];
        for (entry, (id, w)) in entries.iter_mut().zip(ids.iter().zip(painted)) {
            *entry = id.filter(|_| w > 0).map(|m| (m, w));
        }
        Some(Self::canonical(entries))
    }

    fn canonical(mut entries: [Option<(MaterialId, u8)>; SLOTS]) -> Self {
        // `None` sorts first in `Option`'s order; we want it last.
        entries.sort_by_key(|e| e.map_or((1, MaterialId(0)), |(m, _)| (0, m)));
        Self { entries }
    }
}

/// One chunk's paint.
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkPaint {
    /// Ascending ids, empty slots last.
    slots: Slots,
    /// Row-major over the chunk's full vertex rect, borders included.
    weights: Vec<SlotWeights>,
}

impl ChunkPaint {
    #[must_use]
    pub fn solid(material: MaterialId) -> Self {
        Self {
            slots: [Some(material), None, None, None],
            weights: vec![[255, 0, 0, 0]; CHUNK_PAINT_VERTICES],
        }
    }

    /// Checks paint read from disk: ids unique, weights normalised over the
    /// occupied slots, unused slots freed.
    pub(crate) fn from_parts(slots: Slots, weights: Vec<SlotWeights>) -> Result<Self, String> {
        if weights.len() != CHUNK_PAINT_VERTICES {
            return Err(format!("{} paint weights in a chunk", weights.len()));
        }
        for (i, a) in slots.iter().enumerate() {
            if a.is_some() && slots[i + 1..].contains(a) {
                return Err("a chunk lists one material twice".to_owned());
            }
        }
        let Some(first) = slots.iter().position(Option::is_some) else {
            return Err("a chunk with no materials".to_owned());
        };
        let weights = weights
            .into_iter()
            .map(|mut w| {
                for (weight, slot) in w.iter_mut().zip(&slots) {
                    if slot.is_none() {
                        *weight = 0;
                    }
                }
                normalize_onto(w, first)
            })
            .collect();
        let mut paint = Self { slots, weights };
        paint.free_unused_slots(None);
        paint.canonicalize();
        Ok(paint)
    }

    #[must_use]
    pub const fn slots(&self) -> Slots {
        self.slots
    }

    /// Row-major over the chunk's vertex rect, in slot order.
    #[must_use]
    pub fn weights(&self) -> &[SlotWeights] {
        &self.weights
    }

    #[must_use]
    pub fn at(&self, lx: u32, lz: u32) -> VertexPaint {
        let w = self.weights[local(lx, lz)];
        let mut entries = [None; SLOTS];
        for (entry, (slot, weight)) in entries.iter_mut().zip(self.slots.iter().zip(w)) {
            *entry = slot.filter(|_| weight > 0).map(|m| (m, weight));
        }
        VertexPaint::canonical(entries)
    }

    /// Materials with any weight anywhere in the chunk.
    pub fn materials(&self) -> impl Iterator<Item = MaterialId> + '_ {
        // Empty slots first: they'd scan every weight to find nothing.
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(s, m)| m.map(|m| (s, m)))
            .filter(|&(s, _)| self.weights.iter().any(|w| w[s] > 0))
            .map(|(_, m)| m)
    }

    /// Whether `paint` can go at a vertex: every material it uses has a
    /// slot, or one can be freed from materials nothing else here uses.
    #[must_use]
    pub fn can_fit(&self, lx: u32, lz: u32, paint: &VertexPaint) -> bool {
        let missing = paint
            .iter()
            .filter(|(m, _)| !self.slots.contains(&Some(*m)))
            .count();
        if missing == 0 {
            return true;
        }
        let free = self.slots.iter().filter(|s| s.is_none()).count();
        if missing <= free {
            return true;
        }
        let here = local(lx, lz);
        let reclaimable = (0..SLOTS)
            .filter(|&s| {
                self.slots[s].is_some_and(|m| paint.weight_of(m) == 0)
                    && self
                        .weights
                        .iter()
                        .enumerate()
                        .all(|(i, w)| i == here || w[s] == 0)
            })
            .count();
        missing <= free + reclaimable
    }

    /// Writes a vertex. The caller checks [`Self::can_fit`] first. Returns
    /// whether anything changed.
    pub(crate) fn set(&mut self, lx: u32, lz: u32, paint: &VertexPaint) -> bool {
        if self.at(lx, lz) == *paint {
            return false;
        }
        let here = local(lx, lz);
        self.weights[here] = [0; SLOTS];
        if paint.iter().any(|(m, _)| !self.slots.contains(&Some(m))) {
            self.free_unused_slots(Some(paint));
            for (m, _) in paint.iter() {
                if !self.slots.contains(&Some(m)) {
                    let free = self
                        .slots
                        .iter()
                        .position(Option::is_none)
                        .expect("can_fit checked there is room");
                    self.slots[free] = Some(m);
                }
            }
        }
        for (weight, slot) in self.weights[here].iter_mut().zip(&self.slots) {
            *weight = slot.map_or(0, |m| paint.weight_of(m));
        }
        self.canonicalize();
        true
    }

    /// Moves everything painted `from` onto `to`.
    pub(crate) fn replace(&mut self, from: MaterialId, to: MaterialId) -> bool {
        let Some(src) = self.slots.iter().position(|s| *s == Some(from)) else {
            return false;
        };
        // Merging a slot into itself would double then zero its weights.
        if from == to {
            return false;
        }
        match self.slots.iter().position(|s| *s == Some(to)) {
            Some(dst) => {
                for w in &mut self.weights {
                    w[dst] = w[dst].saturating_add(w[src]);
                    w[src] = 0;
                }
                self.slots[src] = None;
            }
            None => self.slots[src] = Some(to),
        }
        self.canonicalize();
        true
    }

    /// Empties slots whose weight is zero everywhere, keeping any `keep` uses.
    fn free_unused_slots(&mut self, keep: Option<&VertexPaint>) {
        for s in 0..SLOTS {
            let Some(m) = self.slots[s] else { continue };
            let kept = keep.is_some_and(|p| p.weight_of(m) > 0);
            if !kept && self.weights.iter().all(|w| w[s] == 0) {
                self.slots[s] = None;
            }
        }
    }

    /// Slots ascending by id, empty ones last, weights following their slots.
    fn canonicalize(&mut self) {
        let mut order: [usize; SLOTS] = [0, 1, 2, 3];
        order.sort_by_key(|&s| self.slots[s].map_or((1, MaterialId(0)), |m| (0, m)));
        if order == [0, 1, 2, 3] {
            return;
        }
        self.slots = order.map(|s| self.slots[s]);
        for w in &mut self.weights {
            *w = order.map(|s| w[s]);
        }
    }
}

const fn local(lx: u32, lz: u32) -> usize {
    lz as usize * SIDE + lx as usize
}

/// Rescales to 255; all-zero goes wholly to `fallback`.
fn normalize_onto(w: SlotWeights, fallback: usize) -> SlotWeights {
    let sum: u32 = w.iter().map(|&v| u32::from(v)).sum();
    if sum == 0 {
        let mut out = [0; SLOTS];
        out[fallback] = 255;
        return out;
    }
    if sum == 255 {
        return w;
    }
    let mut out = w.map(|v| (u32::from(v) * 255 / sum) as u8);
    let assigned: u32 = out.iter().map(|&v| u32::from(v)).sum();
    let largest = (0..SLOTS).max_by_key(|&i| w[i]).unwrap_or(fallback);
    out[largest] += (255 - assigned) as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRASS: MaterialId = MaterialId(0);
    const DIRT: MaterialId = MaterialId(1);
    const ROCK: MaterialId = MaterialId(2);
    const SAND: MaterialId = MaterialId(3);
    const SNOW: MaterialId = MaterialId(4);

    fn sum(p: &VertexPaint) -> u32 {
        p.iter().map(|(_, w)| u32::from(w)).sum()
    }

    #[test]
    fn vertex_paint_is_canonical() {
        let a = VertexPaint::new(&[(DIRT, 100), (GRASS, 155)]).expect("valid");
        let b = VertexPaint::new(&[(GRASS, 155), (DIRT, 100), (ROCK, 0)]).expect("valid");
        assert_eq!(a, b);
        assert_eq!(sum(&a), 255);
        assert!(
            VertexPaint::new(&[(GRASS, 1), (DIRT, 1), (ROCK, 1), (SAND, 1), (SNOW, 1)]).is_none()
        );
    }

    #[test]
    fn painting_toward_a_new_material() {
        let p = VertexPaint::solid(GRASS)
            .toward(DIRT, 0.5, 0.5)
            .expect("room");
        assert!(p.weight_of(DIRT) > 100 && p.weight_of(GRASS) > 100);
        assert_eq!(sum(&p), 255);
        let full =
            VertexPaint::new(&[(GRASS, 60), (DIRT, 60), (ROCK, 60), (SAND, 75)]).expect("valid");
        assert!(full.toward(SNOW, 0.5, 0.5).is_none());
        assert!(full.toward(ROCK, 0.5, 0.5).is_some());
    }

    #[test]
    fn chunks_take_four_materials_then_refuse_a_fifth() {
        let mut c = ChunkPaint::solid(GRASS);
        for (i, m) in [DIRT, ROCK, SAND].into_iter().enumerate() {
            let p = VertexPaint::solid(m);
            assert!(c.can_fit(i as u32, 0, &p));
            c.set(i as u32, 0, &p);
        }
        let snow = VertexPaint::solid(SNOW);
        assert!(
            !c.can_fit(5, 5, &snow),
            "grass, dirt, rock, sand all in use"
        );
        // Painting over the only sand vertex frees sand's slot.
        assert!(c.can_fit(2, 0, &snow));
        c.set(2, 0, &snow);
        assert_eq!(c.slots(), [Some(GRASS), Some(DIRT), Some(ROCK), Some(SNOW)]);
    }

    #[test]
    fn replace_merges_into_an_existing_slot() {
        let mut c = ChunkPaint::solid(GRASS);
        c.set(
            1,
            1,
            &VertexPaint::new(&[(GRASS, 55), (DIRT, 200)]).expect("valid"),
        );
        c.replace(DIRT, GRASS);
        assert_eq!(c.at(1, 1), VertexPaint::solid(GRASS));
        assert_eq!(c.slots(), [Some(GRASS), None, None, None]);
    }

    #[test]
    fn from_parts_repairs_and_rejects() {
        let mut weights = vec![[255, 0, 0, 0]; CHUNK_PAINT_VERTICES];
        weights[0] = [0, 0, 0, 0];
        weights[1] = [100, 100, 0, 0];
        let c = ChunkPaint::from_parts([Some(DIRT), Some(GRASS), None, None], weights)
            .expect("repairable");
        assert_eq!(c.slots(), [Some(GRASS), Some(DIRT), None, None], "sorted");
        assert_eq!(
            c.at(0, 0),
            VertexPaint::solid(DIRT),
            "all-zero goes to the first slot"
        );
        assert_eq!(
            c.at(1, 0).iter().map(|(_, w)| u32::from(w)).sum::<u32>(),
            255
        );

        let dup = ChunkPaint::from_parts(
            [Some(GRASS), Some(GRASS), None, None],
            vec![[255, 0, 0, 0]; CHUNK_PAINT_VERTICES],
        );
        assert!(dup.is_err());
        assert!(ChunkPaint::from_parts([None; 4], vec![[0; 4]; CHUNK_PAINT_VERTICES]).is_err());
    }
}
