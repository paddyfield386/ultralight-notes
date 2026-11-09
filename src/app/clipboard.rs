//! Clipboard and portable JSON tree export.
use super::*;
use crate::mindmap_core::TextBox;
use crate::canvas_core::{CanvasConnector, CanvasShape, CanvasLine};

/// Portable clipboard payload. Only connectors whose two ends were selected
/// are copied; references to objects outside the selection are intentionally excluded.
#[derive(Clone, Serialize, Deserialize)]
struct CanvasCopyPayload {
    #[serde(rename="__mindmap_canvas_clipboard")]
    marker: bool,
    nodes: Vec<Node>,
    text_boxes: Vec<TextBox>,
    shapes: Vec<CanvasShape>,
    #[serde(default)]
    lines: Vec<CanvasLine>,
    connectors: Vec<CanvasConnector>,
    #[serde(default)]
    links: Vec<NodeLink>,
}

impl MindmapApp {
    /// Ctrl+D duplicates canvas objects in one undo step, retaining only
    /// relationships whose endpoints are entirely within the selection.
    pub(super) fn duplicate_canvas_selection(&mut self) {
        let selected=self.canvas_selection.clone();
        if selected.is_empty(){return;}
        if selected.iter().all(|t|matches!(t,CanvasTarget::Node(_))) {
            let forest:Vec<_>=self.selection_roots().iter().filter_map(|id|self.doc.copy_subtree(*id)).collect();
            if !forest.is_empty() {self.checkpoint();let ids=self.doc.paste_forest_after(self.selected,&forest);
                self.selection=ids.iter().copied().collect();
                self.canvas_selection=ids.into_iter().map(CanvasTarget::Node).collect();}
            return;
        }
        let nodes=self.doc.nodes.iter().filter(|n|selected.contains(&CanvasTarget::Node(n.id))).cloned().collect();
        let text_boxes=self.doc.text_boxes.iter().filter(|b|selected.contains(&CanvasTarget::TextBox(b.id))).cloned().collect();
        let shapes=self.doc.shapes.iter().filter(|b|selected.contains(&CanvasTarget::Shape(b.id))).cloned().collect();
        let lines=self.doc.lines.iter().filter(|l|selected.contains(&CanvasTarget::Line(l.id))).cloned().collect();
        let connectors=self.doc.connectors.iter().filter(|c|selected.contains(&c.from.target)&&selected.contains(&c.to.target)).cloned().collect();
        let links=self.doc.links.iter().filter(|l|selected.contains(&CanvasTarget::Node(l.from))&&selected.contains(&CanvasTarget::Node(l.to))).copied().collect();
        self.paste_canvas_payload(CanvasCopyPayload{marker:true,nodes,text_boxes,shapes,lines,connectors,links});
    }
    pub(super) fn pure_json_to_clipboard(&mut self, ctx: &egui::Context, selected_branch: bool) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        let mut export=self.doc.clone();
        if selected_branch {
            let targets=self.canvas_selection.clone();
            let node_ids:HashSet<_>=targets.iter().filter_map(|t|match t{CanvasTarget::Node(id)=>Some(*id),_=>None}).collect();
            export.nodes.retain(|n|node_ids.contains(&n.id));
            for n in &mut export.nodes {if n.parent.is_none() || !node_ids.contains(&n.parent.unwrap_or(0)){n.parent=None;}}
            export.shapes.retain(|s|targets.contains(&CanvasTarget::Shape(s.id)));
            export.text_boxes.retain(|b|targets.contains(&CanvasTarget::TextBox(b.id)));
            export.lines.retain(|l|targets.contains(&CanvasTarget::Line(l.id)));
            let valid:HashSet<_>=targets;
            export.connectors.retain(|c|valid.contains(&c.from.target)&&valid.contains(&c.to.target));
            export.links.retain(|l|node_ids.contains(&l.from)&&node_ids.contains(&l.to));
            export.canvas_groups.retain_mut(|g|{g.members.retain(|m|valid.contains(m));g.members.len()>1});
        }
        match serde_json::to_string_pretty(&export) {
            Ok(text) => {
                ctx.copy_text(text);
                self.status.clear();
            }
            Err(err) => self.status = format!("Failed to generate structured JSON: {err}"),
        }
    }
    fn copy_canvas_payload(&mut self, ctx:&egui::Context, with_descendants:bool) -> bool {
        if self.canvas_selection.is_empty() {return false;}
        let mut targets=self.canvas_selection.clone();
        // A frame owns its members. Copy the whole contents so its membership
        // and grid references can be mapped into the destination document.
        loop {
            let before=targets.len();
            for shape in &self.doc.shapes {
                if targets.contains(&CanvasTarget::Shape(shape.id)) {targets.extend(shape.members.iter().copied());}
            }
            if targets.len()==before {break;}
        }
        if with_descendants {
            for id in self.selection_in_doc_order() {
                let mut todo=vec![id];
                while let Some(n)=todo.pop() {
                    targets.insert(CanvasTarget::Node(n));
                    todo.extend(self.doc.children(n));
                }
            }
        }
        let nodes=self.doc.nodes.iter().filter(|n|targets.contains(&CanvasTarget::Node(n.id))).cloned().collect();
        let text_boxes=self.doc.text_boxes.iter().filter(|b|targets.contains(&CanvasTarget::TextBox(b.id))).cloned().collect();
        let shapes=self.doc.shapes.iter().filter(|b|targets.contains(&CanvasTarget::Shape(b.id))).cloned().collect();
        let lines=self.doc.lines.iter().filter(|l|targets.contains(&CanvasTarget::Line(l.id))).cloned().collect();
        let connectors=self.doc.connectors.iter().filter(|c|targets.contains(&c.from.target)&&targets.contains(&c.to.target)).cloned().collect();
        let links=self.doc.links.iter().filter(|l|targets.contains(&CanvasTarget::Node(l.from))&&targets.contains(&CanvasTarget::Node(l.to))).copied().collect();
        let payload=CanvasCopyPayload {marker:true,nodes,text_boxes,shapes,lines,connectors,links};
        if let Ok(text)=serde_json::to_string_pretty(&payload) {ctx.copy_text(text);return true;}
        false
    }
    fn paste_canvas_payload(&mut self,payload:CanvasCopyPayload) {
        if !payload.marker {return;}
        if self.doc.nodes.len()+payload.nodes.len()+payload.shapes.len()+payload.text_boxes.len()+payload.lines.len()>10_000{return;}
        self.finish_edit(true);self.finish_text_box_edit(true);
        self.checkpoint();
        let mut remap:HashMap<CanvasTarget,CanvasTarget>=HashMap::new();
        let mut new_nodes=Vec::new();
        for node in payload.nodes {
            let old=node.id;
            let new=self.doc.next_id;self.doc.next_id+=1;
            remap.insert(CanvasTarget::Node(old),CanvasTarget::Node(new));
            new_nodes.push((old,Node{id:new,offset:Offset{x:node.offset.x+36.0,y:node.offset.y+36.0},..node}));
        }
        // Parent links inside the payload are remapped, external parents attach
        // to the current selection instead (never leave dangling IDs).
        let fallback_parent=self.doc.node(self.selected).map(|n|n.id);
        for (_,mut node) in new_nodes {
            node.parent=node.parent.and_then(|old| match remap.get(&CanvasTarget::Node(old)) {
                Some(CanvasTarget::Node(mapped))=>Some(*mapped),_=>fallback_parent
            });
            if node.parent.is_none(){node.parent=fallback_parent;}
            self.doc.nodes.push(node);
        }
        for mut b in payload.text_boxes {
            let old=b.id;
            b.id=self.doc.next_id;self.doc.next_id+=1;
            b.position.x+=36.0;b.position.y+=36.0;
            remap.insert(CanvasTarget::TextBox(old),CanvasTarget::TextBox(b.id));
            self.doc.text_boxes.push(b);
        }
        for mut s in payload.shapes {
            let old=s.id;
            s.id=self.doc.next_canvas_id;self.doc.next_canvas_id+=1;
            s.position.x+=36.0;s.position.y+=36.0;
            remap.insert(CanvasTarget::Shape(old),CanvasTarget::Shape(s.id));
            self.doc.shapes.push(s);
        }
        for mut l in payload.lines {
            let old=l.id;l.id=self.doc.next_canvas_id;self.doc.next_canvas_id+=1;
            l.move_by(36.0,36.0);
            remap.insert(CanvasTarget::Line(old),CanvasTarget::Line(l.id));
            self.doc.lines.push(l);
        }
        for s in &mut self.doc.shapes {
            if remap.values().any(|t|*t==CanvasTarget::Shape(s.id)) {
                s.members=s.members.iter().filter_map(|member|remap.get(member).copied()).collect();
                if let Some(grid)=s.grid.as_mut() {
                    grid.slots.retain(|slot|remap.contains_key(&slot.target));
                    for slot in &mut grid.slots {
                        if let Some(&mapped)=remap.get(&slot.target){slot.target=mapped;}
                    }
                }
            }
        }
        for mut c in payload.connectors {
            if let (Some(&from),Some(&to))=(remap.get(&c.from.target),remap.get(&c.to.target)) {
                c.id=self.doc.next_canvas_id;self.doc.next_canvas_id+=1;
                c.from.target=from;c.to.target=to;
                // Copied manual waypoints use world coordinates, like their owners.
                for point in &mut c.waypoints {point.x+=36.0;point.y+=36.0;}
                self.doc.connectors.push(c);
            }
        }
        for link in payload.links {
            if let (Some(CanvasTarget::Node(from)),Some(CanvasTarget::Node(to)))=(remap.get(&CanvasTarget::Node(link.from)),remap.get(&CanvasTarget::Node(link.to))) {
                self.doc.add_link(*from,*to);
            }
        }
        self.canvas_selection=remap.values().copied().collect();
        self.selection=self.canvas_selection.iter().filter_map(|t|match t{CanvasTarget::Node(id)=>Some(*id),_=>None}).collect();
        if let Some(id)=self.doc.nodes.iter().find(|n|self.selection.contains(&n.id)).map(|n|n.id){self.selected=id;}
        self.selected_shape=self.canvas_selection.iter().find_map(|t|match t{CanvasTarget::Shape(id)=>Some(*id),_=>None});
    }
    pub(super) fn copy_selected(&mut self, ctx: &egui::Context, with_descendants: bool) {
        if self.copy_canvas_payload(ctx,with_descendants) {return;}
        if self.editing_text_box.is_some() { self.finish_text_box_edit(true); }
        if let Some(id) = self.selected_text_box {
            if let Some(box_) = self.doc.text_box(id) { ctx.copy_text(box_.text.clone()); }
            return;
        }
        if self.editing.is_some() { self.finish_edit(true); }
        let targets = if with_descendants { self.selection_roots() } else { self.selection_in_doc_order() };
        let forest: Vec<_> = targets.into_iter().filter_map(|id| {
            if with_descendants { self.doc.copy_subtree(id) } else { self.doc.copy_node_only(id) }
        }).collect();
        if !forest.is_empty() {
            let text = forest.iter().map(CopiedNode::plain_text).collect::<Vec<_>>().join("\n");
            ctx.copy_text(text.clone());
            self.clipboard = Some(MindmapClipboard { text, forest });
        }
    }
    pub(super) fn paste_text_as_nodes(&mut self, text: &str) {
        if let Ok(payload)=serde_json::from_str::<CanvasCopyPayload>(text) {
            if payload.marker { self.paste_canvas_payload(payload);return; }
        }
        if self.editing_text_box.is_some() { self.finish_text_box_edit(true); }
        if let Some(id) = self.selected_text_box {
            let text = text.chars().take(10_000).collect::<String>();
            if text.trim().is_empty() { return; }
            let position = self.doc.text_box(id).map(|b| b.position).unwrap_or_default();
            self.checkpoint();
            let new_id = self.doc.add_text_box(position.x + 32.0, position.y + 32.0);
            if let Some(box_) = self.doc.text_box_mut(new_id) { box_.text = text; }
            self.select_text_box(new_id);
            return;
        }
        if self.editing.is_some() { self.finish_edit(true); }
        let previous = self.clipboard.as_ref().filter(|saved| saved.text == text)
            .map(|saved| saved.forest.clone());
        let forest = previous.unwrap_or_else(|| {
            let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if title.is_empty() { return Vec::new(); }
            vec![CopiedNode {
                title: title.chars().take(500).collect(), color: None,
                offset: Offset::default(), children: Vec::new(), collapsed: false,
            }]
        });
        let count: usize = forest.iter().map(CopiedNode::node_count).sum();
        if count == 0 { return; }
        if self.doc.nodes.len().saturating_add(count) > 10_000 {
            self.status = "Pasting would exceed the 10,000-node limit".into();
            return;
        }
        self.checkpoint();
        let ids = self.doc.paste_forest_after(self.selected, &forest);
        if let Some(&id) = ids.first() {
            self.selected = id;
            self.selection = ids.into_iter().collect();
            self.canvas_selection=self.selection.iter().map(|id|CanvasTarget::Node(*id)).collect();
            self.status.clear();
        } else {
            if let Some(doc) = self.undo.pop() { self.doc = doc; }
            self.status = "Paste failed".into();
        }
    }
    pub(super) fn paste_from_system_clipboard(&mut self) {
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
            Ok(text) => self.paste_text_as_nodes(&text),
            Err(error) => self.status = format!("Could not read the clipboard: {error}"),
        }
    }
    pub(super) fn handle_clipboard_events(&mut self, ctx: &egui::Context) -> bool {
        // winit emits Event::Copy and Event::Paste(text), *not* ordinary
        // Ctrl+C/Ctrl+V key-presses. While editing, leave those events entirely
        // to TextEdit, including Japanese IME composition and text selection.
        if self.editing.is_some() || self.editing_text_box.is_some() || self.editing_shape.is_some() || self.ime_composing { return false; }
        let actions = ctx.input(|i| {
            // winit can provide a Copy event for Ctrl+C or Ctrl+Shift+C.
            // Preserve the modifiers from the key event when available;
            // otherwise use the current modifier state.
            let with_descendants = i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: egui::Key::C, pressed: true, modifiers, ..
                } if modifiers.command => Some(modifiers.shift),
                _ => None,
            }).unwrap_or(i.modifiers.command && i.modifiers.shift);
            let mut actions = i.events.iter().filter_map(|event| match event {
                egui::Event::Copy => Some(ClipboardEvent::Copy { with_descendants }),
                egui::Event::Paste(text) => Some(ClipboardEvent::Paste(text.clone())),
                _ => None,
            }).collect::<Vec<_>>();
            // Some winit/egui integrations do not emit Copy for Ctrl+Shift+C.
            // Handle the key combination directly, without duplicating Copy.
            if i.modifiers.command && i.modifiers.shift
                && i.key_pressed(egui::Key::C)
                && !actions.iter().any(|a| matches!(a, ClipboardEvent::Copy { .. }))
            {
                actions.push(ClipboardEvent::Copy { with_descendants: true });
            }
            actions
        });
        if actions.is_empty() { return false; }
        for action in actions {
            match action {
                ClipboardEvent::Copy { with_descendants } => self.copy_selected(ctx, with_descendants),
                ClipboardEvent::Paste(text) => self.paste_text_as_nodes(&text),
            }
        }
        true
    }
}
