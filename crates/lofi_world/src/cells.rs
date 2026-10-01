//! Per-cell flags and how cells split into triangles.

/// One byte per cell. Bits a newer tool defines are kept as they are, so a
/// save from this one doesn't drop them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellFlags(pub u8);

const HOLE: u8 = 1;
const DIAGONAL_SHIFT: u8 = 1;
const DIAGONAL_MASK: u8 = 0b11 << DIAGONAL_SHIFT;
const NO_CLUTTER: u8 = 1 << 3;
const UNWALKABLE: u8 = 1 << 4;

/// Which way a cell is cut. `Main` joins its (0, 0) and (1, 1) corners,
/// `Anti` its (1, 0) and (0, 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Diagonal {
    /// Left to the map's [`Triangulation`].
    #[default]
    Auto,
    Main,
    Anti,
}

impl CellFlags {
    #[must_use]
    pub const fn hole(self) -> bool {
        self.0 & HOLE != 0
    }

    #[must_use]
    pub const fn diagonal(self) -> Diagonal {
        match (self.0 & DIAGONAL_MASK) >> DIAGONAL_SHIFT {
            1 => Diagonal::Main,
            2 => Diagonal::Anti,
            // 3 is unassigned; read it as auto.
            _ => Diagonal::Auto,
        }
    }

    /// Ground clutter (grass, pebbles) stays off this cell.
    #[must_use]
    pub const fn no_clutter(self) -> bool {
        self.0 & NO_CLUTTER != 0
    }

    #[must_use]
    pub const fn unwalkable(self) -> bool {
        self.0 & UNWALKABLE != 0
    }

    #[must_use]
    pub const fn with_hole(self, on: bool) -> Self {
        self.with_bit(HOLE, on)
    }

    #[must_use]
    pub const fn with_diagonal(self, diagonal: Diagonal) -> Self {
        let bits = match diagonal {
            Diagonal::Auto => 0,
            Diagonal::Main => 1,
            Diagonal::Anti => 2,
        };
        Self(self.0 & !DIAGONAL_MASK | bits << DIAGONAL_SHIFT)
    }

    #[must_use]
    pub const fn with_no_clutter(self, on: bool) -> Self {
        self.with_bit(NO_CLUTTER, on)
    }

    #[must_use]
    pub const fn with_unwalkable(self, on: bool) -> Self {
        self.with_bit(UNWALKABLE, on)
    }

    const fn with_bit(self, bit: u8, on: bool) -> Self {
        if on {
            Self(self.0 | bit)
        } else {
            Self(self.0 & !bit)
        }
    }
}

/// Twist or bend smaller than this is a tie, cut by the checkerboard. Without
/// it, float noise from smoothing would flip near-planar cells back and forth.
/// Part of [`Triangulation::FollowTerrain`]'s definition; don't change it.
const TIE_METRES: f32 = 0.001;

/// How cells left on [`Diagonal::Auto`] are cut. Stored per map, so a map
/// keeps the shape it was sculpted with. Each rule is a file-format promise:
/// a new way of cutting is a new variant, never a change to an old one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Triangulation {
    /// Alternating diagonals. Maps from before 0.3 used this.
    #[default]
    Checkerboard,
    /// Cut so the crease bends the way the ground around it does: up on
    /// ridges and hilltops, down in valleys. Edges then run along ridges
    /// instead of zigzagging across them.
    FollowTerrain,
}

