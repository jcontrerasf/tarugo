//! The document: materials, parameters, parts and screws.
//!
//! Every part is an axis-aligned box (X width, Y height, Z depth; the front of the furniture
//! looks towards +Z). On each axis a part either spans between two [`End`]s or has the size
//! its material fixes, placed by one anchor. An end can be absolute or refer to a face of
//! another part, which is how dimensions, snapping and parametric designs are all stored.

use serde::{Deserialize, Serialize};

use crate::expr::{self, format_number};
use crate::material::{MATERIAL_COLORS, Material, MaterialId, MaterialKind};
use crate::shape::Shape;

pub type PartId = u32;
pub type ScrewId = u32;

pub const FILE_VERSION: u32 = 1;
pub const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

/// A length typed by the user: a number or a formula over the parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Dim(pub String);

impl Dim {
    pub fn num(v: f64) -> Dim {
        Dim(format_number(v))
    }

    pub fn zero() -> Dim {
        Dim("0".into())
    }

    /// Is this a formula (not just a number)?
    pub fn is_formula(&self) -> bool {
        !matches!(expr::parse(&self.0), Ok(expr::Expr::Num(_)))
            && !matches!(expr::parse(&self.0), Ok(expr::Expr::Neg(e)) if matches!(*e, expr::Expr::Num(_)))
    }
}

impl From<f64> for Dim {
    fn from(v: f64) -> Dim {
        Dim::num(v)
    }
}

impl From<&str> for Dim {
    fn from(s: &str) -> Dim {
        Dim(s.to_string())
    }
}

/// Which face of a part along an axis (or its middle plane).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Min,
    Center,
    Max,
}

impl Side {
    pub fn of(self, lo: f64, hi: f64) -> f64 {
        match self {
            Side::Min => lo,
            Side::Center => (lo + hi) / 2.0,
            Side::Max => hi,
        }
    }

    pub fn opposite(self) -> Side {
        match self {
            Side::Min => Side::Max,
            Side::Center => Side::Center,
            Side::Max => Side::Min,
        }
    }
}

/// A face of another part on the same axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FaceRef {
    pub part: PartId,
    pub side: Side,
}

/// Where one end of a part is along an axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum End {
    /// A coordinate.
    Abs { value: Dim },
    /// At a face of another part, plus an offset.
    Ref {
        #[serde(flatten)]
        face: FaceRef,
        offset: Dim,
    },
    /// At a fraction of the way between two faces, plus an offset (equal divisions).
    Between {
        lo: FaceRef,
        hi: FaceRef,
        frac: Dim,
        offset: Dim,
    },
}

impl End {
    pub fn abs(v: impl Into<Dim>) -> End {
        End::Abs { value: v.into() }
    }

    pub fn at(part: PartId, side: Side) -> End {
        End::Ref {
            face: FaceRef { part, side },
            offset: Dim::zero(),
        }
    }

    pub fn at_offset(part: PartId, side: Side, offset: impl Into<Dim>) -> End {
        End::Ref {
            face: FaceRef { part, side },
            offset: offset.into(),
        }
    }

    /// Parts this end refers to.
    pub fn refs(&self) -> impl Iterator<Item = PartId> {
        let (a, b) = match self {
            End::Abs { .. } => (None, None),
            End::Ref { face, .. } => (Some(face.part), None),
            End::Between { lo, hi, .. } => (Some(lo.part), Some(hi.part)),
        };
        a.into_iter().chain(b)
    }

    pub fn dims_mut(&mut self) -> Vec<&mut Dim> {
        match self {
            End::Abs { value } => vec![value],
            End::Ref { offset, .. } => vec![offset],
            End::Between { frac, offset, .. } => vec![frac, offset],
        }
    }
}

/// How a part occupies one axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Extent {
    /// Free size: from `lo` to `hi`.
    Span { lo: End, hi: End },
    /// Size fixed by the material; `align` says which face (or the middle) sits at `anchor`.
    Thick { anchor: End, align: Side },
}

impl Extent {
    pub fn is_thick(&self) -> bool {
        matches!(self, Extent::Thick { .. })
    }

