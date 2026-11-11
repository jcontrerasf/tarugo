//! Tools of the 3D view and the logic behind them, kept apart from the drawing.

pub mod add;
pub mod divide;
pub mod drag;
pub mod screw;
pub mod snap;

use tarugo_core::material::MaterialId;

use crate::i18n::t;

pub use drag::{Drag, DragKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    /// Divide a cell of the front with a board placed along this axis (0: vertical board).
    Divide(usize),
    AddBoard,
    AddStick,
    Screw,
}

impl Tool {
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => t().hint_select,
            Tool::Divide(_) => t().hint_divide,
            Tool::AddBoard | Tool::AddStick => t().hint_add,
            Tool::Screw => t().hint_screw,
        }
    }
}

/// Which face of the part at the end of the cell a division is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefChoice {
    Inner,
    Center,
    Outer,
}

/// Which face of the new board the dimension reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignChoice {
    Near,
    Center,
    Far,
}

#[derive(Clone, Copy, Debug)]
pub struct DivideOptions {
    pub reference: RefChoice,
    pub align: AlignChoice,
    /// Divide in equal spaces instead of placing one board.
    pub equal: bool,
    /// Number of equal spaces.
    pub spaces: usize,
}

impl Default for DivideOptions {
    fn default() -> Self {
        DivideOptions {
            reference: RefChoice::Inner,
            align: AlignChoice::Near,
            equal: false,
            spaces: 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AddOptions {
    pub material: Option<MaterialId>,
    /// For a board: the axis of its thickness. For a stick: the axis of its length.
    pub axis: usize,
}

impl Default for AddOptions {
    fn default() -> Self {
        AddOptions {
            material: None,
            axis: 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScrewOptions {
    /// Screws placed per click, spread along the joint.
    pub count: usize,
}

impl Default for ScrewOptions {
    fn default() -> Self {
        ScrewOptions { count: 2 }
    }
}
