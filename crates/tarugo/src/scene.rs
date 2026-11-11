//! What is computed from the document every time it changes: the solved boxes, joints,
//! screws in space and the issues found.

use std::collections::{HashMap, HashSet};

use glam::Vec3;
use tarugo_core::analysis::{self, Issue, ScrewPlace};
use tarugo_core::joints::{Joint, find_joints};
use tarugo_core::model::{Document, PartId};
use tarugo_core::shape::Outline;
use tarugo_core::solve::{Aabb, Solved, solve};

use crate::view3d::mesh::vec3;

#[derive(Default)]
pub struct Scene {
    pub solved: Solved,
    pub joints: Vec<Joint>,
    pub places: Vec<ScrewPlace>,
    /// Broken parts, overlaps, features that do not fit.
    pub geometry: Vec<Issue>,
    /// Screws and drill access.
    pub fastening: Vec<Issue>,
    /// Parts to paint as wrong.
    pub bad: HashSet<PartId>,
    /// Free-form outlines, with the axis they are extruded along.
    pub outlines: HashMap<PartId, (usize, Outline)>,
}

impl Scene {
    pub fn build(doc: &Document) -> Scene {
        let solved = solve(doc);
        let joints = find_joints(doc, &solved);
        let places = analysis::screw_places(doc, &solved, &joints);
        let geometry = analysis::check_geometry(doc, &solved);
        let fastening = analysis::check_fastening(doc, &solved);
        let bad = geometry.iter().flat_map(|i| i.parts()).collect();
        let mut outlines = HashMap::new();
        for part in &doc.parts {
            if let (Some(shape), Some(b), Some(axis)) =
                (&part.shape, solved.get(part.id), part.normal_axis())
                && !shape.is_plain()
                && let Some((su, sv)) = analysis::face_size(part, b)
            {
                let outline = shape.outline(su, sv, &|d| solved.eval(d));
                outlines.insert(part.id, (axis, outline));
            }
        }
        Scene {
            solved,
            joints,
            places,
            geometry,
            fastening,
            bad,
            outlines,
        }
    }

    pub fn get(&self, part: PartId) -> Option<&Aabb> {
        self.solved.get(part)
    }

    /// Solved boxes in document order.
    pub fn boxes<'a>(&'a self, doc: &'a Document) -> impl Iterator<Item = (PartId, &'a Aabb)> {
        doc.parts
            .iter()
            .filter_map(|p| Some((p.id, self.solved.get(p.id)?)))
    }

    /// Corners of the box around everything.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let b = self.solved.bounds()?;
        Some((vec3(b.min), vec3(b.max)))
    }
}