    pub fn ends(&self) -> Vec<&End> {
        match self {
            Extent::Span { lo, hi } => vec![lo, hi],
            Extent::Thick { anchor, .. } => vec![anchor],
        }
    }

    pub fn ends_mut(&mut self) -> Vec<&mut End> {
        match self {
            Extent::Span { lo, hi } => vec![lo, hi],
            Extent::Thick { anchor, .. } => vec![anchor],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub name: String,
    pub material: MaterialId,
    /// X, Y, Z.
    pub axes: [Extent; 3],
    /// A stick lying with its section turned 90°.
    #[serde(default)]
    pub swap_section: bool,
    /// Free-form outline (corners, notches, slots, holes); the cut list ignores it.
    #[serde(default)]
    pub shape: Option<Shape>,
    /// Kept as it is: the interface does not move, edit or delete it. It still follows
    /// the parts and parameters it refers to.
    #[serde(default)]
    pub locked: bool,
}

impl Part {
    /// Axes whose size the material fixes.
    pub fn thick_axes(&self) -> Vec<usize> {
        (0..3).filter(|&a| self.axes[a].is_thick()).collect()
    }

    /// The axis the face of the part is perpendicular to: its first fixed axis.
    pub fn normal_axis(&self) -> Option<usize> {
        self.thick_axes().first().copied()
    }

    /// Position of `axis` among the fixed axes (which fixed size of the material it takes).
    pub fn thick_index(&self, axis: usize) -> usize {
        (0..axis).filter(|&a| self.axes[a].is_thick()).count()
    }
}

/// A named value usable in any dimension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub value: Dim,
}

/// A screw through one part into another, placed on their joint: `u` and `v` are fractions
/// (0–1) of the contact rectangle along its two axes (in axis order).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screw {
    pub id: ScrewId,
    pub through: PartId,
    pub into: PartId,
    pub u: f64,
    pub v: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Room a drill with its bit needs behind a screw (mm).
    pub drill_length: f64,
    /// Width of the body of the drill (mm): it needs half of it free around a screw.
    pub drill_width: f64,
    /// Width of the saw cut (mm), for the stock estimate.
    pub kerf: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            drill_length: 220.0,
            drill_width: 60.0,
            kerf: 3.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub materials: Vec<Material>,
    /// Material of the frame and of new parts.
    pub main_material: MaterialId,
    pub params: Vec<Param>,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub screws: Vec<Screw>,
    #[serde(default)]
    pub settings: Settings,
    /// The depth was confirmed after the front sketch.
    #[serde(default)]
    pub depth_set: bool,
    next_id: u32,
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

/// Names of the parameters the front sketch creates.
pub const WIDTH: &str = "W";
pub const HEIGHT: &str = "H";
pub const DEPTH: &str = "D";

impl Document {
    /// An empty document with one 18 mm board as main material.
    pub fn new() -> Document {
        let mut doc = Document {
            version: FILE_VERSION,
            materials: Vec::new(),
            main_material: 0,
            params: Vec::new(),
            parts: Vec::new(),
            screws: Vec::new(),
            settings: Settings::default(),
            depth_set: false,
            next_id: 1,
        };
        doc.main_material = doc.add_material(
            String::new(),
            MaterialKind::Board {
                thickness: 18.0,
                stock_length: None,
                stock_width: None,
            },
        );
        doc
    }

    pub fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_material(&mut self, name: String, kind: MaterialKind) -> MaterialId {
        let id = self.new_id();
        let color = MATERIAL_COLORS[self.materials.len() % MATERIAL_COLORS.len()];
        self.materials.push(Material {
            id,
            name,
            kind,
            color,
        });
        id
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    pub fn material_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.iter_mut().find(|m| m.id == id)
    }

    /// Is any part made of this material?
    pub fn material_in_use(&self, id: MaterialId) -> bool {
        self.parts.iter().any(|p| p.material == id)
    }

    pub fn part(&self, id: PartId) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }

    pub fn part_mut(&mut self, id: PartId) -> Option<&mut Part> {
        self.parts.iter_mut().find(|p| p.id == id)
    }