impl Triangulation {
    /// The number stored in map files.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Checkerboard => 0,
            Self::FollowTerrain => 1,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Checkerboard),
            1 => Some(Self::FollowTerrain),
            _ => None,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Checkerboard => "Checkerboard",
            Self::FollowTerrain => "Follow terrain",
        }
    }

    /// Whether a cell with corner heights `a` (0, 0), `b` (1, 0), `c` (0, 1)
    /// and `d` (1, 1) is cut along its main diagonal.
    ///
    /// `curvature` is the summed Laplacian of the corners: negative where the
    /// ground is convex. Only called under [`Self::FollowTerrain`].
    #[must_use]
    pub fn splits_on_main(
        self,
        flags: CellFlags,
        (cx, cz): (u32, u32),
        [a, b, c, d]: [f32; 4],
        curvature: impl FnOnce() -> f32,
    ) -> bool {
        let parity = (cx + cz).is_multiple_of(2);
        match flags.diagonal() {
            Diagonal::Main => true,
            Diagonal::Anti => false,
            Diagonal::Auto => match self {
                Self::Checkerboard => parity,
                Self::FollowTerrain => {
                    // Cutting main creases the cell along a–d: a ridge if that
                    // pair is the higher one, a valley if lower.
                    let twist = (a + d) - (b + c);
                    if twist.abs() < TIE_METRES {
                        return parity;
                    }
                    let bend = curvature();
                    if bend.abs() < TIE_METRES {
                        parity
                    } else {
                        (twist > 0.0) == (bend < 0.0)
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_round_trip_and_keep_unknown_bits() {
        let reserved = CellFlags(0b1110_0000);
        let f = reserved
            .with_hole(true)
            .with_diagonal(Diagonal::Anti)
            .with_no_clutter(true)
            .with_unwalkable(true);
        assert!(f.hole() && f.no_clutter() && f.unwalkable());
        assert_eq!(f.diagonal(), Diagonal::Anti);
        assert_eq!(f.0 & 0b1110_0000, 0b1110_0000);
        let cleared = f
            .with_hole(false)
            .with_diagonal(Diagonal::Auto)
            .with_no_clutter(false)
            .with_unwalkable(false);
        assert_eq!(cleared, reserved);
    }

    #[test]
    fn unassigned_diagonal_reads_as_auto() {
        assert_eq!(CellFlags(0b110).diagonal(), Diagonal::Auto);
    }

    const RULES: [Triangulation; 2] = [Triangulation::Checkerboard, Triangulation::FollowTerrain];

    #[test]
    fn forced_diagonals_win() {
        let flat = [0.0; 4];
        for rule in RULES {
            let main = CellFlags::default().with_diagonal(Diagonal::Main);
            let anti = CellFlags::default().with_diagonal(Diagonal::Anti);
            assert!(rule.splits_on_main(main, (1, 0), flat, || -1.0));
            assert!(!rule.splits_on_main(anti, (0, 0), flat, || -1.0));
        }
    }

    #[test]
    fn follow_terrain_creases_along_ridges_and_valleys() {
        let auto = CellFlags::default();
        let rule = Triangulation::FollowTerrain;
        // Both cells sit where checkerboard would cut the other way.
        // A ridge from (0, 0) to (1, 1) on convex ground.
        assert!(rule.splits_on_main(auto, (1, 0), [5.0, 0.0, 0.0, 5.0], || -10.0));
        // The same corners in a valley along (1, 0)–(0, 1).
        assert!(!rule.splits_on_main(auto, (0, 0), [5.0, 0.0, 0.0, 5.0], || 10.0));
    }

    #[test]
    fn follow_terrain_falls_back_to_checkerboard_on_planes_and_saddles() {
        let auto = CellFlags::default();
        for cell in [(0, 0), (1, 0), (3, 4)] {
            let checker = Triangulation::Checkerboard.splits_on_main(auto, cell, [0.0; 4], || 0.0);
            // A tilted plane has no twist.
            let plane = [0.0, 1.0, 2.0, 3.0];
            assert_eq!(
                Triangulation::FollowTerrain.splits_on_main(auto, cell, plane, || -3.0),
                checker
            );
            // A saddle has twist but no overall bend.
            let saddle = [1.0, -1.0, -1.0, 1.0];
            assert_eq!(
                Triangulation::FollowTerrain.splits_on_main(auto, cell, saddle, || 0.0),
                checker
            );
            // Float noise is a tie too.
            assert_eq!(
                Triangulation::FollowTerrain.splits_on_main(auto, cell, saddle, || 1e-5),
                checker
            );
        }
    }

    #[test]
    fn codes_round_trip() {
        for rule in RULES {
            assert_eq!(Triangulation::from_code(rule.code()), Some(rule));
        }
        assert_eq!(Triangulation::from_code(2), None);
    }
}
