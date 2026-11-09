//! Generic selection, shapes, connector anchors, and z-order operations.
//! The tree remains owned by Document; CanvasTarget is the shared interaction interface.
use super::*;

impl MindmapApp {
    pub(super) fn reset_canvas_ephemeral(&mut self) {
        self.canvas_selection.clear();
        if let Some(root)=self.doc.roots().first().copied(){self.canvas_selection.insert(CanvasTarget::Node(root));}
        self.selected_shape = None;
        self.selected_text_box=None;
        self.selected_line=None;
        self.context_line=None;
        self.insert_line_mode=false;
        self.selected_connector = None;
        self.selected_connectors.clear();self.selected_link=None;self.link_origin=None;
        self.context_target=None;self.context_text_box=None;self.context_link=None;
        self.insert_text_box_mode=false;
        self.context_shape = None;
        self.context_connector = None;
        self.insert_connector_mode = false;
        self.insert_shape_mode = None;
        self.shape_resize_aspect=None;
        self.editing_shape = None;
        self.shape_edit_text.clear();
    }
    pub(super) fn select_shape(&mut self, id: u64, additive: bool) {
        if self.doc.shape(id).is_none() { return; }
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        if !additive {
            self.canvas_selection.clear();
            self.selection.clear();
        }
        let target = CanvasTarget::Shape(id);
        if additive && !self.canvas_selection.insert(target) { self.canvas_selection.remove(&target); }
        else { self.canvas_selection.insert(target); }
        if !additive {
            for member in self.doc.canvas_group_members(target) {
                if let CanvasTarget::Node(node)=member {self.selection.insert(node);}
                self.canvas_selection.insert(member);
            }
        }
        self.selected_shape = Some(id);
        self.selected_line=None;
        self.selected_text_box = None;
        self.selected_link = None;
        self.selected_connector = None;
    }
    pub(super) fn group_canvas_selection(&mut self) {
        if self.canvas_selection.len()<2{return;}
        self.checkpoint();let selected=self.canvas_selection.clone();
        self.doc.group_canvas(&selected);
    }
    pub(super) fn ungroup_canvas_selection(&mut self) {
        let selected=self.canvas_selection.clone();
        if self.doc.canvas_groups.iter().any(|g|g.members.iter().any(|t|selected.contains(t))) {
            self.checkpoint();self.doc.ungroup_canvas(&selected);
        }
    }
    pub(super) fn add_shape_at(&mut self, kind: ShapeKind, point: Pos2, origin: Pos2) {
        // Do not discard Japanese IME text from the last note in continuous mode.
        self.finish_shape_edit(true);
        self.checkpoint();
        let p = (point - origin) / self.zoom;
        let id = self.doc.add_shape(kind, Offset { x: p.x, y: p.y });
        if kind==ShapeKind::StickyNote {
            if let Some(frame_id)=self.frame_at_drop(point,origin) {
                self.apply_grid_insert(frame_id,id,point,origin);
            }
        }
        self.select_shape(id, false);
        // Preserve active drawing tool. Right-click / Esc exits insertion mode.
        self.insert_shape_mode = Some(kind);
        if kind == ShapeKind::Text || kind == ShapeKind::StickyNote { self.start_shape_edit(id); }
    }
    pub(super) fn start_shape_edit(&mut self, id: u64) {
        let Some(item) = self.doc.shape(id) else { return };
        if item.locked { return; }
        self.shape_edit_text = item.text.clone();
        self.select_shape(id, false);
        self.editing_shape = Some(id);
        self.shape_edit_focus_pending=true;
    }
    pub(super) fn finish_shape_edit(&mut self, commit: bool) {
        if let Some(id) = self.editing_shape.take() {
            if commit && self.doc.shape(id).is_some_and(|s| s.text != self.shape_edit_text) {
                self.checkpoint();
                if let Some(shape) = self.doc.shape_mut(id) { shape.text = self.shape_edit_text.clone(); }
            }
        }
    }
    pub(super) fn shape_screen_rect(&self, id: u64, origin: Pos2) -> Option<Rect> {
        let s = self.doc.shape(id)?;
        Some(Rect::from_min_size(
            origin + egui::vec2(s.position.x, s.position.y) * self.zoom,
            egui::vec2(s.width, s.height) * self.zoom))
    }
    pub(super) fn hit_shape(&self, pos: Pos2, origin: Pos2) -> Option<u64> {
        self.doc.shapes.iter().filter(|s|s.visible)
            .filter(|s|self.shape_screen_rect(s.id,origin).is_some_and(|r| {
                if s.kind!=ShapeKind::Frame {return r.contains(pos);}
                // Keep the title drop zone and selection zone identical.
                r.contains(pos) && (self.frame_header_screen_rect(s.id,origin)
                    .is_some_and(|header|header.contains(pos)) ||
                    (pos.x-r.left()).abs()<8.0 || (pos.x-r.right()).abs()<8.0 ||
                    (pos.y-r.bottom()).abs()<8.0)
            }))
            .max_by_key(|s|(s.kind!=ShapeKind::Frame,s.z,s.id)).map(|s|s.id)
    }
    /// Select a Frame by dragging its empty interior, without blocking its child objects.
    /// The caller must first try all foreground hit-tests (nodes, notes, connectors, lines).
    pub(super) fn hit_frame_interior(&self, pos: Pos2, origin: Pos2) -> Option<u64> {
        self.doc.shapes.iter().filter(|s|s.visible && s.kind==ShapeKind::Frame)
            .filter(|s|self.shape_screen_rect(s.id,origin).is_some_and(|r|r.contains(pos)))
            .max_by_key(|s|(s.z,s.id)).map(|s|s.id)
    }
    /// One square handle per side, with a larger 18px hit area for touchpads.
    /// This function is shared by painting and hit-testing so the visuals
    /// always match the actual resize target.
    pub(super) fn resize_handles(&self, id: u64, origin: Pos2) -> Option<Vec<(ResizeEdge,Rect)>> {
        let s = self.doc.shape(id)?;
        if !s.visible || s.locked || !self.canvas_selection.contains(&CanvasTarget::Shape(id)) { return None; }
        let r = self.shape_screen_rect(id, origin)?;
        let points = [
            (ResizeEdge::Top,r.center_top()), (ResizeEdge::Right,r.right_center()),
            (ResizeEdge::Bottom,r.center_bottom()), (ResizeEdge::Left,r.left_center()),
        ];
        Some(points.into_iter().map(|(edge,p)|
            (edge,Rect::from_center_size(p,egui::vec2(18.0,18.0)))).collect())
    }
    pub(super) fn resize_handle_at(&self,id:u64,origin:Pos2,p:Pos2)->Option<ResizeEdge> {
        self.resize_handles(id,origin)?.into_iter()
            .find_map(|(edge,r)|r.contains(p).then_some(edge))
    }
    /// Resize individual sides in world coordinates, anchoring the opposite
    /// side instead of translating the whole object. The minimum size cannot
    /// become negative even if the pointer crosses the opposite side.
    pub(super) fn resize_shape_by(&mut self,id:u64,edge:ResizeEdge,screen_delta:Vec2,keep_aspect:bool) {
        let aspect=self.shape_resize_aspect.unwrap_or(1.0).max(0.001);
        let dx=screen_delta.x/self.zoom;let dy=screen_delta.y/self.zoom;
        let spacing=if self.snap_to_grid {Some(self.grid.spacing)}else{None};
        let Some(s)=self.doc.shape_mut(id) else {return};
        if s.locked {return;}
        let (mut l,mut t)=(s.position.x,s.position.y);
        let (mut r,mut b)=(l+s.width,t+s.height);
        if edge.left() {l=(l+dx).min(r-32.0);}
        if edge.right() {r=(r+dx).max(l+32.0);}
        if edge.top() {t=(t+dy).min(b-24.0);}
        if edge.bottom() {b=(b+dy).max(t+24.0);}
        if keep_aspect {
            // Opposite corner/edge remains anchored. Preserve the ratio while
            // preferring the axis that the user is actively dragging.
            if edge.left() || edge.right() {
                let new_height=((r-l)/aspect).max(24.0);
                if edge.top() {t=b-new_height;} else {b=t+new_height;}
            } else {
                let new_width=((b-t)*aspect).max(32.0);
                if edge.top() {r=l+new_width;} else {r=l+new_width;}
            }
        }
        if let Some(step)=spacing.filter(|x|*x>0.0) {
            if edge.left() {l=(l/step).round()*step;}
            if edge.right() {r=(r/step).round()*step;}
            if edge.top() {t=(t/step).round()*step;}
            if edge.bottom() {b=(b/step).round()*step;}
            if r-l<32.0 {if edge.left() {l=r-32.0;}else{r=l+32.0;}}
            if b-t<24.0 {if edge.top() {t=b-24.0;}else{b=t+24.0;}}
        }
        s.position.x=l;s.position.y=t;
        s.width=(r-l).max(32.0);s.height=(b-t).max(24.0);
    }
    pub(super) fn object_screen_rect(&self, target: CanvasTarget, positions: &HashMap<NodeId,Position>, origin: Pos2) -> Option<Rect> {
        match target {
            CanvasTarget::Node(id) => Some(self.screen_rect(self.doc.node(id)?, *positions.get(&id)?, origin)),
            CanvasTarget::TextBox(id) => self.text_box_screen_rect(id, origin),
            CanvasTarget::Shape(id) => self.shape_screen_rect(id, origin),
            CanvasTarget::Line(id) => self.line_screen_rect(id, origin),
        }
    }
    pub(super) fn anchor_position(rect: Rect, anchor: Anchor) -> Pos2 {
        match anchor { Anchor::Left => rect.left_center(), Anchor::Right => rect.right_center(),
            Anchor::Top => rect.center_top(), Anchor::Bottom => rect.center_bottom() }
    }
    /// The visible connector circle is centered exactly on the object's outline.
    pub(super) fn visual_anchor_position(rect:Rect,anchor:Anchor)->Pos2 {
        Self::anchor_position(rect,anchor)
    }
    pub(super) fn find_anchor(&self,pointer:Pos2,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<ConnectorEnd> {
        // A user can drag from a visible handle without selecting the insertion tool.
        // Prefer already selected objects, then any hovered object.
        // A selected shape uses the square resize handles. Its connector
        // circles are deliberately hidden to make the two interactions clear.
        let selected = (!self.selected_shape.is_some()).then(|| self.canvas_selection.iter().copied())
            .into_iter().flatten();
        let others=self.doc.shapes.iter().filter(|s|s.visible).map(|s|CanvasTarget::Shape(s.id))
            .chain(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)))
            .chain(self.doc.nodes.iter().filter(|node|positions.contains_key(&node.id)).map(|node|CanvasTarget::Node(node.id)));
        let mut closest:Option<(f32,ConnectorEnd)>=None;
        for target in selected.chain(others) {
            if matches!(target,CanvasTarget::Line(_)){continue;}
            let Some(rect)=self.object_screen_rect(target,positions,origin) else{continue;};
            if !rect.expand(29.0).contains(pointer){continue;}
            for anchor in [Anchor::Top,Anchor::Right,Anchor::Bottom,Anchor::Left] {
                let dist=pointer.distance_sq(Self::visual_anchor_position(rect,anchor));
                if dist<=100.0 && closest.is_none_or(|(d,_)|dist<d){
                    closest=Some((dist,ConnectorEnd{target,anchor}));
                }
            }
        }
        closest.map(|(_,endpoint)|endpoint)
    }
    /// Connector endpoint snap: inside an object OR within 22 screen pixels
    /// of any of its four anchors. Normal hit-tests are intentionally unchanged.
    pub(super) fn connector_target_near(&self,p:Pos2,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<ConnectorEnd> {
        if let Some(target)=self.target_at(p,positions,origin) {
            let rect=self.object_screen_rect(target,positions,origin)?;
            return Some(ConnectorEnd{target,anchor:Self::closest_anchor(rect,p)});
        }
        let targets=self.doc.shapes.iter().filter(|s|s.visible).map(|s|CanvasTarget::Shape(s.id))
            .chain(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)))
            .chain(self.doc.nodes.iter().filter(|node|positions.contains_key(&node.id)).map(|node|CanvasTarget::Node(node.id)));
        let mut closest:Option<(f32,ConnectorEnd)>=None;
        for target in targets {
            let Some(rect)=self.object_screen_rect(target,positions,origin) else {continue};
            for anchor in [Anchor::Top,Anchor::Right,Anchor::Bottom,Anchor::Left] {
                let dist=p.distance_sq(Self::anchor_position(rect,anchor));
                if dist<22.0*22.0 && closest.is_none_or(|(d,_)|dist<d) {
                    closest=Some((dist,ConnectorEnd{target,anchor}));
                }
            }
        }
        closest.map(|(_,end)|end)
    }
    pub(super) fn target_at(&self, pointer: Pos2, positions: &HashMap<NodeId,Position>, origin: Pos2) -> Option<CanvasTarget> {
        self.hit_shape(pointer, origin).map(CanvasTarget::Shape)
            .or_else(|| self.hit_test_text_box(pointer, origin).map(CanvasTarget::TextBox))
            .or_else(|| self.hit_test(pointer, positions, origin, None).map(CanvasTarget::Node))
    }
    pub(super) fn closest_anchor(rect: Rect, point: Pos2) -> Anchor {
        [Anchor::Top,Anchor::Right,Anchor::Bottom,Anchor::Left].into_iter()
            .min_by(|a,b| Self::anchor_position(rect,*a).distance_sq(point)
                .total_cmp(&Self::anchor_position(rect,*b).distance_sq(point)))
            .unwrap_or(Anchor::Left)
    }
    /// Pick the geometrically nearest source circle to the destination circle.
    #[cfg(test)]
    pub(super) fn facing_source_anchor(source:Rect,destination:Pos2)->Anchor {
        Self::closest_anchor(source,destination)
    }
    pub(super) fn connector_screen_points(&self, id: u64, positions: &HashMap<NodeId,Position>, origin: Pos2) -> Option<(Pos2,Pos2)> {
        let c = self.doc.connectors.iter().find(|c| c.id == id)?;
        let a = self.object_screen_rect(c.from.target, positions, origin)?;
        let b = self.object_screen_rect(c.to.target, positions, origin)?;
        Some((Self::anchor_position(a,c.from.anchor), Self::anchor_position(b,c.to.anchor)))
    }
    pub(super) fn hit_connector(&self,pointer:Pos2,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<u64> {
        self.doc.connectors.iter().rev().filter_map(|c|{
            let path=self.connector_screen_path(c.id,positions,origin)?;
            path.windows(2).any(|seg|crate::diagram_core::distance_sq(
                Offset{x:pointer.x,y:pointer.y},Offset{x:seg[0].x,y:seg[0].y},
                Offset{x:seg[1].x,y:seg[1].y})<65.0).then_some(c.id)
        }).next()
    }
    pub(super) fn paint_canvas_connectors(&self,painter:&egui::Painter,positions:&HashMap<NodeId,Position>,origin:Pos2){
        for c in &self.doc.connectors {self.paint_connector(painter,c,positions,origin);}
    }
    pub(super) fn paint_canvas_shapes(&self, painter: &egui::Painter, origin: Pos2, hover: Option<Pos2>) {
        let mut ordered:Vec<_>=self.doc.shapes.iter().filter(|s|s.visible&&s.kind!=ShapeKind::Frame)
            .map(|s|(s.z,s.id,false)).chain(self.doc.lines.iter().filter(|l|l.visible)
                .map(|l|(l.z,l.id,true))).collect();
        ordered.sort_by_key(|(z,id,_)|(*z,*id));
        for (_,id,is_line) in ordered {
            if is_line {self.paint_single_line(painter,origin,id);continue;}
            let Some(s)=self.doc.shape(id) else {continue};
            let Some(r)=self.shape_screen_rect(s.id,origin) else {continue};
            if !r.intersects(painter.clip_rect()) {continue;}
            let fill=s.style.fill.map(color_from_rgb).unwrap_or(Color32::TRANSPARENT);
            let outline=s.style.outline.map(color_from_rgb).unwrap_or(Color32::TRANSPARENT);
            let stroke=Stroke::new(s.style.stroke_width*self.zoom,outline);
            Self::paint_shape_kind(painter,s.kind,r,fill,stroke);
            let chosen=self.canvas_selection.contains(&CanvasTarget::Shape(s.id));
            if chosen {
                painter.rect_stroke(r.expand(3.0),4.0,Stroke::new(1.6,ACCENT),StrokeKind::Outside);
                if !s.locked {
                    if let Some(handles)=self.resize_handles(s.id,origin) {
                        for (_,hit) in handles {
                            let h=Rect::from_center_size(hit.center(),egui::vec2(9.0,9.0));
                            let hovered = hover.is_some_and(|pointer| hit.contains(pointer));
                            painter.rect_filled(h,2.0,if hovered { ACCENT } else { Color32::WHITE });
                            painter.rect_stroke(h,2.0,Stroke::new(1.0,if hovered { Color32::WHITE } else { ACCENT }),StrokeKind::Inside);
                        }
                    }
                }
            }
            self.paint_shape_text(painter,r,s);
        }
    }
    pub(super) fn paint_canvas_groups(&self,painter:&egui::Painter,positions:&HashMap<NodeId,Position>,origin:Pos2) {
        for group in &self.doc.canvas_groups {
            let mut outline:Option<Rect>=None;
            for &target in &group.members {
                if let Some(rect)=self.object_screen_rect(target,positions,origin) {
                    outline=Some(outline.map_or(rect,|old|old.union(rect)));
                }
            }
            if let Some(rect)=outline {
                if painter.clip_rect().intersects(rect) {
                    painter.rect_stroke(rect.expand(9.0),10.0,Stroke::new(1.0,Color32::from_rgb(165,177,202)),StrokeKind::Inside);
                }
            }
        }
    }
    pub(super) fn paint_anchor_handles(&self,painter:&egui::Painter,positions:&HashMap<NodeId,Position>,origin:Pos2,hover:Option<Pos2>) {
        let dragging_connector=self.dragging.is_some_and(|d|matches!(d,Drag::Connector(_)|Drag::RetargetConnector(..)));
        let connecting=self.insert_connector_mode || dragging_connector;
        let hovered_target=hover.and_then(|p|self.target_at(p,positions,origin));
        // Keep circles visible when moving between an object and its boundary.
        let hovered_anchor_target=hover.and_then(|p|self.find_anchor(p,positions,origin))
            .map(|endpoint|endpoint.target);
        let hide_ports = !connecting && (self.selected_shape.is_some() || self.selected_text_box.is_some());
        let selected=(!hide_ports).then(|| self.canvas_selection.iter().copied())
            .into_iter().flatten();
        let others=if connecting {
            self.doc.shapes.iter().filter(|s|s.visible).map(|s|CanvasTarget::Shape(s.id))
                .chain(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)))
                .chain(self.doc.nodes.iter().filter(|node|positions.contains_key(&node.id))
                    .map(|node|CanvasTarget::Node(node.id))).collect::<Vec<_>>()
        }else{Vec::new()};
        let hovered_targets = (!hide_ports).then_some((hovered_target, hovered_anchor_target));
        let targets=selected.chain(others).chain(hovered_targets.into_iter().flat_map(|(a,b)| [a,b].into_iter().flatten()));
        let mut visited=HashSet::new();
        let mut painted:Vec<Pos2>=Vec::new();
        for target in targets {
            if !visited.insert(target)||matches!(target,CanvasTarget::Line(_)){continue;}
            let Some(rect)=self.object_screen_rect(target,positions,origin) else{continue;};
            if !rect.expand(30.0).intersects(painter.clip_rect()){continue;}
            for anchor in [Anchor::Top,Anchor::Right,Anchor::Bottom,Anchor::Left] {
                let p=Self::visual_anchor_position(rect,anchor);
                // Coincident handles should look like one circle, not flicker
                // because multiple filled/stroked circles are drawn on top.
                if painted.iter().any(|q|q.distance_sq(p)<4.0){continue;}
                painted.push(p);
                // Fixed circle appearance prevents hover/drag flicker.
                let radius=4.5;
                painter.circle_filled(p,radius,Color32::WHITE);
                painter.circle_stroke(p,radius,Stroke::new(1.25,ACCENT));
            }
        }
    }
    pub(super) fn handle_canvas_keyboard(&mut self, ctx:&egui::Context) {
        if self.editing.is_some()||self.editing_text_box.is_some()||self.editing_shape.is_some()||self.editing_connector.is_some()||self.ime_composing {return;}
        if let Some(id)=self.selected_connector {
            if ctx.input(|i|i.key_pressed(egui::Key::Delete)) {
                self.checkpoint();
                let selected=self.selected_connectors.clone();
                self.doc.connectors.retain(|c|c.id!=id&&!selected.contains(&c.id));
                self.selected_connectors.clear();self.selected_connector=None;
            }
        }
    }
    pub(super) fn move_canvas_selection(&mut self,delta:Vec2) {
        let mut targets=self.canvas_selection.clone();
        let frames:Vec<_>=targets.iter().filter_map(|t|match t {CanvasTarget::Shape(id) => self.doc.shape(*id)
            .filter(|s|s.kind==ShapeKind::Frame&&!s.locked).map(|s|s.members.clone()),_=>None}).collect();
        for members in frames {targets.extend(members);}
        for target in targets {
            match target {
                CanvasTarget::Shape(id) => {
                    if let Some(s)=self.doc.shape_mut(id) { if !s.locked {s.position.x+=delta.x/self.zoom;s.position.y+=delta.y/self.zoom;} }
                }
                CanvasTarget::TextBox(id) => {
                    if let Some(b)=self.doc.text_box_mut(id) {b.position.x+=delta.x/self.zoom;b.position.y+=delta.y/self.zoom;}
                }
                CanvasTarget::Line(id) => {
                    if let Some(l)=self.doc.line_mut(id) {if !l.locked {l.move_by(delta.x/self.zoom,delta.y/self.zoom);}}
                }
                CanvasTarget::Node(id) => {
                    if !self.drag_moving.contains(&id) {
                        if let Some(n)=self.doc.node_mut(id){n.offset.x+=delta.x/self.zoom;n.offset.y+=delta.y/self.zoom;}
                    }
                }
            }
        }
    }
    pub(super) fn snap_moved_shapes(&mut self) {
        if !self.snap_to_grid { return; }
        let step=self.grid.spacing;
        for &item in &self.canvas_selection {
            if let CanvasTarget::Line(id)=item {
                if let Some(line)=self.doc.line_mut(id) {if !line.locked {
                    let dx=(line.start.x/step).round()*step-line.start.x;
                    let dy=(line.start.y/step).round()*step-line.start.y;
                    line.move_by(dx,dy);
                }}
            }
            if let CanvasTarget::TextBox(id)=item {
                if let Some(b)=self.doc.text_box_mut(id) {
                    b.position.x=(b.position.x/step).round()*step;
                    b.position.y=(b.position.y/step).round()*step;
                }
            }
            if let CanvasTarget::Shape(id)=item {
                if let Some(s)=self.doc.shape_mut(id) {if !s.locked {
                    s.position.x=(s.position.x/step).round()*step;
                    s.position.y=(s.position.y/step).round()*step;
                }}
            }
        }
    }
    pub(super) fn change_selected_shape_z(&mut self,direction:i32) {
        self.change_selected_line_z(direction);
    }

}

