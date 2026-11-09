//! Free-floating text-box editing and creation.
use super::*;

impl MindmapApp {
    pub(super) fn start_text_box_edit(&mut self, id: u64, is_new: bool) {
        let Some(text) = self.doc.text_box(id).map(|box_| box_.text.clone()) else { return; };
        self.select_text_box(id);
        self.text_box_original = text.clone();
        self.text_box_editor = text;
        self.editing_text_box = Some(id);
        self.text_box_focus_pending = true;
        self.text_box_is_new = is_new;
    }
    pub(super) fn finish_text_box_edit(&mut self, commit: bool) {
        let Some(id) = self.editing_text_box.take() else { return; };
        if commit && self.text_box_editor != self.text_box_original {
            if !self.text_box_is_new { self.checkpoint(); }
            if let Some(box_) = self.doc.text_box_mut(id) {
                box_.text = self.text_box_editor.clone();
            }
            self.dirty = true;
        }
        self.text_box_is_new = false;
        self.text_box_focus_pending = false;
    }
    pub(super) fn insert_text_box_at(&mut self, point: Pos2, origin: Pos2) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        self.checkpoint();
        let relative = (point - origin) / self.zoom;
        let id = self.doc.add_text_box(relative.x, relative.y);
        self.insert_text_box_mode = false;
        self.start_text_box_edit(id, true);
    }
}
