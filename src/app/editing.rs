//! Node editor, keyboard input, IME and add-node commands.
use super::*;

impl MindmapApp {
    pub(super) fn start_edit(&mut self, id: NodeId, is_new: bool) {
        if self.editing_text_box.is_some() { self.finish_text_box_edit(true); }
        if self.editing.is_some() { self.finish_edit(true); }
        // Clone the title before mutably borrowing `self` in `select`.
        let Some(title) = self.doc.node(id).map(|node| node.title.clone()) else { return; };
        self.select(id);
        self.edit_original = title.clone();
        // A new node already has the compact placeholder title. Select it so
        // the next keystroke replaces it without opening an oversized blank editor.
        self.edit_text = title;
        self.editing = Some(id);
        self.edit_focus_pending = true;
        self.edit_select_all_pending = true;
        self.edit_ignore_enter_once = true;
        self.edit_is_new_node = is_new;
        self.ime_composing = false;
        self.ime_preedit.clear();
    }
    pub(super) fn finish_edit(&mut self, commit: bool) {
        let Some(id) = self.editing.take() else { return; };
        let title = self.edit_text.trim().to_owned();
        if commit && !title.is_empty() && title != self.edit_original {
            // A newly inserted topic must also preserve the existing layout
            // when its placeholder is replaced by a wider Japanese title.
            let positions_before = (self.edit_is_new_node && !self.reflow_on_add)
                .then(|| self.positions());
            // Adding and immediately naming a node is one Undo transaction.
            if !self.edit_is_new_node { self.checkpoint(); }
            if let Some(n) = self.doc.node_mut(id) { n.title = title; }
            if let Some(before) = positions_before { self.preserve_existing_positions(&before); }
            self.dirty = true;
        }
        self.edit_focus_pending = false;
        self.edit_select_all_pending = false;
        self.edit_ignore_enter_once = false;
        self.ime_composing = false;
        self.ime_preedit.clear();
        self.edit_is_new_node = false;
    }
    /// Preserve the current world coordinates across a structural or width
    /// change. Offsets are inherited, so apply only the difference from the
    /// parent correction to each node, in depth order.
    pub(super) fn preserve_existing_positions(&mut self, before: &HashMap<NodeId, Position>) {
        let automatically_placed = self.positions();
        let mut world_corrections: HashMap<NodeId, Offset> = HashMap::new();
        let mut existing: Vec<NodeId> = before.keys().copied().collect();
        existing.sort_by_key(|id| self.doc.depth(*id));
        for id in existing {
            let (Some(old), Some(auto)) = (before.get(&id), automatically_placed.get(&id)) else { continue };
            let correction = Offset { x: old.x - auto.x, y: old.y - auto.y };
            let inherited = self.doc.parent(id).and_then(|p| world_corrections.get(&p))
                .copied().unwrap_or_default();
            if let Some(node) = self.doc.node_mut(id) {
                node.offset.x += correction.x - inherited.x;
                node.offset.y += correction.y - inherited.y;
            }
            world_corrections.insert(id, correction);
        }
    }
    /// After inserting, retain the old nodes' positions and find an empty slot
    /// for the new one. Explicit auto-reflow skips this behavior.
    pub(super) fn position_new_node_without_reflow(
        &mut self, before: &HashMap<NodeId, Position>, new_id: NodeId,
        anchor_id: NodeId, sibling: bool,
    ) {
        self.preserve_existing_positions(before);
        let current = self.positions();
        let Some(placed) = current.get(&new_id).copied() else { return };
        let (anchor_x, anchor_y) = before.get(&anchor_id).map(|p| (p.x, p.y))
            .unwrap_or((placed.x, placed.y));
        let y = if sibling { anchor_y + 76.0 } else {
            // Add below the last existing child, or at the parent's height.
            before.iter().filter(|(id, _)| self.doc.parent(**id) == Some(anchor_id))
                .map(|(_, p)| p.y).fold(anchor_y - 76.0, f32::max) + 76.0
        };
        let x = if sibling { anchor_x } else { placed.x };
        let mut desired_y = y;
        let candidate_width = self.doc.node(new_id).map(|n| node_width_text(&n.title, self.doc.depth(new_id))).unwrap_or(150.0);
        // Never place the newly inserted card on top of an existing visible card.
        for _ in 0..200 {
            let occupied = before.iter().any(|(id, pos)| {
                let Some(node) = self.doc.node(*id) else { return false };
                let width = self.card_width(node, self.doc.depth(*id));
                (pos.x - x).abs() < (candidate_width + width) * 0.5 + 12.0
                    && (pos.y - desired_y).abs() < 54.0
            });
            if !occupied { break; }
            desired_y += 76.0;
        }
        if let Some(node) = self.doc.node_mut(new_id) {
            node.offset.x += x - placed.x;
            node.offset.y += desired_y - placed.y;
        }
    }
    pub(super) fn add_child(&mut self) {
        if self.editing.is_some() { self.finish_edit(true); }
        let anchor = self.selected;
        let before = (!self.reflow_on_add).then(|| self.positions());
        self.checkpoint();
        if let Some(node) = self.doc.node_mut(anchor) { node.collapsed = false; }
        if let Some(id) = self.doc.add_child(anchor, "New Topic") {
            if let Some(ref before) = before {
                self.position_new_node_without_reflow(before, id, anchor, false);
            }
            self.start_edit(id, true);
        }
    }
    pub(super) fn add_sibling(&mut self) {
        if self.editing.is_some() { self.finish_edit(true); }
        if self.doc.parent(self.selected).is_none() { self.add_child(); return; }
        let anchor = self.selected;
        let before = (!self.reflow_on_add).then(|| self.positions());
        self.checkpoint();
        if let Some(id) = self.doc.add_sibling(anchor, "New Topic") {
            if let Some(ref before) = before {
                self.position_new_node_without_reflow(before, id, anchor, true);
            }
            self.start_edit(id, true);
        }
    }
    /// Shift+Tab navigates from the current topic to its parent.
    /// `select` commits an active inline edit before changing selection.
    /// A root topic has no parent, so the shortcut is a no-op there.
    pub(super) fn select_parent(&mut self) {
        let source = self.editing.unwrap_or(self.selected);
        if let Some(parent) = self.doc.parent(source) {
            self.select(parent);
        }
    }
    pub(super) fn select_direction(&mut self, key: egui::Key) {
        let destination = match key {
            egui::Key::ArrowLeft => self.doc.parent(self.selected),
            egui::Key::ArrowRight => self.doc.children(self.selected).into_iter().next(),
            egui::Key::ArrowUp | egui::Key::ArrowDown => {
                let siblings = self.doc.parent(self.selected).map(|p| self.doc.children(p))
                    .unwrap_or_else(|| self.doc.roots());
                let index = siblings.iter().position(|&n| n == self.selected).unwrap_or(0);
                if key == egui::Key::ArrowUp && index > 0 { siblings.get(index - 1).copied() }
                else if key == egui::Key::ArrowDown { siblings.get(index + 1).copied() }
                else { None }
            }
            _ => None,
        };
        if let Some(id) = destination { self.select(id); }
    }
    pub(super) fn handle_keys(&mut self, ctx: &egui::Context) {
        let (ctrl, shift, save, open, new, undo, redo, enter, f2, space, delete, arrows, composing_now) = ctx.input(|i| (
            i.modifiers.command, i.modifiers.shift,
            i.key_pressed(egui::Key::S), i.key_pressed(egui::Key::O), i.key_pressed(egui::Key::N),
            i.key_pressed(egui::Key::Z), i.key_pressed(egui::Key::Y),
            i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::F2), i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            [egui::Key::ArrowLeft, egui::Key::ArrowRight, egui::Key::ArrowUp, egui::Key::ArrowDown]
                .into_iter().find(|&key| i.key_pressed(key)),
            i.events.iter().any(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty())),
        ));
        if self.ask_discard {
            // Ctrl+S works even while the unsaved-changes confirmation is open.
            // A failure/cancellation keeps the pending action and editor intact.
            if ctrl && save && self.save() { self.complete_pending_action(ctx); }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.ask_discard = false;
                self.pending_action = None;
            }
            return;
        }
        if self.grid_settings_open { return; }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.finish_edit(true);
            self.finish_text_box_edit(true);
            self.finish_connector_edit();
            self.search_open = true;
            self.search_focus_pending = true;
            ctx.request_repaint();
            return;
        }
        if self.link_origin.is_some() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.link_origin = None;
            self.status.clear();
            return;
        }
        if self.selected_link.is_some() && self.editing.is_none() && self.editing_text_box.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            self.delete_selected_link();
            return;
        }
        if self.insert_line_mode && ctx.input(|i|i.key_pressed(egui::Key::Escape)) {
            self.insert_line_mode=false;return;
        }
        if self.insert_shape_mode.is_some() && ctx.input(|i|i.key_pressed(egui::Key::Escape)) {
            self.insert_shape_mode=None;return;
        }
        if self.insert_connector_mode && ctx.input(|i|i.key_pressed(egui::Key::Escape)) {
            self.insert_connector_mode=false;return;
        }
        if self.insert_text_box_mode && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.insert_text_box_mode = false;
            return;
        }
        if ctrl && save {self.finish_connector_edit(); let _ = self.save(); return; }
        if ctrl && shift && self.editing.is_none() && self.editing_text_box.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::J)) {
            self.pure_json_to_clipboard(ctx, false);
            return;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q)) {
            self.request_exit(ctx);
            return;
        }
        if ctrl && open { self.open_file_dialog(FileDialog::Open); return; }
        if ctrl && new { self.request_new(); return; }
        if self.handle_clipboard_events(ctx) { return; }

        // Consume both Tab and Shift+Tab BEFORE drawing widgets, so neither
        // reaches egui's normal keyboard focus navigation. Respect IME preedit
        // and preserve ordinary Tab handling while editing a free text box.
        let (tab, parent_tab) = if !ctrl && self.editing_text_box.is_none() && self.editing_table_cell.is_none()
            && !self.ime_composing && !composing_now {
            ctx.input_mut(|i| {
                if shift {
                    (false, i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
                } else {
                    (i.consume_key(egui::Modifiers::NONE, egui::Key::Tab), false)
                }
            })
        } else { (false, false) };
        if !ctrl && self.editing_shape.is_none() {
            if let Some((table_id,row,col))=self.selected_table_cell.filter(|(id,_,_)|
                self.selected_shape==Some(*id)&&self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table))
            {
                if ctx.input(|i|i.key_pressed(egui::Key::F2)) {
                    ctx.input_mut(|i|{i.consume_key(egui::Modifiers::NONE,egui::Key::F2);});
                    self.start_table_cell_edit(table_id,row,col);
                    self.table_cell_input_mode=false;
                    self.formula_reference_mode=false;
                    return;
                }
                let movement=if tab{Some((0,if shift{-1}else{1},false,egui::Key::Tab))}
                    else if parent_tab{Some((0,-1,false,egui::Key::Tab))}
                    else if enter&&!shift{Some((1,0,false,egui::Key::Enter))}
                    else{arrows.map(|key|match key{
                        egui::Key::ArrowLeft=>(0,-1,shift,key),egui::Key::ArrowRight=>(0,1,shift,key),
                        egui::Key::ArrowUp=>(-1,0,shift,key),egui::Key::ArrowDown=>(1,0,shift,key),_ => (0,0,false,key),
                    })};
                if let Some((dr,dc,extend,key))=movement {
                    ctx.input_mut(|i|{i.consume_key(if key==egui::Key::Tab&&shift{egui::Modifiers::SHIFT}else{egui::Modifiers::NONE},key);});
                    self.move_table_cell_selection(table_id,row,col,dr,dc,extend);
                    return;
                }
                if delete {
                    ctx.input_mut(|i|{i.consume_key(egui::Modifiers::NONE,egui::Key::Delete);i.consume_key(egui::Modifiers::NONE,egui::Key::Backspace);});
                    let has_text=self.doc.shape(table_id).and_then(|s|s.table.as_ref()).and_then(|t|t.cells.get(row)).and_then(|r|r.get(col)).is_some_and(|text|!text.is_empty());
                    if has_text {
                        self.checkpoint();
                        if let Some(table)=self.doc.shape_mut(table_id).and_then(|s|s.table.as_mut()){
                            if let Some(cell)=table.cells.get_mut(row).and_then(|r|r.get_mut(col)){cell.clear();}
                            table.formula_cells.retain(|coord|*coord!=(row,col));
                        }
                    }
                    return;
                }
                let replacement=ctx.input(|i|i.events.iter().find_map(|event|match event{
                    egui::Event::Text(text) if !text.is_empty()=>Some(text.clone()),
                    egui::Event::Ime(egui::ImeEvent::Commit(text)) if !text.is_empty()=>Some(text.clone()),
                    _=>None,
                }));
                if let Some(text)=replacement {
                    self.start_table_cell_edit(table_id,row,col);
                    self.shape_edit_text=text;
                    self.formula_reference_mode=self.shape_edit_text.starts_with('=');
                    self.table_cell_input_mode=true;
                    self.table_overwrite_pending=false;
                    self.table_hidden_overwrite_selection=false;
                    let edit_id=egui::Id::new(("inline-table-cell",table_id,row,col));
                    let mut state=egui::TextEdit::load_state(ctx,edit_id).unwrap_or_default();
                    let cursor=self.shape_edit_text.chars().count();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor))));
                    state.store(ctx,edit_id);
                    ctx.input_mut(|i|i.events.retain(|event|!matches!(event,egui::Event::Text(_)|egui::Event::Ime(egui::ImeEvent::Commit(_)))));
                    return;
                }
            }
        }
        if parent_tab {
            if self.selected_text_box.is_none() { self.select_parent(); }
            return;
        }
        if tab {
            if self.selected_text_box.is_some() || self.selected_shape.is_some() { return; }
            if self.editing.is_some() { self.finish_edit(true); }
            self.add_child();
            return;
        }
        if self.editing.is_some() || self.editing_text_box.is_some() || self.editing_shape.is_some() || self.editing_connector.is_some() { return; }
        if let Some(key)=arrows {
            let movable=self.canvas_selection.iter().any(|t|matches!(t,CanvasTarget::Node(_) | CanvasTarget::Shape(_) | CanvasTarget::TextBox(_) | CanvasTarget::Line(_)));
            if movable && !ctrl {
                let step=if shift {5.0}else{1.0};
                let delta=match key {
                    egui::Key::ArrowLeft=>Vec2::new(-step,0.0), egui::Key::ArrowRight=>Vec2::new(step,0.0),
                    egui::Key::ArrowUp=>Vec2::new(0.0,-step), egui::Key::ArrowDown=>Vec2::new(0.0,step), _=>Vec2::ZERO,
                };
                self.checkpoint(); self.move_canvas_selection(delta); return;
            }
        }
        if ctrl && ctx.input(|i|i.key_pressed(egui::Key::D)) {
            self.duplicate_canvas_selection();return;
        }
        if ctrl && ctx.input(|i|i.key_pressed(egui::Key::A)) && self.editing_shape.is_none() {
            if let Some((table_id,_,_))=self.selected_table_cell.filter(|(id,_,_)|
                self.selected_shape==Some(*id)&&self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table)) {
                if let Some(table)=self.doc.shape(table_id).and_then(|s|s.table.as_ref()) {
                    let rows=table.cells.len();let columns=table.cells.first().map_or(0,Vec::len);
                    if rows>0&&columns>0 {
                        ctx.input_mut(|i|{i.consume_key(egui::Modifiers::COMMAND,egui::Key::A);i.consume_key(egui::Modifiers::CTRL,egui::Key::A);});
                        self.selected_table_cell=Some((table_id,0,0));
                        self.selected_table_range=Some((table_id,0,0,rows-1,columns-1));
                        return;
                    }
                }
            }
        }
        if let Some(id) = self.selected_text_box {
            if !ctrl && (f2 || space) { self.start_text_box_edit(id, false); }
            else if !ctrl && delete { self.delete_selected(); }
            else if ctrl && undo && !shift { self.undo(); }
            else if ctrl && (redo || (undo && shift)) { self.redo(); }
            return;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::A)) {
            self.selection = self.doc.visible_ids().into_iter().collect();
            self.canvas_selection=self.selection.iter().map(|id|CanvasTarget::Node(*id)).collect();
            self.canvas_selection.extend(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)));
            self.canvas_selection.extend(self.doc.shapes.iter().filter(|s|s.visible).map(|s|CanvasTarget::Shape(s.id)));
            self.canvas_selection.extend(self.doc.lines.iter().filter(|l|l.visible).map(|l|CanvasTarget::Line(l.id)));
            return;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::G)) {
            if shift { self.ungroup_selection(); } else { self.group_selection(); }
            return;
        }
        if ctx.egui_wants_keyboard_input() { return; }
        if ctrl && undo && !shift { self.undo(); }
        else if ctrl && (redo || (undo && shift)) { self.redo(); }
        else if !ctrl && enter { self.add_sibling(); }
        else if !ctrl && (f2 || space) { self.start_edit(self.selected, false); }
        else if !ctrl && delete { self.delete_selected(); }
        else if !ctrl { if let Some(key) = arrows { self.select_direction(key); } }
    }
    pub(super) fn read_ime(&mut self, ctx: &egui::Context) -> bool {
        if self.editing.is_none() && self.editing_text_box.is_none() && self.editing_shape.is_none() { return false; }
        let mut just_committed = false;
        ctx.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                        self.ime_composing = !text.is_empty();
                        self.ime_preedit = text.clone();
                    }
                    egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                        self.ime_composing = false;
                        self.ime_preedit.clear();
                        just_committed = true;
                    }
                    _ => {}
                }
            }
        });
        just_committed
    }
}
