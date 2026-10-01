//! The selection: shapes on the ground that scope brushes and auto-paint.
//!
//! Editor state, not part of the map and not undoable. Shapes are kept for
//! drawing; a per-vertex mask, rebuilt when they change, answers lookups.

use bevy::prelude::*;
use lofi_world::{GridSize, Terrain, VertexRect};

/// Lasso points kept at most; more add nothing at cell resolution.
pub const MAX_LASSO_POINTS: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Corners in world XZ.
    Rect { min: Vec2, max: Vec2 },
    /// A closed outline in world XZ.
    Lasso(Vec<Vec2>),
}

impl Shape {
    /// A rect from any two opposite corners.
    pub fn rect(a: Vec2, b: Vec2) -> Self {
        Self::Rect {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Whether it encloses any ground: not a click, a sliver or a line.
    pub fn has_area(&self, cell_size: f32) -> bool {
        match self {
            Self::Rect { min, max } => (*max - *min).min_element() >= cell_size * 0.5,
            Self::Lasso(points) => points.len() >= 3,
        }
    }

    /// Even-odd test for the lasso, so a looped outline behaves predictably.
    pub fn contains(&self, p: Vec2) -> bool {
        match self {
            Self::Rect { min, max } => p.cmpge(*min).all() && p.cmple(*max).all(),
            Self::Lasso(points) => {
                let mut inside = false;
                let mut j = points.len().wrapping_sub(1);
                for (i, &a) in points.iter().enumerate() {
                    let b = points[j];
                    if (a.y > p.y) != (b.y > p.y)
                        && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
        }
    }

    fn bounds(&self) -> (Vec2, Vec2) {
        match self {
            Self::Rect { min, max } => (*min, *max),
            Self::Lasso(points) => points.iter().fold(
                (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
                |(lo, hi), &p| (lo.min(p), hi.max(p)),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Subtract,
}

#[derive(Resource, Default)]
pub struct Selection {
    shapes: Vec<(Shape, Op)>,
    /// Per vertex, row-major; empty when nothing is selected.
    mask: Vec<bool>,
    /// Bounds of the selected vertices.
    rect: Option<VertexRect>,
    size: Option<GridSize>,
    /// Map generation this was made on; another map clears it.
    pub(super) generation: Option<u64>,
}

impl Selection {
    /// Whether anything limits the tools. An empty selection limits nothing.
    pub fn is_active(&self) -> bool {
        self.rect.is_some()
    }

    pub fn shapes(&self) -> &[(Shape, Op)] {
        &self.shapes
    }

    /// The selected vertices' bounds, if there's a selection.
    pub const fn vertex_rect(&self) -> Option<VertexRect> {
        self.rect
    }

    /// 1 for a selected vertex or when nothing is selected, else 0.
    pub fn weight(&self, x: u32, z: u32) -> f32 {
        let Some(size) = self.size else {
            return 1.0;
        };
        if self.mask.is_empty() {
            return 1.0;
        }
        if self.mask[size.vertex_index(x, z)] {
            1.0
        } else {
            0.0
        }
    }

    /// For points between vertices, such as cell centres.
    pub fn contains_point(&self, p: Vec2) -> bool {
        if !self.is_active() {
            return true;
        }
        let mut inside = false;
        for (shape, op) in &self.shapes {
            if shape.contains(p) {
                inside = *op == Op::Add;
            }
        }
        inside
    }

    /// A rectangle selection, for other modules' tests.
    #[cfg(test)]
    pub fn rect(terrain: &Terrain, min: Vec2, max: Vec2) -> Self {
        let mut selection = Self::default();
        selection.apply(Shape::rect(min, max), Op::Add, true, terrain);
        selection
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
        self.mask.clear();
        self.rect = None;
    }

    /// Adds a shape the user drew. `replace` starts over. Subtracting from
    /// nothing does nothing.
    pub fn apply(&mut self, shape: Shape, op: Op, replace: bool, terrain: &Terrain) {
        if replace {
            self.shapes.clear();
        }
        if op == Op::Subtract && self.shapes.is_empty() {
            return;
        }
        self.shapes.push((shape, op));
        self.rasterise(terrain);
    }

    /// Rebuilds the vertex mask from the shapes, row by row: rects fill
    /// spans directly and lassos by scanline crossings, so a big lasso over
    /// a big map stays cheap.
    fn rasterise(&mut self, terrain: &Terrain) {
        let size = terrain.size();
        let cell = terrain.cell_size();
        let (verts_x, verts_z) = (size.verts_x(), size.verts_z());
        self.mask = vec![false; size.vertex_count()];
        self.size = Some(size);
        let mut spans = Vec::new();
        for (shape, op) in &self.shapes {
            let (lo, hi) = shape.bounds();
            let z_first = (lo.y / cell).ceil().max(0.0) as u32;
            let z_last = ((hi.y / cell).floor() as i64).min(i64::from(verts_z) - 1);
            if z_last < 0 {
                continue;
            }
            for z in z_first..=z_last as u32 {
                let zw = z as f32 * cell;
                row_spans(shape, zw, &mut spans);
                for &(x0, x1) in &spans {
                    let first = (x0 / cell).ceil().max(0.0) as u32;
                    let last = ((x1 / cell).floor() as i64).min(i64::from(verts_x) - 1);
                    if last < i64::from(first) {
                        continue;
                    }
                    let row = z as usize * verts_x as usize;
                    for x in first..=last as u32 {
                        self.mask[row + x as usize] = *op == Op::Add;
                    }
                }
            }
        }
        self.rect = bounds_of(&self.mask, verts_x, verts_z);
        if self.rect.is_none() {
            // Everything subtracted away: say so rather than limit to nothing.
            self.clear();
        }
    }
}

/// The x ranges a shape covers along the row at world z = `zw`.
fn row_spans(shape: &Shape, zw: f32, spans: &mut Vec<(f32, f32)>) {
    spans.clear();
    match shape {
        Shape::Rect { min, max } => {
            if zw >= min.y && zw <= max.y {
                spans.push((min.x, max.x));
            }
        }
        Shape::Lasso(points) => {
            let mut crossings: Vec<f32> = Vec::new();
            let mut j = points.len().wrapping_sub(1);
            for (i, &a) in points.iter().enumerate() {
                let b = points[j];
                if (a.y > zw) != (b.y > zw) {
                    crossings.push((b.x - a.x) * (zw - a.y) / (b.y - a.y) + a.x);
                }
                j = i;
            }
            crossings.sort_by(f32::total_cmp);
            spans.extend(crossings.as_chunks::<2>().0.iter().map(|&[a, b]| (a, b)));
        }
    }
}

fn bounds_of(mask: &[bool], verts_x: u32, verts_z: u32) -> Option<VertexRect> {
    let mut rect: Option<VertexRect> = None;
    for z in 0..verts_z {
        let row = &mask[(z * verts_x) as usize..((z + 1) * verts_x) as usize];
        let (Some(first), Some(last)) = (row.iter().position(|&s| s), row.iter().rposition(|&s| s))
        else {
            continue;
        };
        let here = VertexRect {
            min_x: first as u32,
            min_z: z,
            max_x: last as u32,
            max_z: z,
        };
        rect = Some(rect.map_or(here, |r| r.union(here)));
    }
    rect
}

#[cfg(test)]
mod tests {
    use lofi_world::{GridSize, MaterialId};

    use super::*;

    fn terrain() -> Terrain {
        Terrain::flat(
            GridSize::new(1, 1).expect("valid size"),
            2.0,
            0.0,
            MaterialId(0),
        )
    }

    fn square(at: f32, side: f32) -> Shape {
        Shape::rect(Vec2::splat(at), Vec2::splat(at + side))
    }

    #[test]
    fn empty_selection_limits_nothing() {
        let s = Selection::default();
        assert!(!s.is_active());
        assert_eq!(s.weight(3, 3), 1.0);
        assert!(s.contains_point(Vec2::new(100.0, 1.0)));
    }

    #[test]
    fn rects_add_and_subtract_in_order() {
        let t = terrain();
        let mut s = Selection::default();
        s.apply(square(10.0, 20.0), Op::Add, true, &t);
        s.apply(square(14.0, 4.0), Op::Subtract, false, &t);
        assert_eq!(s.weight(6, 6), 1.0, "12 m: inside the first square");
        assert_eq!(s.weight(8, 8), 0.0, "16 m: in the hole");
        assert_eq!(s.weight(20, 20), 0.0, "40 m: outside");
        assert_eq!(
            s.vertex_rect(),
            Some(VertexRect {
                min_x: 5,
                min_z: 5,
                max_x: 15,
                max_z: 15
            })
        );
        assert!(!s.contains_point(Vec2::new(16.0, 16.0)));
        assert!(s.contains_point(Vec2::new(11.0, 29.0)));
    }

    #[test]
    fn lasso_rasterises_like_point_tests() {
        let t = terrain();
        // A triangle with a bite out of it.
        let lasso = Shape::Lasso(vec![
            Vec2::new(4.0, 4.0),
            Vec2::new(60.0, 10.0),
            Vec2::new(30.0, 30.0),
            Vec2::new(20.0, 15.0),
            Vec2::new(10.0, 50.0),
        ]);
        let Shape::Lasso(points) = &lasso else {
            unreachable!()
        };
        let on_outline = |p: Vec2| {
            points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .any(|(&a, &b)| {
                    let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                    p.distance(a + (b - a) * t) < 1e-3
                })
        };
        let mut s = Selection::default();
        s.apply(lasso.clone(), Op::Add, true, &t);
        for (x, z) in t.size().full_rect().iter() {
            let p = Vec2::new(x as f32, z as f32) * 2.0;
            // Points exactly on the outline may fall either way.
            if on_outline(p) {
                continue;
            }
            let expected = if lasso.contains(p) { 1.0 } else { 0.0 };
            assert_eq!(s.weight(x, z), expected, "vertex {x},{z}");
        }
    }

    #[test]
    fn subtracting_from_nothing_or_everything_leaves_no_limit() {
        let t = terrain();
        let mut s = Selection::default();
        s.apply(square(10.0, 20.0), Op::Subtract, false, &t);
        assert!(!s.is_active(), "nothing to subtract from");
        s.apply(square(10.0, 20.0), Op::Add, true, &t);
        s.apply(square(0.0, 64.0), Op::Subtract, false, &t);
        assert!(!s.is_active(), "all subtracted: no limit, not a dead brush");
    }

    #[test]
    fn clicks_and_slivers_have_no_area() {
        assert!(!Shape::rect(Vec2::ONE, Vec2::ONE).has_area(2.0));
        assert!(!Shape::Lasso(vec![Vec2::ZERO, Vec2::ONE]).has_area(2.0));
        assert!(square(0.0, 4.0).has_area(2.0));
    }
}
