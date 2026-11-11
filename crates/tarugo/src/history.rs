//! The open document with its file, selection and snapshot-based undo/redo.

use std::path::PathBuf;

use tarugo_core::model::{Document, PartId, ScrewId};

/// Undo steps kept.
const HISTORY_LIMIT: usize = 200;

pub struct Session {
    doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    /// Key of the grouped edit in progress (dragging a value, a gesture on the canvas).
    group: Option<String>,
    /// Changes every time the document may have changed (to solve and draw it again).
    revision: u64,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub selected: Option<PartId>,
    pub selected_screw: Option<ScrewId>,
}

impl Session {
    pub fn new(doc: Document, path: Option<PathBuf>) -> Session {
        Session {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
            revision: 0,
            path,
            dirty: false,
            selected: None,
            selected_screw: None,
        }
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// One undoable change.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Document) -> R) -> R {
        self.group = None;
        self.checkpoint();
        self.revision += 1;
        f(&mut self.doc)
    }

    /// Changes with the same `key` until [`Self::end_group`] are undone together.
    pub fn edit_grouped<R>(&mut self, key: &str, f: impl FnOnce(&mut Document) -> R) -> R {
        if self.group.as_deref() != Some(key) {
            self.checkpoint();
            self.group = Some(key.to_string());
        }
        self.revision += 1;
        self.dirty = true;
        f(&mut self.doc)
    }

    pub fn end_group(&mut self) {
        self.group = None;
    }

    /// Throws away the grouped edit in progress (Esc during a gesture).
    pub fn cancel_group(&mut self) {
        if self.group.take().is_some()
            && let Some(prev) = self.undo.pop()
        {
            self.doc = prev;
            self.revision += 1;
            self.prune_selection();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.group = None;
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, prev));
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        self.group = None;
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, next));
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.revision += 1;
        self.dirty = true;
        self.prune_selection();
    }

    /// Drops the selection of what no longer exists.
    pub fn prune_selection(&mut self) {
        if self.selected.is_some_and(|id| self.doc.part(id).is_none()) {
            self.selected = None;
        }
        if self
            .selected_screw
            .is_some_and(|id| !self.doc.screws.iter().any(|s| s.id == id))
        {
            self.selected_screw = None;
        }
    }

    pub fn select(&mut self, part: Option<PartId>) {
        self.selected = part;
        self.selected_screw = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarugo_core::model::Dim;

    fn width(s: &Session) -> String {
        s.doc()
            .param("W")
            .map(|p| p.value.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn grouped_edits_undo_together() {
        let mut s = Session::new(Document::new(), None);
        s.edit(|d| d.set_param("W", Dim::num(10.0)));
        s.edit_grouped("w", |d| d.set_param("W", Dim::num(20.0)));
        s.edit_grouped("w", |d| d.set_param("W", Dim::num(30.0)));
        s.end_group();
        s.edit(|d| d.set_param("W", Dim::num(40.0)));
        s.undo();
        assert_eq!(width(&s), "30");
        s.undo();
        assert_eq!(width(&s), "10");
        s.redo();
        assert_eq!(width(&s), "30");
        s.edit_grouped("h", |d| d.set_param("W", Dim::num(5.0)));
        s.cancel_group();
        assert_eq!(width(&s), "30");
        assert!(!s.can_redo());
    }
}
