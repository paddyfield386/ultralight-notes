//! Fast node-title search. The compact floating search bar does not occupy a sidebar.
use super::*;

impl MindmapApp {
    pub(super) fn search_matches(&self) -> Vec<NodeId> {
        let term = self.search_query.trim().to_lowercase();
        if term.is_empty() { return Vec::new(); }
        self.doc.nodes.iter()
            .filter(|node| node.title.to_lowercase().contains(&term))
            .map(|node| node.id).collect()
    }
    pub(super) fn jump_to_match(&mut self, id: NodeId) {
        if self.doc.node(id).is_none() { return; }
        let mut ancestor = self.doc.parent(id);
        let requires_expand = {
            let mut cur = ancestor;
            let mut expand = false;
            while let Some(parent) = cur {
                expand |= self.doc.node(parent).is_some_and(|n| n.collapsed);
                cur = self.doc.parent(parent);
            }
            expand
        };
        if requires_expand { self.checkpoint(); }
        while let Some(parent) = ancestor {
            let next = self.doc.parent(parent);
            if let Some(node) = self.doc.node_mut(parent) { node.collapsed = false; }
            ancestor = next;
        }
        self.select(id);
        self.center_pending = true;
        self.search_focus_pending = true;
    }
    pub(super) fn search_ui(&mut self, ctx: &egui::Context) {
        if !self.search_open { return; }
        let mut keep_open = true;
        let mut close_by_key = false;
        egui::Window::new("Find Node")
            .anchor(Align2::RIGHT_TOP, [-22.0, 142.0])
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .open(&mut keep_open)
            .show(ctx, |ui| {
                let mut query_changed = false;
                ui.horizontal(|ui| {
                    let input = ui.add(egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search node names")
                        .return_key(None::<egui::KeyboardShortcut>)
                        .desired_width(235.0)
                        .id(egui::Id::unique("node-search-field")));
                    if self.search_focus_pending {
                        input.request_focus();
                        self.search_focus_pending = false;
                    }
                    query_changed = input.changed();
                    if query_changed { self.search_index = 0; }
                    if ui.button("×").clicked() { close_by_key = true; }
                });
                let matches = self.search_matches();
                let len = matches.len();
                ui.horizontal(|ui| {
                    if self.search_query.trim().is_empty() {
                        ui.label("Enter a search term");
                    } else if len == 0 {
                        ui.label("No matching nodes");
                    } else {
                        let current = self.search_index % len;
                        ui.label(format!("{} / {} 件", current + 1, len));
                    }
                    let next = ui.add_enabled(len > 0, egui::Button::new("Next ↓")).clicked();
                    let prev = ui.add_enabled(len > 0, egui::Button::new("Previous ↑")).clicked();
                    let (enter, shift, escape) = ui.input(|i| (
                        i.key_pressed(egui::Key::Enter)
                            && !i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))),
                        i.modifiers.shift,
                        i.key_pressed(egui::Key::Escape),
                    ));
                    if escape { close_by_key = true; }
                    if len > 0 {
                        if prev || (enter && shift) {
                            self.search_index = (self.search_index + len - 1) % len;
                            let id = matches[self.search_index];
                            self.jump_to_match(id);
                        } else if next || enter {
                            if self.search_index < len && self.doc.node(matches[self.search_index]).is_some()
                                && self.selected != matches[self.search_index] {
                                // First Enter selects the first match, without skipping it.
                                let id = matches[self.search_index];
                                self.jump_to_match(id);
                            } else {
                                self.search_index = (self.search_index + 1) % len;
                                let id = matches[self.search_index];
                                self.jump_to_match(id);
                            }
                        } else if self.search_query.trim().len() > 0
                            && self.selected != matches[self.search_index.min(len - 1)]
                            && query_changed {
                            self.jump_to_match(matches[0]);
                        }
                    }
                });
            });
        self.search_open = keep_open && !close_by_key;
    }
}
