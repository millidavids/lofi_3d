//! The map's terrain materials: what paint can be.

/// Materials per map. The renderer holds them in one texture array.
pub const MAX_MATERIALS: usize = 32;

/// Stable for the life of a map: deleting a material never renumbers others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialId(pub u16);

/// The procedural surface detail a material gets until it has a real texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pattern {
    #[default]
    Grass,
    Dirt,
    Rock,
    Sand,
    Snow,
    Cobble,
    Plain,
}

impl Pattern {
    pub const ALL: [Self; 7] = [
        Self::Grass,
        Self::Dirt,
        Self::Rock,
        Self::Sand,
        Self::Snow,
        Self::Cobble,
        Self::Plain,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Grass => "Grass",
            Self::Dirt => "Dirt",
            Self::Rock => "Rock",
            Self::Sand => "Sand",
            Self::Snow => "Snow",
            Self::Cobble => "Cobble",
            Self::Plain => "Plain",
        }
    }

    /// Unknown names fall back to `Plain`, so a newer pattern still loads.
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|p| p.label() == label)
            .unwrap_or(Self::Plain)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    /// sRGB, 0..1. Tints the texture.
    pub color: [f32; 3],
    pub pattern: Pattern,
    /// Metres one texture repeat covers.
    pub tile_size: f32,
}

pub const MIN_TILE_SIZE: f32 = 0.5;
pub const MAX_TILE_SIZE: f32 = 64.0;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialTable {
    materials: Vec<Material>,
    next_id: u16,
    /// What erasing paints back to.
    base: MaterialId,
}

impl Default for MaterialTable {
    fn default() -> Self {
        Self::starter()
    }
}

impl MaterialTable {
    /// The usual WoW-ish starting set.
    #[must_use]
    pub fn starter() -> Self {
        let mut table = Self {
            materials: Vec::new(),
            next_id: 0,
            base: MaterialId(0),
        };
        for (name, color, pattern) in [
            ("Grass", [0.36, 0.49, 0.20], Pattern::Grass),
            ("Dirt", [0.48, 0.37, 0.24], Pattern::Dirt),
            ("Rock", [0.50, 0.48, 0.45], Pattern::Rock),
            ("Sand", [0.79, 0.70, 0.49], Pattern::Sand),
            ("Snow", [0.90, 0.92, 0.95], Pattern::Snow),
        ] {
            table.add(name, color, pattern);
        }
        table
    }

    /// A table from its parts, checked: ids unique and below `next_id`, the
    /// base among them. Colours and tile sizes are clamped.
    pub fn new(materials: Vec<Material>, next_id: u16, base: MaterialId) -> Result<Self, String> {
        if materials.is_empty() || materials.len() > MAX_MATERIALS {
            return Err(format!("{} materials", materials.len()));
        }
        let mut seen = std::collections::BTreeSet::new();
        for m in &materials {
            if !seen.insert(m.id) {
                return Err(format!("material id {} used twice", m.id.0));
            }
            if m.id.0 >= next_id {
                return Err(format!(
                    "material id {} not below next id {next_id}",
                    m.id.0
                ));
            }
        }
        if !seen.contains(&base) {
            return Err(format!("base material {} is missing", base.0));
        }
        let materials = materials
            .into_iter()
            .map(|mut m| {
                m.color = m.color.map(|c| {
                    if c.is_finite() {
                        c.clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                });
                m.tile_size = clamp_tile_size(m.tile_size);
                m
            })
            .collect();
        Ok(Self {
            materials,
            next_id,
            base,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Material> {
        self.materials.iter()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.materials.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.materials.len() >= MAX_MATERIALS
    }

    #[must_use]
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.iter_mut().find(|m| m.id == id)
    }

    /// Position in the table, which is also the renderer's texture layer.
    #[must_use]
    pub fn index_of(&self, id: MaterialId) -> Option<usize> {
        self.materials.iter().position(|m| m.id == id)
    }

    #[must_use]
    pub const fn base(&self) -> MaterialId {
        self.base
    }

    /// Ignored for unknown ids.
    pub fn set_base(&mut self, id: MaterialId) {
        if self.get(id).is_some() {
            self.base = id;
        }
    }

    #[must_use]
    pub const fn next_id(&self) -> u16 {
        self.next_id
    }

    /// `None` when the table is full.
    pub fn add(&mut self, name: &str, color: [f32; 3], pattern: Pattern) -> Option<MaterialId> {
        if self.is_full() || self.next_id == u16::MAX {
            return None;
        }
        let id = MaterialId(self.next_id);
        self.next_id += 1;
        self.materials.push(Material {
            id,
            name: name.to_owned(),
            color,
            pattern,
            tile_size: 4.0,
        });
        Some(id)
    }

    /// Removes `id` from the table. The terrain must stop using it first;
    /// see [`crate::Terrain::replace_material`]. Refuses the base material.
    pub fn remove(&mut self, id: MaterialId) -> bool {
        if id == self.base {
            return false;
        }
        let before = self.materials.len();
        self.materials.retain(|m| m.id != id);
        self.materials.len() != before
    }
}

#[must_use]
pub fn clamp_tile_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(MIN_TILE_SIZE, MAX_TILE_SIZE)
    } else {
        4.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_never_reused() {
        let mut t = MaterialTable::starter();
        let mud = t.add("Mud", [0.3, 0.25, 0.2], Pattern::Dirt).expect("room");
        assert!(t.remove(mud));
        let clay = t
            .add("Clay", [0.6, 0.4, 0.3], Pattern::Plain)
            .expect("room");
        assert_ne!(mud, clay);
    }

    #[test]
    fn base_cannot_be_removed() {
        let mut t = MaterialTable::starter();
        assert!(!t.remove(t.base()));
    }

    #[test]
    fn fills_up() {
        let mut t = MaterialTable::starter();
        while t.add("x", [0.5; 3], Pattern::Plain).is_some() {}
        assert_eq!(t.len(), MAX_MATERIALS);
    }

    #[test]
    fn new_validates() {
        let t = MaterialTable::starter();
        let ms: Vec<_> = t.iter().cloned().collect();
        assert!(MaterialTable::new(ms.clone(), t.next_id(), t.base()).is_ok());
        assert!(
            MaterialTable::new(ms.clone(), 1, t.base()).is_err(),
            "ids >= next"
        );
        assert!(MaterialTable::new(ms, t.next_id(), MaterialId(99)).is_err());
        assert!(MaterialTable::new(Vec::new(), 0, MaterialId(0)).is_err());
    }

    #[test]
    fn unknown_patterns_fall_back() {
        assert_eq!(Pattern::from_label("Rock"), Pattern::Rock);
        assert_eq!(Pattern::from_label("Lava"), Pattern::Plain);
    }
}
