//! Materials: boards (sheets, with a thickness) and sticks (lumber, with a section).

use serde::{Deserialize, Serialize};

pub type MaterialId = u32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MaterialKind {
    /// A sheet. The stock size is only needed to estimate how many sheets to buy.
    Board {
        thickness: f64,
        #[serde(default)]
        stock_length: Option<f64>,
        #[serde(default)]
        stock_width: Option<f64>,
    },
    /// Lumber with a rectangular section.
    Stick {
        width: f64,
        height: f64,
        #[serde(default)]
        stock_length: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    #[serde(flatten)]
    pub kind: MaterialKind,
    /// Color in the 3D view (sRGB).
    pub color: [u8; 3],
}

impl Material {
    pub fn is_board(&self) -> bool {
        matches!(self.kind, MaterialKind::Board { .. })
    }

    /// Size of the `n`-th axis fixed by the material: the thickness of a board; the width and
    /// then the height of a stick (the other way around if `swapped`).
    pub fn fixed_size(&self, n: usize, swapped: bool) -> f64 {
        match self.kind {
            MaterialKind::Board { thickness, .. } => thickness,
            MaterialKind::Stick { width, height, .. } => {
                if (n == 0) != swapped {
                    width
                } else {
                    height
                }
            }
        }
    }

    /// The smallest fixed size (what a joint against this material's edge is as wide as).
    pub fn thickness(&self) -> f64 {
        match self.kind {
            MaterialKind::Board { thickness, .. } => thickness,
            MaterialKind::Stick { width, height, .. } => width.min(height),
        }
    }

    /// Are the mandatory sizes there (positive)?
    pub fn is_valid(&self) -> bool {
        match self.kind {
            MaterialKind::Board { thickness, .. } => thickness > 0.0,
            MaterialKind::Stick { width, height, .. } => width > 0.0 && height > 0.0,
        }
    }
}

/// Colors given to new materials, in turn.
pub const MATERIAL_COLORS: [[u8; 3]; 6] = [
    [0xe3, 0xc7, 0x9a],
    [0xf2, 0xf0, 0xea],
    [0xb9, 0x8a, 0x5b],
    [0x8c, 0x8f, 0x97],
    [0xd9, 0xa8, 0x6c],
    [0x6f, 0x4e, 0x37],
];
