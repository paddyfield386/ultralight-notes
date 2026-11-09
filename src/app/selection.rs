//! Document and selection state, grouping and branch alignment.
use super::*;

impl MindmapApp {
    pub(super) fn filename(&self) -> String {
        self.current_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.json".to_owned())
    }
    pub(super) fn checkpoint(&mut self) {
        if self.undo.len() >= 100 { self.undo.remove(0); }
        self.undo.push(self.doc.clone());
        self.redo.clear();
        self.dirty = true;
    }
    pub(super) fn select(&mut self, id: NodeId) {
        if self.doc.node(id).is_none() { return; }
        if self.selected_text_box.is_some() { self.finish_text_box_edit(true); }
        self.selected_text_box = None;
        self.selected_shape = None;
        self.selected_line=None;
        self.selected_connector = None;
        self.canvas_selection.clear();
        self.canvas_selection.insert(CanvasTarget::Node(id));
        for member in self.doc.canvas_group_members(CanvasTarget::Node(id)) {
            if let CanvasTarget::Node(node)=member {self.selection.insert(node);}
            self.canvas_selection.insert(member);
        }
        self.selected_link = None;
        if self.selected != id && self.editing.is_some() { self.finish_edit(true); }
        self.selected = id;
        self.selection = self.doc.members_of(id).into_iter().collect();
    }
    pub(super) fn select_text_box(&mut self, id: u64) {
        if self.doc.text_box(id).is_none() { return; }
        self.finish_edit(true);
        if self.editing_text_box != Some(id) { self.finish_text_box_edit(true); }
        self.selected_text_box = Some(id);
        self.selected_link = None;
        self.selected_shape = None;
        self.selected_line=None;
        self.selected_connector = None;
        self.canvas_selection.clear();
        self.canvas_selection.insert(CanvasTarget::TextBox(id));
        self.selection.clear();
        for member in self.doc.canvas_group_members(CanvasTarget::TextBox(id)) {
            if let CanvasTarget::Node(node)=member {self.selection.insert(node);}
            self.canvas_selection.insert(member);
        }
    }
    pub(super) fn toggle_selection(&mut self, id: NodeId) {
        if self.doc.node(id).is_none() { return; }
        if self.editing.is_some() { self.finish_edit(true); }
        let set = self.doc.members_of(id);
        if set.iter().all(|id| self.selection.contains(id)) {
            for id in set { self.selection.remove(&id); }
        } else {
            self.selection.extend(set);
        }
        if self.selection.is_empty() { self.selection.insert(id); }
        self.canvas_selection.retain(|t| !matches!(t, CanvasTarget::Node(_)));
        for &node in &self.selection { self.canvas_selection.insert(CanvasTarget::Node(node)); }
        self.selected = if self.selection.contains(&id) { id } else { *self.selection.iter().next().unwrap() };
    }
    pub(super) fn selection_in_doc_order(&self) -> Vec<NodeId> {
        self.doc.nodes.iter().filter(|n| self.selection.contains(&n.id))
            .map(|n| n.id).collect()
    }
    /// Exclude selected descendants when a selected ancestor already moves its subtree.
    pub(super) fn selection_roots(&self) -> Vec<NodeId> {
        self.selection_in_doc_order().into_iter().filter(|id| {
            let mut parent = self.doc.parent(*id);
            while let Some(p) = parent {
                if self.selection.contains(&p) { return false; }
                parent = self.doc.parent(p);
            }
            true
        }).collect()
    }
    pub(super) fn prune_selection(&mut self) {
        self.selection.retain(|id| self.doc.node(*id).is_some());
        if self.doc.node(self.selected).is_none() {
            self.selected = self.doc.roots().first().copied().unwrap_or(ROOT);
        }
        if self.selection.is_empty() && self.canvas_selection.is_empty() { self.selection.insert(self.selected); }
        self.canvas_selection.retain(|target| self.doc.target_exists(*target));
    }
    pub(super) fn group_selection(&mut self) {
        if self.editing.is_some() { self.finish_edit(true); }
        if self.canvas_selection.len()>1 && self.canvas_selection.iter().any(|t|!matches!(t,CanvasTarget::Node(_))) {
            self.group_canvas_selection();return;
        }
        if self.selection.len() < 2 { return; }
        self.checkpoint();
        self.doc.group_nodes(&self.selection);
    }
    pub(super) fn ungroup_selection(&mut self) {
        if self.canvas_selection.iter().any(|t|!matches!(t,CanvasTarget::Node(_))) {
            self.ungroup_canvas_selection();return;
        }
        let groups: HashSet<_> = self.selection.iter().filter_map(|id| self.doc.group_for(*id)).collect();
        if groups.is_empty() { return; }
        self.checkpoint();
        self.doc.groups.retain(|g| !groups.contains(&g.id));
    }
    pub(super) fn align_selection(&mut self) {
        if self.editing.is_some() { self.finish_edit(true); }
        let targets = self.selection_roots();
        let should_change = targets.iter().any(|id| {
            let mut todo = vec![*id];
            while let Some(current) = todo.pop() {
                if self.doc.node(current).is_some_and(|n| n.parent.is_some() && n.offset != Offset::default()) { return true; }
                todo.extend(self.doc.children(current));
            }
            false
        });
        if should_change {
            self.checkpoint();
            for id in targets { self.doc.reset_branch_offsets(id); }
        }
    }
    pub(super) fn add_map(&mut self) {
        if self.editing.is_some() { self.finish_edit(true); }
        self.checkpoint();
        let id = self.doc.add_map("New Mind Map");
        self.select(id);
        self.center_pending = true;
        self.start_edit(id, false);
    }
    pub(super) fn toggle_collapse(&mut self, collapse: bool) {
        if self.editing.is_some() { self.finish_edit(true); }
        let targets = self.selection_in_doc_order();
        if targets.iter().any(|id| !self.doc.children(*id).is_empty()
            && self.doc.node(*id).is_some_and(|n| n.collapsed != collapse)) {
            self.checkpoint();
            for id in targets {
                if !self.doc.children(id).is_empty() {
                    if let Some(node) = self.doc.node_mut(id) { node.collapsed = collapse; }
                }
            }
        }
    }
    pub(super) fn reset_all_layout(&mut self) {
        if self.doc.nodes.iter().any(|n| n.parent.is_some() && n.offset != Offset::default()) {
            self.checkpoint(); self.doc.reset_all_offsets();
        }
    }
}