    pub fn add_part(&mut self, name: String, material: MaterialId, axes: [Extent; 3]) -> PartId {
        let id = self.new_id();
        self.parts.push(Part {
            id,
            name,
            material,
            axes,
            swap_section: false,
            locked: false,
            shape: None,
        });
        id
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Sets a parameter, creating it if needed.
    pub fn set_param(&mut self, name: &str, value: Dim) {
        match self.params.iter_mut().find(|p| p.name == name) {
            Some(p) => p.value = value,
            None => self.params.push(Param {
                name: name.to_string(),
                value,
            }),
        }
    }

    /// A parameter name not in use, starting from `base`.
    pub fn free_param_name(&self, base: &str) -> String {
        if self.param(base).is_none() {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}{n}"))
            .find(|n| self.param(n).is_none())
            .unwrap_or_default()
    }

    /// Every dimension of the document (to rename parameters or look for their uses).
    pub fn dims_mut(&mut self) -> Vec<&mut Dim> {
        let mut out: Vec<&mut Dim> = Vec::new();
        for p in &mut self.params {
            out.push(&mut p.value);
        }
        for part in &mut self.parts {
            for extent in &mut part.axes {
                for end in extent.ends_mut() {
                    out.extend(end.dims_mut());
                }
            }
            if let Some(shape) = &mut part.shape {
                out.extend(shape.dims_mut());
            }
        }
        out
    }

    /// Renames a parameter everywhere it is used. False if the new name is invalid or taken.
    pub fn rename_param(&mut self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        if !expr::is_valid_name(to) || self.param(to).is_some() || self.param(from).is_none() {
            return false;
        }
        for dim in self.dims_mut() {
            dim.0 = expr::rename(&dim.0, from, to);
        }
        for p in &mut self.params {
            if p.name == from {
                p.name = to.to_string();
            }
        }
        true
    }

    /// Is the parameter used by any dimension?
    pub fn param_in_use(&mut self, name: &str) -> bool {
        self.dims_mut().iter().any(|d| {
            let mut names = Vec::new();
            if let Ok(e) = expr::parse(&d.0) {
                e.names(&mut names);
            }
            names.iter().any(|n| n == name)
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Document, String> {
        let doc: Document = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if doc.version > FILE_VERSION {
            return Err(format!(
                "file version {} is newer than this program",
                doc.version
            ));
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let mut doc = Document::new();
        doc.set_param(WIDTH, Dim::num(800.0));
        let m = doc.main_material;
        let a = doc.add_part(
            "side".into(),
            m,
            [
                Extent::Thick {
                    anchor: End::abs(0.0),
                    align: Side::Min,
                },
                Extent::Span {
                    lo: End::abs(0.0),
                    hi: End::abs("H"),
                },
                Extent::Span {
                    lo: End::abs(0.0),
                    hi: End::abs(300.0),
                },
            ],
        );
        doc.add_part(
            "shelf".into(),
            m,
            [
                Extent::Span {
                    lo: End::at(a, Side::Max),
                    hi: End::abs("W"),
                },
                Extent::Thick {
                    anchor: End::Between {
                        lo: FaceRef {
                            part: a,
                            side: Side::Min,
                        },
                        hi: FaceRef {
                            part: a,
                            side: Side::Max,
                        },
                        frac: Dim("1/2".into()),
                        offset: Dim::zero(),
                    },
                    align: Side::Center,
                },
                Extent::Span {
                    lo: End::abs(0.0),
                    hi: End::abs(300.0),
                },
            ],
        );
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn renaming_a_parameter_updates_its_uses() {
        let mut doc = Document::new();
        doc.set_param("W", Dim::num(800.0));
        doc.set_param("half", Dim("W / 2".into()));
        assert!(doc.rename_param("W", "width"));
        assert_eq!(doc.param("half").unwrap().value.0, "width / 2");
        assert!(!doc.rename_param("width", "half"));
        assert!(!doc.rename_param("width", "max"));
        assert!(doc.param_in_use("width"));
        assert!(!doc.param_in_use("half"));
    }

    #[test]
    fn formulas_are_told_from_numbers() {
        assert!(!Dim::num(12.5).is_formula());
        assert!(!Dim("-3".into()).is_formula());
        assert!(Dim("W/2".into()).is_formula());
        assert!(Dim("1+1".into()).is_formula());
    }
}
