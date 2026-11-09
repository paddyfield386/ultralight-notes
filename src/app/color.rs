//! Node and annotation colors and their context menus.
use super::*;

impl MindmapApp {
    pub(super) fn apply_color(&mut self, color: Option<u8>) {
        if self.editing.is_some() { self.finish_edit(true); }
        let targets = self.selection_in_doc_order();
        if targets.iter().any(|id| self.doc.node(*id).is_some_and(|n| n.color != color)) {
            self.checkpoint();
            for id in targets {
                if let Some(node) = self.doc.node_mut(id) { node.color = color; }
            }
        }
    }
    pub(super) fn apply_text_box_color(&mut self, border: bool, color: Option<[u8; 3]>) {
        self.apply_text_box_appearance(border, color, false);
    }
    pub(super) fn apply_text_box_appearance(&mut self, border: bool, color: Option<[u8; 3]>, transparent: bool) {
        let Some(id) = self.selected_text_box else { return; };
        let changed = self.doc.text_box(id).is_some_and(|b| {
            if border { b.border_color != color || b.border_none }
            else { b.fill_color != color || b.fill_transparent != transparent }
        });
        if changed {
            self.finish_text_box_edit(true);
            self.checkpoint();
            if let Some(b) = self.doc.text_box_mut(id) {
                if border { b.border_color = color; b.border_none = false; }
                else {
                    b.fill_color = color;
                    b.fill_transparent = transparent;
                }
            }
            self.dirty = true;
        }
    }
    pub(super) fn remove_text_box_border(&mut self) {
        let Some(id) = self.selected_text_box else { return; };
        if self.doc.text_box(id).is_some_and(|b| !b.border_none || b.border_color.is_some()) {
            self.finish_text_box_edit(true);
            self.checkpoint();
            if let Some(b) = self.doc.text_box_mut(id) {
                b.border_none = true;
                b.border_color = None;
            }
            self.dirty = true;
        }
    }
    /// Color tiles are blank swatches: no circle/character is drawn over the fill.
    pub(super) fn color_tile(ui: &mut egui::Ui, color: Color32, border: Color32, label: &str) -> bool {
        ui.add_sized([23.0, 23.0], egui::Button::new("").fill(color)
            .stroke(Stroke::new(1.0, border)))
            .on_hover_text(label).clicked()
    }
    pub(super) fn text_box_palette(&mut self, ui: &mut egui::Ui, border: bool) {
        if ui.button(self.language.text("Default Color", "既定の色")).clicked() {
            self.apply_text_box_color(border, None);
            ui.close();
        }
        if !border && ui.button(self.language.text("Transparent", "透明")).clicked() {
            self.apply_text_box_appearance(false, None, true);
            ui.close();
        }
        if border && ui.button(self.language.text("No Line", "線なし")).clicked() {
            self.remove_text_box_border();
            ui.close();
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            for (i, label) in COLOR_LABELS.iter().enumerate() {
                let color = if border { TINTS[i].line } else { TINTS[i].fill };
                if Self::color_tile(ui, color, TINTS[i].border, label) {
                    self.apply_text_box_color(border, Some(rgb_of(color)));
                    ui.close();
                }
            }
        });
    }
    pub(super) fn context_color(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("色", |ui| {
            if ui.button("Automatic (Branch Color)").clicked() { self.apply_color(None); ui.close(); }
            ui.separator();
            for (i, name) in COLOR_LABELS.iter().enumerate() {
                let text = egui::RichText::new(*name).color(TINTS[i].line);
                if ui.button(text).clicked() { self.apply_color(Some(i as u8)); ui.close(); }
            }
        });
    }
    pub(super) fn tint_index(&self, id: NodeId) -> usize {
        if let Some(index) = self.doc.node(id).and_then(|n| n.color) { return (index as usize) % TINTS.len(); }
        let map_root = self.doc.map_root(id).unwrap_or(ROOT);
        let branch = self.doc.root_branch(id).unwrap_or(map_root);
        let branch_index = self.doc.children(map_root).iter().position(|&b| b == branch).unwrap_or(0);
        (branch_index + self.doc.roots().iter().position(|&r| r == map_root).unwrap_or(0)) % TINTS.len()
    }
}