#[cfg(test)]
mod resize_regressions {
    use super::*;

    #[test]
    fn four_side_handles_are_visible_and_hit_test_matches() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let id=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:100.0,y:140.0});
        app.zoom=1.0;
        app.canvas_selection.insert(CanvasTarget::Shape(id));
        let handles=app.resize_handles(id,Pos2::ZERO).unwrap();
        assert_eq!(handles.len(),4);
        for (edge,area) in handles {
            assert_eq!(app.resize_handle_at(id,Pos2::ZERO,area.center()),Some(edge));
        }
    }

    #[test]
    fn resizing_from_left_and_top_moves_only_those_edges() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.zoom=1.0;
        let id=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:100.0,y:140.0});
        let (old_right,old_bottom)={
            let s=app.doc.shape(id).unwrap();
            (s.position.x+s.width,s.position.y+s.height)
        };
        app.resize_shape_by(id,ResizeEdge::Left,egui::vec2(30.0,0.0),false);
        app.resize_shape_by(id,ResizeEdge::Top,egui::vec2(0.0,-20.0),false);
        let s=app.doc.shape(id).unwrap();
        assert_eq!(s.position.x,130.0);
        assert_eq!(s.position.y,120.0);
        assert_eq!(s.position.x+s.width,old_right);
        assert_eq!(s.position.y+s.height,old_bottom);
    }
}
