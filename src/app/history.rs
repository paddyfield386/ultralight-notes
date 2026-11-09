//! Deletion and undo/redo history.
use super::*;

impl MindmapApp {
    pub(super) fn delete_selected(&mut self) {
        if let Some(id) = self.selected_connector.take() {
            self.checkpoint();
            self.doc.connectors.retain(|c| c.id != id);
            return;
        }
        if self.selected_link.is_some() { self.delete_selected_link(); return; }
        if !self.canvas_selection.is_empty() {
            let shapes: Vec<_> = self.canvas_selection.iter().filter_map(|item| match item {
                CanvasTarget::Shape(id) => Some(*id), _ => None,
            }).collect();
            let lines:Vec<_>=self.canvas_selection.iter().filter_map(|item|match item {
                CanvasTarget::Line(id)=>Some(*id),_=>None,
            }).collect();
            let boxes:Vec<_>=self.canvas_selection.iter().filter_map(|item|match item {
                CanvasTarget::TextBox(id)=>Some(*id),_=>None,
            }).collect();
            if !shapes.is_empty() || !boxes.is_empty() || !lines.is_empty() {
                self.checkpoint();
                for id in shapes { self.doc.delete_shape(id); }
                for id in lines {self.doc.delete_line(id);}
                self.selected_line=None;
                for id in boxes {self.doc.delete_text_box(id);}
                self.canvas_selection.retain(|t| self.doc.target_exists(*t));
                self.selected_shape = None;
                self.selected_text_box = None;
                if self.selection.is_empty() { return; }
            }
        }
        if let Some(box_id) = self.selected_text_box {
            self.finish_text_box_edit(true);
            self.checkpoint();
            self.doc.delete_text_box(box_id);
            self.selected_text_box = None;
            return;
        }
        if self.editing.is_some() { self.finish_edit(true); }
        let ids = self.selection_roots();
        let targets: Vec<_> = ids.into_iter().filter(|id| {
            *id != ROOT && (self.doc.parent(*id).is_some() || self.doc.roots().len() > 1)
        }).collect();
        if targets.is_empty() { return; }
        self.checkpoint();
        let next = targets.iter().find_map(|id| self.doc.parent(*id))
            .unwrap_or_else(|| self.doc.roots().into_iter().find(|id| !targets.contains(id)).unwrap_or(ROOT));
        for id in targets { self.doc.delete_subtree(id); }
        self.selected = if self.doc.node(next).is_some() { next } else { self.doc.roots()[0] };
        self.selection = HashSet::from([self.selected]);
        self.canvas_selection=HashSet::from([CanvasTarget::Node(self.selected)]);
    }
    pub(super) fn undo(&mut self) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        if let Some(doc) = self.undo.pop() {
            self.redo.push(self.doc.clone());
            self.doc = doc;
            self.prune_selection();
            self.canvas_selection.retain(|t| self.doc.target_exists(*t));
            if self.selected_text_box.is_some_and(|id| self.doc.text_box(id).is_none()) { self.selected_text_box = None; }
            if self.selected_line.is_some_and(|id|self.doc.line(id).is_none()){self.selected_line=None;}
            if self.selected_shape.is_some_and(|id|self.doc.shape(id).is_none()){self.selected_shape=None;}
            self.dirty = true;
        }
    }
    pub(super) fn redo(&mut self) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        if let Some(doc) = self.redo.pop() {
            self.undo.push(self.doc.clone());
            self.doc = doc;
            self.prune_selection();
            self.canvas_selection.retain(|t| self.doc.target_exists(*t));
            if self.selected_text_box.is_some_and(|id| self.doc.text_box(id).is_none()) { self.selected_text_box = None; }
            if self.selected_line.is_some_and(|id|self.doc.line(id).is_none()){self.selected_line=None;}
            if self.selected_shape.is_some_and(|id|self.doc.shape(id).is_none()){self.selected_shape=None;}
            self.dirty = true;
        }
    }
}
