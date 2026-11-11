//! Turns the document's dimensions into boxes: evaluates the parameters and resolves every
//! end that refers to another part. A broken part (bad formula, missing reference, cycle)
//! gets an error and does not stop the others.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::expr::{self, ExprError};
use crate::model::{Dim, Document, End, Extent, FaceRef, Part, PartId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    pub fn center(&self) -> [f64; 3] {
        [
            (self.max[0] + self.min[0]) / 2.0,
            (self.max[1] + self.min[1]) / 2.0,
            (self.max[2] + self.min[2]) / 2.0,
        ]
    }

    pub fn union(&self, other: &Aabb) -> Aabb {
        let mut out = *self;
        for a in 0..3 {
            out.min[a] = out.min[a].min(other.min[a]);
            out.max[a] = out.max[a].max(other.max[a]);
        }
        out
    }

    /// How much the two boxes share along `axis` (negative: the gap between them).
    pub fn overlap(&self, other: &Aabb, axis: usize) -> f64 {
        self.max[axis].min(other.max[axis]) - self.min[axis].max(other.min[axis])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SolveError {
    /// A dimension that cannot be evaluated.
    Expr(ExprError),
    /// A reference to a part that does not exist.
    MissingPart(PartId),
    /// Parts that refer to each other in a loop.
    Cycle,
    MissingMaterial,
}

impl From<ExprError> for SolveError {
    fn from(e: ExprError) -> Self {
        SolveError::Expr(e)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Solved {
    pub params: BTreeMap<String, Result<f64, ExprError>>,
    pub boxes: HashMap<PartId, Aabb>,
    pub errors: HashMap<PartId, SolveError>,
    /// Parts with an end before its start on some axis (shown with the ends swapped).
    pub inverted: HashSet<PartId>,
}

impl Solved {
    /// Value of a dimension with the solved parameters.
    pub fn eval(&self, dim: &Dim) -> Result<f64, ExprError> {
        expr::parse(&dim.0)?.eval(&mut |name| match self.params.get(name) {
            Some(v) => v.clone(),
            None => Err(ExprError::UnknownName(name.to_string())),
        })
    }

    pub fn get(&self, part: PartId) -> Option<&Aabb> {
        self.boxes.get(&part)
    }

    /// Box around every solved part.
    pub fn bounds(&self) -> Option<Aabb> {
        let mut boxes = self.boxes.values();
        let first = *boxes.next()?;
        Some(boxes.fold(first, |acc, b| acc.union(b)))
    }
}

fn solve_params(doc: &Document) -> BTreeMap<String, Result<f64, ExprError>> {
    fn value(
        doc: &Document,
        name: &str,
        done: &mut BTreeMap<String, Result<f64, ExprError>>,
        stack: &mut Vec<String>,
    ) -> Result<f64, ExprError> {
        if let Some(v) = done.get(name) {
            return v.clone();
        }
        let Some(param) = doc.param(name) else {
            return Err(ExprError::UnknownName(name.to_string()));
        };
        if stack.iter().any(|n| n == name) {
            return Err(ExprError::Cycle(name.to_string()));
        }
        stack.push(name.to_string());
        let result =
            expr::parse(&param.value.0).and_then(|e| e.eval(&mut |n| value(doc, n, done, stack)));
        stack.pop();
        done.insert(name.to_string(), result.clone());
        result
    }
    let mut done = BTreeMap::new();
    for p in &doc.params {
        let _ = value(doc, &p.name, &mut done, &mut Vec::new());
    }
    done
}

enum State {
    InProgress,
    Done(Result<(f64, f64, bool), SolveError>),
}

struct Solver<'a> {
    doc: &'a Document,
    solved: Solved,
    index: HashMap<PartId, &'a Part>,
    memo: HashMap<(PartId, usize), State>,
}

impl Solver<'_> {
    /// `(lo, hi, inverted)` of a part along an axis.
    fn interval(&mut self, part: PartId, axis: usize) -> Result<(f64, f64, bool), SolveError> {
        match self.memo.get(&(part, axis)) {
            Some(State::Done(r)) => return r.clone(),
            Some(State::InProgress) => return Err(SolveError::Cycle),
            None => {}
        }
        self.memo.insert((part, axis), State::InProgress);
        let result = self.compute(part, axis);
        self.memo.insert((part, axis), State::Done(result.clone()));
        result
    }

    fn compute(&mut self, id: PartId, axis: usize) -> Result<(f64, f64, bool), SolveError> {
        let part = *self.index.get(&id).ok_or(SolveError::MissingPart(id))?;
        match &part.axes[axis] {
            Extent::Span { lo, hi } => {
                let (lo, hi) = (self.end(lo, axis)?, self.end(hi, axis)?);
                Ok(if lo <= hi {
                    (lo, hi, false)
                } else {
                    (hi, lo, true)
                })
            }
            Extent::Thick { anchor, align } => {
                let material = self
                    .doc
                    .material(part.material)
                    .ok_or(SolveError::MissingMaterial)?;
                let size = material.fixed_size(part.thick_index(axis), part.swap_section);
                let at = self.end(anchor, axis)?;
                let lo = match align {
                    crate::model::Side::Min => at,
                    crate::model::Side::Center => at - size / 2.0,
                    crate::model::Side::Max => at - size,
                };
                Ok((lo, lo + size, false))
            }
        }
    }

    fn face(&mut self, face: &FaceRef, axis: usize) -> Result<f64, SolveError> {
        let (lo, hi, _) = self.interval(face.part, axis)?;
        Ok(face.side.of(lo, hi))
    }

    fn end(&mut self, end: &End, axis: usize) -> Result<f64, SolveError> {
        Ok(match end {
            End::Abs { value } => self.solved.eval(value)?,
            End::Ref { face, offset } => self.face(face, axis)? + self.solved.eval(offset)?,
            End::Between {
                lo,
                hi,
                frac,
                offset,
            } => {
                let (a, b) = (self.face(lo, axis)?, self.face(hi, axis)?);
                a + (b - a) * self.solved.eval(frac)? + self.solved.eval(offset)?
            }
        })
    }
}

pub fn solve(doc: &Document) -> Solved {
    let mut solver = Solver {
        doc,
        solved: Solved {
            params: solve_params(doc),
            ..Solved::default()
        },
        index: doc.parts.iter().map(|p| (p.id, p)).collect(),
        memo: HashMap::new(),
    };
    for part in &doc.parts {
        let mut aabb = Aabb {
            min: [0.0; 3],
            max: [0.0; 3],
        };
        let mut error = None;
        for axis in 0..3 {
            match solver.interval(part.id, axis) {
                Ok((lo, hi, inverted)) => {
                    aabb.min[axis] = lo;
                    aabb.max[axis] = hi;
                    if inverted {
                        solver.solved.inverted.insert(part.id);
                    }
                }
                Err(e) => {
                    error.get_or_insert(e);
                }
            }
        }
        match error {
            Some(e) => {
                solver.solved.errors.insert(part.id, e);
            }
            None => {
                solver.solved.boxes.insert(part.id, aabb);
            }
        }
    }
    solver.solved
}

/// Does the position of `part` along `axis` depend (directly or not) on `on`?
pub fn depends_on(doc: &Document, part: PartId, axis: usize, on: PartId) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![part];
    while let Some(id) = stack.pop() {
        if id == on {
            return true;
        }
        if !seen.insert(id) {
            continue;
        }
        if let Some(p) = doc.part(id) {
            for end in p.axes[axis].ends() {
                stack.extend(end.refs());
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dim, Side};

    fn span(lo: End, hi: End) -> Extent {
        Extent::Span { lo, hi }
    }

    fn thick(anchor: End, align: Side) -> Extent {
        Extent::Thick { anchor, align }
    }

    /// Two sides 800 apart (outside) and a shelf between them.
    fn shelf() -> (Document, PartId, PartId, PartId) {
        let mut doc = Document::new();
        doc.set_param("W", Dim::num(800.0));
        doc.set_param("H", Dim("W / 2".into()));
        let m = doc.main_material;
        let z = || span(End::abs(0.0), End::abs(300.0));
        let y = || span(End::abs(0.0), End::abs("H"));
        let left = doc.add_part("l".into(), m, [thick(End::abs(0.0), Side::Min), y(), z()]);
        let right = doc.add_part("r".into(), m, [thick(End::abs("W"), Side::Max), y(), z()]);
        let shelf = doc.add_part(
            "s".into(),
            m,
            [
                span(End::at(left, Side::Max), End::at(right, Side::Min)),
                thick(End::at_offset(left, Side::Min, "H / 2"), Side::Center),
                z(),
            ],
        );
        (doc, left, right, shelf)
    }

    #[test]
    fn references_resolve_and_follow_parameters() {
        let (mut doc, _, right, shelf) = shelf();
        let s = solve(&doc);
        assert!(s.errors.is_empty());
        let b = s.boxes[&shelf];
        assert_eq!((b.min[0], b.max[0]), (18.0, 782.0));
        assert_eq!((b.min[1], b.max[1]), (191.0, 209.0));
        assert_eq!(s.boxes[&right].min[0], 782.0);

        doc.set_param("W", Dim::num(1000.0));
        let s = solve(&doc);
        let b = s.boxes[&shelf];
        assert_eq!((b.min[0], b.max[0]), (18.0, 982.0));
        assert_eq!((b.min[1], b.max[1]), (241.0, 259.0));
    }

    #[test]
    fn between_places_equal_divisions() {
        let (mut doc, left, right, _) = shelf();
        let m = doc.main_material;
        let d = doc.add_part(
            "d".into(),
            m,
            [
                thick(
                    End::Between {
                        lo: FaceRef {
                            part: left,
                            side: Side::Max,
                        },
                        hi: FaceRef {
                            part: right,
                            side: Side::Min,
                        },
                        frac: Dim("1/2".into()),
                        offset: Dim::zero(),
                    },
                    Side::Center,
                ),
                span(End::abs(0.0), End::abs(100.0)),
                span(End::abs(0.0), End::abs(300.0)),
            ],
        );
        let b = solve(&doc).boxes[&d];
        assert_eq!((b.min[0], b.max[0]), (391.0, 409.0));
    }

    #[test]
    fn broken_parts_do_not_stop_the_rest() {
        let (mut doc, left, right, shelf) = shelf();
        // left now hangs from the shelf, which hangs from left: a cycle on X.
        doc.part_mut(left).unwrap().axes[0] = thick(End::at(shelf, Side::Min), Side::Max);
        let s = solve(&doc);
        assert_eq!(s.errors.get(&left), Some(&SolveError::Cycle));
        assert_eq!(s.errors.get(&shelf), Some(&SolveError::Cycle));
        assert!(s.boxes.contains_key(&right));
        assert!(depends_on(&doc, left, 0, shelf));
        assert!(!depends_on(&doc, right, 0, shelf));
    }

    #[test]
    fn bad_formulas_and_missing_parts() {
        let (mut doc, left, right, shelf) = shelf();
        doc.part_mut(left).unwrap().axes[1] = span(End::abs(0.0), End::abs("nope"));
        doc.part_mut(right).unwrap().axes[0] = thick(End::at(99, Side::Min), Side::Min);
        let s = solve(&doc);
        assert!(matches!(
            s.errors.get(&left),
            Some(SolveError::Expr(ExprError::UnknownName(_)))
        ));
        assert_eq!(s.errors.get(&right), Some(&SolveError::MissingPart(99)));
        // The shelf spans up to the broken right side.
        assert!(s.errors.contains_key(&shelf));
    }

    #[test]
    fn parameter_cycles_are_errors() {
        let mut doc = Document::new();
        doc.set_param("a", Dim("b + 1".into()));
        doc.set_param("b", Dim("a + 1".into()));
        doc.set_param("c", Dim::num(3.0));
        let s = solve(&doc);
        assert!(matches!(s.params["a"], Err(ExprError::Cycle(_))));
        assert_eq!(s.params["c"], Ok(3.0));
    }

    #[test]
    fn inverted_spans_are_flagged() {
        let mut doc = Document::new();
        let m = doc.main_material;
        let id = doc.add_part(
            "p".into(),
            m,
            [
                span(End::abs(100.0), End::abs(40.0)),
                thick(End::abs(0.0), Side::Min),
                span(End::abs(0.0), End::abs(10.0)),
            ],
        );
        let s = solve(&doc);
        assert!(s.inverted.contains(&id));
        assert_eq!(s.boxes[&id].min[0], 40.0);
    }
}
