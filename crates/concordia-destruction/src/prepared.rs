//! A loaded breakable indexed for the runtime: piece masses and centres, and each bond's pieces
//! and centre.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_core::math::Vec3;

use crate::Loaded;

/// Per-recipe lookups.
pub(crate) struct Prepared {
    pub loaded: Arc<Loaded>,
    /// Piece index of each cell.
    pub piece_of_cell: BTreeMap<u32, u32>,
    /// Each bond's two pieces.
    pub bond_pieces: Vec<(u32, u32)>,
    /// Each bond's centre: the midpoint of its pieces' centres (mesh space).
    pub bond_centre: Vec<Vec3>,
}

impl Prepared {
    pub fn new(loaded: Arc<Loaded>) -> Self {
        let piece_of_cell: BTreeMap<u32, u32> = loaded
            .fracture
            .pieces
            .iter()
            .enumerate()
            .map(|(i, p)| (p.cell, i as u32))
            .collect();
        let bond_pieces: Vec<(u32, u32)> = loaded
            .structure
            .edges
            .iter()
            .map(|e| (piece_of_cell[&e.a], piece_of_cell[&e.b]))
            .collect();
        let centre = |p: u32| centre_of(&loaded, p);
        let bond_centre = bond_pieces
            .iter()
            .map(|&(a, b)| (centre(a) + centre(b)) * 0.5)
            .collect();
        Self {
            loaded,
            piece_of_cell,
            bond_pieces,
            bond_centre,
        }
    }

    /// A piece's mass.
    pub fn mass(&self, piece: u32) -> f64 {
        self.loaded.structure.pieces[piece as usize].mass
    }

    /// A piece's centre of mass (mesh space).
    pub fn centre(&self, piece: u32) -> Vec3 {
        centre_of(&self.loaded, piece)
    }

    /// The pieces' total mass and centre of mass.
    pub fn mass_and_centre(&self, pieces: &[u32]) -> (f64, Vec3) {
        let mass: f64 = pieces.iter().map(|&p| self.mass(p)).sum();
        let mut moment = [0.0f64; 3];
        for &p in pieces {
            let c = self.loaded.structure.pieces[p as usize].centre;
            for k in 0..3 {
                moment[k] += self.mass(p) * c[k];
            }
        }
        let centre = match mass > 0.0 {
            true => Vec3::from_array(moment.map(|m| (m / mass) as f32)),
            false => Vec3::ZERO,
        };
        (mass, centre)
    }

    /// Bonds with both pieces in `pieces` (sorted), as bond indices.
    pub fn internal_bonds<'a>(&'a self, pieces: &'a [u32]) -> impl Iterator<Item = usize> + 'a {
        self.bond_pieces
            .iter()
            .enumerate()
            .filter(move |(_, (a, b))| {
                pieces.binary_search(a).is_ok() && pieces.binary_search(b).is_ok()
            })
            .map(|(k, _)| k)
    }

    /// The pieces' points (mesh space), one hull per piece.
    pub fn hulls(&self, pieces: &[u32]) -> Vec<Vec<Vec3>> {
        pieces
            .iter()
            .map(|&p| {
                self.loaded.fracture.pieces[p as usize]
                    .positions
                    .iter()
                    .map(|v| Vec3::from_array(*v))
                    .collect()
            })
            .collect()
    }
}

fn centre_of(loaded: &Loaded, piece: u32) -> Vec3 {
    Vec3::from_array(
        loaded.structure.pieces[piece as usize]
            .centre
            .map(|c| c as f32),
    )
}
