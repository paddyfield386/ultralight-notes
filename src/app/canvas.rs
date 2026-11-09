//! Canvas rendering and drag/pan/zoom interactions.
use super::*;

impl MindmapApp {
    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let bounds = response.rect;
        let canvas_bg = if self.dark { Color32::from_rgb(24, 29, 41) } else { BG };
        painter.rect_filled(bounds, 0.0, canvas_bg);
        let ctx = ui.ctx().clone();
        let just_committed_ime = self.read_ime(&ctx);
        let positions = self.positions();
        if self.fit_pending { self.fit_to_board(bounds, &positions); self.fit_pending = false; }
        let base = Pos2::new(bounds.left() + 152.0, bounds.center().y);
        if self.center_pending {
            if let Some(id) = self.selected_text_box {
                if let Some(box_) = self.doc.text_box(id) {
                    let mid = egui::vec2(box_.position.x + box_.width * 0.5,
                        box_.position.y + box_.height * 0.5);
                    self.pan = bounds.center() - base - mid * self.zoom;
                }
            } else if let Some(p) = positions.get(&self.selected) {
                self.pan = bounds.center() - base - egui::vec2(p.x, p.y) * self.zoom;
            }
            self.center_pending = false;
        }
        let mut origin = base + self.pan;
        let hover = ctx.input(|i| i.pointer.hover_pos()).filter(|p| bounds.contains(*p));

        // Scroll-wheel zoom works even if the inline TextEdit currently owns keyboard focus.
        if let Some(mouse) = hover {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                let old = self.zoom;
                let new = (old * (scroll * 0.0018).exp()).clamp(0.30, 2.5);
                self.pan = (mouse - base) - (mouse - base - self.pan) * (new / old);
                self.zoom = new;
                origin = base + self.pan;
                ctx.request_repaint();
            }
        }

        // Wheel-button (middle mouse) drag pans, including while typing.
        // We track the pointer directly so a focused TextEdit cannot swallow the gesture.
        let (middle_pressed, middle_down, delta) = ctx.input(|i| (
            i.pointer.button_pressed(PointerButton::Middle),
            i.pointer.button_down(PointerButton::Middle), i.pointer.delta(),
        ));
        if middle_pressed && hover.is_some() { self.middle_pan = true; }
        if !middle_down { self.middle_pan = false; }
        if self.middle_pan && middle_down {
            self.pan += delta;
            origin = base + self.pan;
            ctx.request_repaint();
        }
        // A secondary click cancels the active tool without opening a context menu.
        // Deliberately check input, not only response.secondary_clicked(), so it also
        // works when the pointer is above an inline editor or canvas object.
        let cancelled_tool = ctx.input(|i|i.pointer.button_pressed(PointerButton::Secondary))
            && (self.insert_shape_mode.is_some() || self.insert_line_mode || self.insert_text_box_mode
                || self.insert_connector_mode);
        if cancelled_tool {
            // The platform reports a context click on button release, one frame
            // after cancellation on press. Keep a latch until release so the
            // canvas context menu never appears while finishing a drawing.
            self.suppress_canvas_context_once=true;
            self.insert_shape_mode=None;
            self.insert_line_mode=false;
            self.insert_text_box_mode=false;
            self.insert_connector_mode=false;
            self.dragging=None;
        }
        let foreground_shape = hover.and_then(|p| self.hit_shape(p, origin));
        let hovered_text_box = if foreground_shape.is_none() { hover.and_then(|p| self.hit_test_text_box(p, origin)) } else { None };
        let hovered = if hovered_text_box.is_none() && foreground_shape.is_none() {
            hover.and_then(|p| self.hit_test(p, &positions, origin, None))
        } else { None };
        let hovered_link = if hovered.is_none() && hovered_text_box.is_none() && foreground_shape.is_none() {
            hover.and_then(|p| self.hit_test_link(p, &positions, origin))
        } else { None };
        let hovered_connector = if hovered.is_none() && foreground_shape.is_none() && hovered_text_box.is_none()
            && hovered_link.is_none() { hover.and_then(|p|self.hit_connector(p,&positions,origin)) } else {None};
        let hovered_line = if hovered.is_none() && hovered_text_box.is_none() && foreground_shape.is_none()
            && hovered_connector.is_none() && hovered_link.is_none() {hover.and_then(|p|self.hit_line(p,origin))} else {None};
        // Empty Frame interiors can be dragged, but foreground content stays clickable.
        let hovered_shape=foreground_shape.or_else(|| {
            if hovered.is_none() && hovered_text_box.is_none() && hovered_link.is_none()
                && hovered_connector.is_none() && hovered_line.is_none() {
                hover.and_then(|p|self.hit_frame_interior(p,origin))
            } else {None}
        });
        let line_endpoint_hover = hover.and_then(|p| self.hit_line_endpoint(p,origin));
        let connector_waypoint_hover=hover.and_then(|p|self.connector_waypoint_hit(p,&positions,origin));
        let connector_label_hover=hover.and_then(|p|self.connector_label_hit(p,&positions,origin));
        let shape_resize_hover = hover.and_then(|p| self.selected_shape.and_then(|id|
            self.resize_handle_at(id,origin,p).map(|edge|(id,edge))));
        // Connection points are visible/active only in connector or line-drawing mode.
        // Selecting a normal shape should expose the square resize handle instead.
        let anchor_hover = if self.insert_shape_mode.is_none() && !self.insert_line_mode && !self.insert_text_box_mode {
            hover.and_then(|p|self.find_anchor(p,&positions,origin))
        } else {None};
        let endpoint_hover = hover.and_then(|p| self.selected_connector.and_then(|id| {
            self.connector_screen_points(id,&positions,origin).and_then(|(a,b)| {
                if p.distance_sq(a) < 100.0 {Some((id,true))}
                else if p.distance_sq(b) < 100.0 {Some((id,false))} else {None}
            })
        }));
        let resize_hover = hover.and_then(|p| self.selected_text_box.filter(|id| {
            self.text_box_resize_handle(*id, origin).is_some_and(|rect| rect.contains(p))
        }));
        let modifiers = ctx.input(|i| i.modifiers);
        if response.clicked_by(PointerButton::Primary) && !self.middle_pan {
            if hovered_connector.is_none() && !modifiers.shift && !modifiers.command {
                self.selected_connectors.clear();
            }
            if let Some(from) = self.link_origin.take() {
                if let Some(to) = hovered {
                    self.create_link(from, to);
                } else {
                    self.status = "Link creation cancelled".into();
                }
            } else if let Some(kind) = self.insert_shape_mode {
                // Click creates default size; drag creates a rectangle matching the gesture.
                if let Some(pointer) = hover {
                    self.add_shape_at(kind,pointer,origin);
                }
            } else if self.insert_line_mode {
                // A free line requires a nonzero-length drag.
            } else if self.insert_text_box_mode {
                if let Some(pointer) = hover { self.insert_text_box_at(pointer, origin); }
            } else if let Some(id) = hovered_connector {
                if modifiers.shift||modifiers.command {
                    if !self.selected_connectors.insert(id){self.selected_connectors.remove(&id);}
                } else {self.selected_connectors.clear();self.selected_connectors.insert(id);}
                self.selected_line=None;
                self.selected_connector=Some(id);
                self.selected_shape=None;
                self.selected_link=None;
                self.canvas_selection.clear();
                self.selection.clear();
            } else if let Some(id) = hovered_shape {
                self.select_shape(id,modifiers.shift||modifiers.command);
            } else if let Some(id) = hovered_line {
                self.select_line(id,modifiers.shift||modifiers.command);
            } else if let Some(link) = hovered_link {
                self.finish_edit(true);
                self.finish_text_box_edit(true);
                self.selected_link = Some(link);
                self.selected_text_box = None;
                self.selected_shape = None;
                self.canvas_selection.clear();
                self.selection.clear();
            } else if let Some(id) = hovered_text_box {
                if modifiers.shift || modifiers.command {
                    self.canvas_selection.insert(CanvasTarget::TextBox(id));
                } else {self.select_text_box(id);}
            } else if let Some(id) = hovered {
                if modifiers.shift || modifiers.command { self.toggle_selection(id); }
                else { self.select(id); }
            } else if !modifiers.shift && !modifiers.command {
                self.selected_link = None;
                self.finish_text_box_edit(true);
                self.selected_text_box = None;
                self.selected_shape = None;
                self.selected_line = None;
                self.selected_connector = None;
                self.canvas_selection.clear();
                self.selection.clear();
            }
        }
        if response.double_clicked() && !self.middle_pan {
            if let Some(id) = hovered_shape { self.start_shape_edit(id); }
            else if let Some(id)=hovered_connector {self.begin_connector_edit(id);}
            else if let Some(id) = hovered_text_box { self.start_text_box_edit(id, false); }
            else if let Some(id) = hovered { self.start_edit(id, false); }
        }
        // At the drag threshold the current pointer may already have moved off
        // the small circle. Identify its source from the original press position.
        let pressed_anchor=ctx.input(|i|i.pointer.press_origin())
            .filter(|p|bounds.contains(*p))
            .and_then(|p|self.find_anchor(p,&positions,origin));
        if response.drag_started_by(PointerButton::Primary) && !self.middle_pan {
            self.dragging = Some(if let Some(kind)=self.insert_shape_mode {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                Drag::CreateShape{kind,start:pointer,current:pointer}
            } else if self.insert_line_mode {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                Drag::CreateLine{start:pointer,current:pointer}
            } else if let Some((id,index))=connector_waypoint_hover {
                self.begin_connector_waypoint_drag(id,&positions,origin);Drag::ConnectorWaypoint(id,index)
            } else if let Some(id)=connector_label_hover.filter(|id|self.selected_connector==Some(*id)) {
                self.checkpoint();Drag::ConnectorLabel(id)
            } else if let Some((id,is_start))=line_endpoint_hover {
                self.checkpoint();Drag::LineEndpoint(id,is_start)
            } else if let Some(from) = pressed_anchor.or(anchor_hover) {
                Drag::Connector(from)
            } else if let Some((id,is_from)) = endpoint_hover {
                Drag::RetargetConnector(id,is_from)
            } else if let Some((id,edge)) = shape_resize_hover {
                self.shape_resize_aspect=self.doc.shape(id).map(|s|s.width/s.height.max(1.0));
                self.checkpoint();
                Drag::ResizeShape(id,edge)
            } else if let Some(id)=hovered_line {
                if !self.canvas_selection.contains(&CanvasTarget::Line(id)) {self.select_line(id,modifiers.shift||modifiers.command);}
                self.checkpoint();
                Drag::Line(id)
            } else if let Some(id) = hovered_shape {
                if !self.canvas_selection.contains(&CanvasTarget::Shape(id)) {
                    self.select_shape(id,modifiers.shift||modifiers.command);
                }
                self.drag_moving = self.selection_roots();
                self.checkpoint();
                Drag::Shape(id)
            } else if let Some(id) = resize_hover {
                self.finish_text_box_edit(true);
                self.checkpoint();
                Drag::ResizeTextBox(id)
            } else if let Some(id) = hovered_text_box {
                self.select_text_box(id);
                self.checkpoint();
                Drag::TextBox(id)
            } else if let Some(id) = hovered.filter(|id| self.editing != Some(*id)) {
                if !self.selection.contains(&id) { self.select(id); }
                self.drag_moving = self.selection_roots();
                self.checkpoint();
                Drag::Node(id)
            } else {
                let pointer = ctx.input(|i| i.pointer.interact_pos()).unwrap_or(bounds.center());
                Drag::Marquee { start: pointer, current: pointer, additive: modifiers.shift || modifiers.command }
            });
        }
        if response.dragged_by(PointerButton::Primary) && !self.middle_pan {
            match self.dragging {
                Some(Drag::CreateShape{kind,start,..})=>{
                    let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(start);
                    self.dragging=Some(Drag::CreateShape{kind,start,current:pointer});
                }
                Some(Drag::CreateLine{start,..})=>{
                    let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(start);
                    self.dragging=Some(Drag::CreateLine{start,current:Self::constrained_line_end(start,pointer,modifiers.shift)});
                }
                Some(Drag::Line(_))=>{self.move_canvas_selection(delta);}
                Some(Drag::LineEndpoint(id,is_start))=>{
                    if let Some(line)=self.doc.line_mut(id) {if !line.locked {
                        let fixed=if is_start {line.end} else {line.start};
                        let moved=if is_start {&mut line.start}else{&mut line.end};
                        moved.x+=delta.x/self.zoom;moved.y+=delta.y/self.zoom;
                        if modifiers.shift {
                            let snapped=Self::constrained_line_end(Pos2::new(fixed.x,fixed.y),Pos2::new(moved.x,moved.y),true);
                            moved.x=snapped.x;moved.y=snapped.y;
                        }
                    }}
                }
                Some(Drag::Marquee { start, additive, .. }) => {
                    let pointer = ctx.input(|i| i.pointer.interact_pos()).unwrap_or(start);
                    self.dragging = Some(Drag::Marquee { start, current: pointer, additive });
                }
                Some(Drag::Node(_)) => {
                    let ids = self.drag_moving.clone();
                    for id in ids {
                        if let Some(n) = self.doc.node_mut(id) {
                            n.offset.x += delta.x / self.zoom;
                            n.offset.y += delta.y / self.zoom;
                        }
                    }
                    self.move_canvas_selection(delta);
                }
                Some(Drag::Shape(_)) => {
                    self.move_canvas_selection(delta);
                    for id in self.drag_moving.clone() {
                        if let Some(n) = self.doc.node_mut(id) {
                            n.offset.x += delta.x/self.zoom;
                            n.offset.y += delta.y/self.zoom;
                        }
                    }
                }
                Some(Drag::ResizeShape(id,edge)) => {
                    self.resize_shape_by(id,edge,delta,modifiers.shift);
                }
                Some(Drag::ConnectorWaypoint(id,index))=>{
                    if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id) {
                        if let Some(p)=c.waypoints.get_mut(index) {p.x+=delta.x/self.zoom;p.y+=delta.y/self.zoom;}
                    }
                }
                Some(Drag::ConnectorLabel(id))=>{
                    if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id) {
                        c.label_offset.x+=delta.x/self.zoom;c.label_offset.y+=delta.y/self.zoom;
                    }
                }
                Some(Drag::Connector(_)) | Some(Drag::RetargetConnector(..)) => {}
                Some(Drag::TextBox(id)) => {
                    if let Some(box_) = self.doc.text_box_mut(id) {
                        box_.position.x += delta.x / self.zoom;
                        box_.position.y += delta.y / self.zoom;
                    }
                }
                Some(Drag::ResizeTextBox(id)) => {
                    if let Some(box_) = self.doc.text_box_mut(id) {
                        box_.width = (box_.width + delta.x / self.zoom).clamp(120.0, 2500.0);
                        box_.height = (box_.height + delta.y / self.zoom).clamp(50.0, 2500.0);
                    }
                }
                None => {}
            }
            origin = base + self.pan;
            ctx.request_repaint();
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            match self.dragging {
                Some(Drag::CreateShape{kind,start,current})=>{
                    self.add_dragged_shape(kind,start,current,origin);
                }
                Some(Drag::CreateLine{start,current})=>{
                    if current.distance_sq(start)>16.0 {
                        self.add_line_screen(start,current,origin);
                    }
                }
                Some(Drag::Marquee { start, current, additive }) => {
                    let screen_selection = Rect::from_two_pos(start, current);
                    let current_positions = self.positions();
                    let mut chosen = if additive { self.selection.clone() } else { HashSet::new() };
                    let mut first = None;
                    for node in &self.doc.nodes {
                        if let Some(&position) = current_positions.get(&node.id) {
                            if screen_selection.intersects(self.screen_rect(node, position, origin)) {
                                if first.is_none() { first = Some(node.id); }
                                chosen.extend(self.doc.members_of(node.id));
                            }
                        }
                    }
                    self.selection = chosen;
                    if !additive {self.canvas_selection.clear();}
                    for &id in &self.selection {self.canvas_selection.insert(CanvasTarget::Node(id));}
                    for box_ in &self.doc.text_boxes {
                        if self.text_box_screen_rect(box_.id,origin).is_some_and(|r|screen_selection.intersects(r)) {
                            self.canvas_selection.insert(CanvasTarget::TextBox(box_.id));
                        }
                    }
                    for shape in &self.doc.shapes {
                        if shape.visible && self.shape_screen_rect(shape.id,origin).is_some_and(|r|screen_selection.intersects(r)) {
                            self.canvas_selection.insert(CanvasTarget::Shape(shape.id));
                            self.selected_shape=Some(shape.id);
                        }
                    }
                    for line in &self.doc.lines {
                        if line.visible && self.line_screen_rect(line.id,origin).is_some_and(|r|screen_selection.intersects(r)) {
                            self.canvas_selection.insert(CanvasTarget::Line(line.id));self.selected_line=Some(line.id);
                        }
                    }
                    if let Some(id) = first { self.selected = id; }
                }
                Some(Drag::Node(id)) if self.drag_moving.len() == 1 && self.doc.parent(id).is_some() => {
                    let after_drag = self.positions();
                    if let Some(pointer) = hover {
                        match self.drop_at(pointer, id, &after_drag, origin) {
                            Some(DropIntent::Child(target)) => { self.doc.reparent(id, target); }
                            Some(DropIntent::Before(sibling)) => { self.doc.move_next_to(id, sibling, false); }
                            Some(DropIntent::After(sibling)) => { self.doc.move_next_to(id, sibling, true); }
                            None => {} // Empty space: preserve manual offsets.
                        }
                    }
                }
                Some(Drag::Connector(from)) => {
                    if let Some(point)=hover {
                        if let Some(to)=self.find_anchor(point,&self.positions(),origin) {
                            if to.target!=from.target {
                                self.checkpoint();
                                let corrected_from=if let Some(source_rect)=self.object_screen_rect(from.target,&positions,origin) {
                                    let dest=self.object_screen_rect(to.target,&positions,origin)
                                        .map(|r|Self::anchor_position(r,to.anchor)).unwrap_or(point);
                                    ConnectorEnd{target:from.target,anchor:Self::facing_source_anchor(source_rect,dest)}
                                }else{from};
                                self.selected_connector=self.doc.add_connector(corrected_from,to,false);
                                if let Some(id)=self.selected_connector {if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.route=self.insert_connector_route;}}
                            }
                        }
                    }
                }
                Some(Drag::RetargetConnector(id,from_end)) => {
                    if let Some(point)=hover {
                        if let Some(end)=self.connector_target_near(point,&self.positions(),origin) {
                            if let Some(original)=self.doc.connectors.iter().find(|c|c.id==id).cloned() {
                                let other=if from_end {original.to.target}else{original.from.target};
                                if end.target!=other {
                                    self.checkpoint();
                                    if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id) {
                                        if from_end {c.from=end}else{c.to=end}
                                    }
                                }
                            }
                        }
                    }
                }
                Some(Drag::ResizeShape(id,_)) => {
                    if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::StickyNote) {
                        self.finish_grid_sticky_resize(id);
                    }else if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Frame) {
                        self.finish_frame_grid_resize(id);
                    }
                }
                Some(Drag::Shape(id)) => {
                    if !modifiers.alt {
                        self.snap_moved_shapes();
                        self.apply_alignment_snap(id,origin);
                    }
                    // Commit the blue/red preview exactly once on mouse release.
                    // The pointer can be outside the canvas when detaching.
                    if let Some(pointer) = ctx.input(|i|i.pointer.interact_pos()).or(hover) {
                        self.update_frame_membership_on_drop(id, pointer, origin);
                    }
                }
                _ => {},
            }
            self.dragging = None;
            self.drag_moving.clear();
            self.drop_intent = None;
            self.shape_resize_aspect=None;
        }
        if response.secondary_clicked() && !cancelled_tool {
            self.context_target = hovered;
            self.context_text_box = hovered_text_box;
            self.context_shape = hovered_shape;
            self.context_line=hovered_line;
            self.context_connector = hovered_connector;
            if let Some(point)=ctx.input(|i|i.pointer.interact_pos()).or(hover) {
                let v=(point-origin)/self.zoom;
                self.context_connector_point=Offset{x:v.x,y:v.y};
            }
            self.context_link = hovered_link;
            if let Some(id) = hovered_shape {
                let frame_with_other_selection = self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Frame)
                    && self.canvas_selection.iter().any(|&t|t!=CanvasTarget::Shape(id));
                if frame_with_other_selection {
                    // Preserve the chosen children so the frame context menu can
                    // attach them, while also highlighting the actual frame.
                    self.canvas_selection.insert(CanvasTarget::Shape(id));
                    self.selected_shape=Some(id);
                    self.selected_text_box=None;
                    self.selected_line=None;
                } else if !self.canvas_selection.contains(&CanvasTarget::Shape(id)) {
                    self.select_shape(id,false);
                }
            }
            if let Some(id) = hovered_connector {
                self.selected_connector=Some(id);
                if !self.selected_connectors.contains(&id) {
                    self.selected_connectors.clear();self.selected_connectors.insert(id);
                }
                self.canvas_selection.clear();
            }
            if let Some(id) = hovered_line {if !self.canvas_selection.contains(&CanvasTarget::Line(id)) {self.select_line(id,false);}}
            if let Some(link) = hovered_link { self.selected_link = Some(link); }
            if let Some(id) = hovered_text_box { self.select_text_box(id); }
            else if let Some(id) = hovered {
                self.selected_link = None;
                if !self.selection.contains(&id) { self.select(id); }
            }
        }
        let positions = self.positions();
        origin = base + self.pan;
        self.drop_intent = if let Some(Drag::Node(id)) = self.dragging {
            if self.drag_moving.len() == 1 && self.doc.parent(id).is_some() {
                hover.and_then(|p| self.drop_at(p, id, &positions, origin))
            } else { None }
        } else { None };
        self.snap_guides=if !modifiers.alt {
            if let Some(Drag::Shape(id))=self.dragging {
                self.alignment_guides_for(id,origin).1
            }else{Vec::new()}
        }else{Vec::new()};
        if self.grid.visible { self.draw_dotted_grid(&painter, bounds, origin); }
        for (a,b) in &self.snap_guides {
            painter.line_segment([*a,*b],Stroke::new(1.0,Color32::from_rgb(58,158,213)));
        }
        if let Some(kind)=self.insert_shape_mode {
            if let Some(Drag::CreateShape{start,current,..})=self.dragging {
                Self::draw_shape_outline(&painter,Rect::from_two_pos(start,current),kind,Stroke::new(1.8,ACCENT));
            } else if let Some(point)=hover {
                let size=match kind {ShapeKind::Text=>(260.0,90.0),ShapeKind::StickyNote=>(240.0,160.0),ShapeKind::Frame=>(360.0,240.0),_=>(180.0,100.0)};
                Self::draw_shape_outline(&painter,Rect::from_min_size(point,egui::vec2(size.0,size.1)*self.zoom),kind,Stroke::new(1.3,ACCENT));
            }
        }
        if let Some(Drag::CreateLine{start,current})=self.dragging {
            painter.line_segment([start,current],Stroke::new(2.0,ACCENT));
            painter.circle_filled(start,3.5,ACCENT);
        }
        if self.insert_text_box_mode {
            if let Some(pointer) = hover {
                painter.rect_stroke(Rect::from_min_size(pointer, egui::vec2(300.0, 100.0) * self.zoom),
                    4.0, Stroke::new(1.0, ACCENT), StrokeKind::Outside);
            }
        }
        if let Some(Drag::Marquee { start, current, .. }) = self.dragging {
            let rect = Rect::from_two_pos(start, current).intersect(bounds);
            painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(69, 121, 219, 24));
            painter.rect_stroke(rect, 3.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
        }

        // Frames act as translucent background areas; their members render on top.
        self.paint_frame_backgrounds(&painter,origin);
        // Free connectors are behind all nodes and shapes, independently of tree edges.
        self.paint_canvas_connectors(&painter,&positions,origin);
        if let Some(Drag::Connector(from)) = self.dragging {
            if let (Some(rect),Some(pointer))=(self.object_screen_rect(from.target,&positions,origin),hover) {
                let to=self.find_anchor(pointer,&positions,origin).filter(|end|end.target!=from.target);
                let end=to.and_then(|target|self.object_screen_rect(target.target,&positions,origin)
                    .map(|r|Self::anchor_position(r,target.anchor))).unwrap_or(pointer);
                let source=ConnectorEnd{target:from.target,anchor:Self::facing_source_anchor(rect,end)};
                let start=Self::anchor_position(rect,source.anchor);
                let mut preview=crate::canvas_core::CanvasConnector::new(0,source,
                    to.unwrap_or(ConnectorEnd{target:from.target,anchor:from.anchor}),false);
                preview.route=self.insert_connector_route;
                let path=crate::diagram_core::route_points(&preview,
                    Offset{x:start.x,y:start.y},Offset{x:end.x,y:end.y});
                for pair in path.windows(2) {
                    painter.line_segment([Pos2::new(pair[0].x,pair[0].y),Pos2::new(pair[1].x,pair[1].y)],
                        Stroke::new(1.7,ACCENT));
                }
            }
        }
        // Paint curves behind the cards.
        for node in &self.doc.nodes {
            let Some(parent) = node.parent else { continue; };
            let (Some(&a), Some(&b)) = (positions.get(&parent), positions.get(&node.id)) else { continue; };
            let parent_node = self.doc.node(parent).unwrap();
            let parent_rect = self.screen_rect(parent_node, a, origin);
            let child_rect = self.screen_rect(node, b, origin);
            let start = parent_rect.right_center();
            let end = child_rect.left_center();
            let bend = ((end.x - start.x) * 0.48).max(24.0);
            let c1 = start + egui::vec2(bend, 0.0);
            let c2 = end - egui::vec2(bend, 0.0);
            let tint = TINTS[self.tint_index(node.id)];
            painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
                [start, c1, c2, end], false, Color32::TRANSPARENT,
                Stroke::new((2.1 * self.zoom).max(1.1), tint.line),
            )));
        }

        // Association links are independent of the hierarchical tree. They
        // re-anchor to the moving node and never influence automatic layout.
        self.paint_links(&painter, &positions, origin);

        // Groups are independent of the parent-child tree. A faint outline
        // indicates members and they can be moved/selected together.
        for group in &self.doc.groups {
            let mut outline: Option<Rect> = None;
            for &id in &group.members {
                if let (Some(node), Some(&pos)) = (self.doc.node(id), positions.get(&id)) {
                    let r = self.screen_rect(node, pos, origin);
                    outline = Some(outline.map_or(r, |old| old.union(r)));
                }
            }
            if let Some(rect) = outline {
                let rect = rect.expand(13.0);
                painter.rect_stroke(rect, 12.0, Stroke::new(1.1, Color32::from_rgb(155, 166, 191)), StrokeKind::Inside);
            }
        }

        for node in &self.doc.nodes {
            let Some(&pos) = positions.get(&node.id) else { continue; };
            let rect = self.screen_rect(node, pos, origin);
            if !rect.intersects(bounds) { continue; }
            let depth = self.doc.depth(node.id);
            let root = node.parent.is_none();
            let focused = self.selection.contains(&node.id);
            let is_drop_target = self.drop_intent == Some(DropIntent::Child(node.id));
            let tint = TINTS[self.tint_index(node.id)];
            let radius = if root { 24.0 } else if depth == 1 { 17.0 } else { 13.0 };
            let root_color = node.color.map(|i| TINTS[i as usize % TINTS.len()].line).unwrap_or(ACCENT);
            let fill = if root { root_color } else if self.dark {
                Color32::from_rgb(43 + (self.tint_index(node.id) % 3) as u8 * 4, 49, 63)
            } else if depth >= 2 { blend_towards_white(tint.fill, 0.42) } else { tint.fill };
            if root {
                painter.rect_filled(rect.translate(egui::vec2(0.0, 3.0 * self.zoom)), radius * self.zoom,
                    Color32::from_rgba_premultiplied(56, 87, 161, 30));
            }
            painter.rect_filled(rect, radius * self.zoom, fill);
            let matches_search = self.search_open && !self.search_query.trim().is_empty()
                && node.title.to_lowercase().contains(&self.search_query.trim().to_lowercase());
            painter.rect_stroke(rect, radius * self.zoom,
                Stroke::new(if focused || is_drop_target || matches_search { 2.1 } else { 1.0 },
                    if is_drop_target { Color32::from_rgb(25, 164, 115) }
                    else if focused { Color32::from_rgb(54, 114, 243) }
                    else if matches_search { Color32::from_rgb(238, 157, 32) }
                    else { tint.border }), StrokeKind::Inside);
            if self.editing == Some(node.id) { continue; }
            painter.text(rect.center(), Align2::CENTER_CENTER, &node.title,
                FontId::proportional((if root { 19.0 } else if depth == 1 { 16.5 } else { 15.0 }) * self.zoom),
                if root { Color32::WHITE } else if self.dark { Color32::from_rgb(242, 246, 251) } else { INK });
            if node.collapsed && !self.doc.children(node.id).is_empty() {
                let child_count = self.doc.children(node.id).len();
                let badge_center = rect.right_center() + egui::vec2(10.0, 0.0);
                painter.circle_filled(badge_center, 9.0, ACCENT);
                painter.text(badge_center, Align2::CENTER_CENTER, format!("{child_count}"),
                    FontId::proportional(10.0), Color32::WHITE);
            }
        }

        // A thin insertion marker indicates an automatic sibling placement.
        if let Some(intent @ (DropIntent::Before(_) | DropIntent::After(_))) = self.drop_intent {
            let target = intent.target();
            if let (Some(n), Some(&pos)) = (self.doc.node(target), positions.get(&target)) {
                let rect = self.screen_rect(n, pos, origin);
                let y = if matches!(intent, DropIntent::After(_)) { rect.bottom() + 8.0 } else { rect.top() - 8.0 };
                let left = Pos2::new(rect.left(), y);
                let right = Pos2::new(rect.right(), y);
                painter.line_segment([left, right], Stroke::new(2.5, ACCENT));
                painter.circle_filled(left, 3.0, ACCENT);
            }
        }

        // Free-floating text annotations are independent from tree nodes.
        // They follow the canvas transform and never affect automatic layout.
        for box_ in &self.doc.text_boxes {
            let Some(rect) = self.text_box_screen_rect(box_.id, origin) else { continue; };
            if !rect.intersects(bounds) { continue; }
            let selected = self.canvas_selection.contains(&CanvasTarget::TextBox(box_.id));
            let fill = if box_.fill_transparent { Color32::TRANSPARENT }
                else { box_.fill_color.map(color_from_rgb).unwrap_or_else(|| {
                    if self.dark { Color32::from_rgb(36, 43, 59) } else { Color32::WHITE }
                }) };
            let border = box_.border_color.map(color_from_rgb).unwrap_or_else(|| {
                if self.dark { Color32::from_rgb(89, 99, 123) } else { Color32::from_rgb(215, 221, 235) }
            });
            painter.rect_filled(rect, 5.0, fill);
            if !box_.border_none {
                painter.rect_stroke(rect, 5.0, Stroke::new(1.2, border), StrokeKind::Inside);
            }
            if selected {
                painter.rect_stroke(rect, 5.0,
                    Stroke::new(1.6, ACCENT), StrokeKind::Inside);
                if let Some(handle) = self.text_box_resize_handle(box_.id, origin) {
                    painter.rect_filled(handle, 2.0, Color32::WHITE);
                    painter.rect_stroke(handle, 2.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
                }
            }
            if self.editing_text_box != Some(box_.id) {
                let clipped = painter.with_clip_rect(rect.shrink(10.0));
                let text = if box_.text.is_empty() && selected {
                    "Enter text".to_owned()
                } else { box_.text.clone() };
                let text_color = if box_.fill_color.is_some() { INK }
                    else if self.dark { Color32::WHITE } else { INK };
                let galley = clipped.layout(text, FontId::proportional(15.5 * self.zoom),
                    text_color, (rect.width() - 30.0 * self.zoom).max(20.0));
                clipped.galley(rect.left_top() + egui::vec2(15.0, 13.0) * self.zoom,
                    galley, text_color);
            }
        }
        // Free shapes are drawn above legacy annotations; hit-testing uses the same priority.
        self.paint_canvas_shapes(&painter, origin);
        self.paint_frame_headers(&painter,origin);
        self.paint_frame_drop_feedback(&painter,origin,
            ctx.input(|i|i.pointer.interact_pos()).or(hover));
        self.paint_canvas_groups(&painter, &positions, origin);
        if let Some(id) = self.editing_text_box {
            if let Some(rect) = self.text_box_screen_rect(id, origin) {
                let editing_bg = if self.doc.text_box(id).is_some_and(|b| b.fill_transparent) {
                    Color32::TRANSPARENT
                } else {
                    self.doc.text_box(id).and_then(|b| b.fill_color)
                        .map(color_from_rgb).unwrap_or(if self.dark {
                            Color32::from_rgb(38, 46, 65)
                        } else { Color32::WHITE })
                };
                let editing_border = self.doc.text_box(id).and_then(|b| b.border_color)
                    .map(color_from_rgb).unwrap_or(ACCENT);
                let custom_fill = self.doc.text_box(id).is_some_and(|b| b.fill_color.is_some());
                let input = egui::TextEdit::multiline(&mut self.text_box_editor)
                    .id(egui::Id::unique(("annotation-text-edit", id)))
                    .font(FontId::proportional(15.5 * self.zoom))
                    .text_color(if custom_fill { INK } else if self.dark { Color32::WHITE } else { INK })
                    .desired_width((rect.width() - 32.0 * self.zoom).max(50.0))
                    .desired_rows(((rect.height() / (23.0 * self.zoom)).floor() as usize).max(2))
                    .margin(egui::Margin::symmetric(12, 10))
                    .frame(egui::Frame::new()
                        .fill(editing_bg)
                        .stroke(Stroke::new(1.6, editing_border)).corner_radius(5));
                let edit_response = ui.place(rect, input);
                if self.text_box_focus_pending {
                    edit_response.request_focus();
                    self.text_box_focus_pending = false;
                }
                if edit_response.changed() { ctx.request_repaint(); }
                let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                if escape && !self.ime_composing && !just_committed_ime {
                    self.finish_text_box_edit(false);
                } else if edit_response.lost_focus() && !self.ime_composing && !just_committed_ime {
                    // Middle mouse panning, zooming, and ordinary canvas clicks
                    // do not destroy an uncommitted IME composition.
                    if self.middle_pan { edit_response.request_focus(); }
                    else { self.finish_text_box_edit(true); }
                }
            }
        }

        // Inline editor floats above a fixed-size card. Its stable ID
        // retains the IME composition state across redraws and zoom/pan gestures.
        if let Some(id) = self.editing {
            if let (Some(node), Some(&pos)) = (self.doc.node(id), positions.get(&id)) {
                let anchor = self.screen_rect(node, pos, origin);
                // A fixed, comfortably padded editor floats over the card.
                // Its width never changes during IME composition or typing, so
                // neither the card nor neighboring nodes jump around.
                // Edit within the normal node footprint. TextEdit handles
                // horizontal scrolling for long uncommitted text; the tree is
                // relaid out only when the title is committed.
                let edit_lines = self.edit_text.lines().count().max(1) as f32;
                let edit_height = anchor.height().max(anchor.height() + (edit_lines - 1.0) * 24.0 * self.zoom);
                let rect = Rect::from_center_size(anchor.center(), egui::vec2(anchor.width(), edit_height));
                let edit_id = egui::Id::unique(("node-inline-edit", id));
                let depth = self.doc.depth(id);
                let font = if depth == 0 { 19.0 } else if depth == 1 { 16.5 } else { 15.0 };
                // Prevent Japanese composition from touching the edit frame.
                // Keep text at a fixed inset while IME content is being composed.
                let pad_x = (18.0 * self.zoom).round().clamp(10.0, 28.0) as i8;
                let pad_y = (9.0 * self.zoom).round().clamp(5.0, 13.0) as i8;
                let edit_rows=self.edit_text.lines().count().max(1);
                let input = egui::TextEdit::multiline(&mut self.edit_text)
                    .id(edit_id)
                    .align(Align2::CENTER_CENTER)
                    .font(FontId::proportional(font * self.zoom))
                    .desired_width((rect.width() - 2.0 * f32::from(pad_x) - 14.0).max(48.0))
                    .margin(egui::Margin::symmetric(pad_x, pad_y))
                    .desired_rows(edit_rows)
                    .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter)))
                    .event_filter(egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, ..Default::default() })
                    .frame(egui::Frame::new().fill(if self.dark { Color32::from_rgb(41, 48, 66) } else { Color32::WHITE })
                        .stroke(Stroke::new(2.0, ACCENT)).corner_radius(10));
                let edit_response = ui.place(rect, input);
                if self.edit_focus_pending {
                    edit_response.request_focus();
                    self.edit_focus_pending = false;
                }
                if self.edit_select_all_pending {
                    let mut state = egui::TextEdit::load_state(&ctx, edit_id).unwrap_or_default();
                    state.cursor.set_char_range(Some(
                        egui::text::CCursorRange::select_all_str(&self.edit_text)));
                    state.store(&ctx, edit_id);
                    edit_response.request_focus();
                    self.edit_select_all_pending = false;
                }
                if edit_response.changed() { ctx.request_repaint(); }
                let (pressed_enter, pressed_escape) = ctx.input(|i| (
                    i.key_pressed(egui::Key::Enter) && !i.modifiers.shift, i.key_pressed(egui::Key::Escape),
                ));
                let ignore_enter_this_frame = std::mem::take(&mut self.edit_ignore_enter_once);
                let is_panning = self.middle_pan;
                if pressed_escape && !self.ime_composing && !just_committed_ime {
                    self.finish_edit(false);
                } else if pressed_enter && !ignore_enter_this_frame && !self.ime_composing && !just_committed_ime {
                    self.finish_edit(true);
                } else if edit_response.lost_focus() && !self.ime_composing && !just_committed_ime {
                    if is_panning || hover.is_some() {
                        // Clicking or dragging the empty canvas must not end editing;
                        // recover focus after a pan gesture (middle button included).
                        edit_response.request_focus();
                    } else {
                        self.finish_edit(true);
                    }
                }
            }
        }
        self.inline_shape_editor(ui,&ctx,origin,just_committed_ime);
        self.inline_connector_editor(ui,&ctx,&positions,origin);
        self.paint_anchor_handles(&painter,&positions,origin,hover);
        // Suppress the release event of the right-click that just cancelled a
        // continuous drawing tool. Do not suppress regular context menus.
        let suppress_context = cancelled_tool || self.suppress_canvas_context_once;
        if !suppress_context { response.context_menu(|ui| {
            if let Some(id)=self.context_shape {
                if ui.button("⧉ Copy").clicked() {self.copy_selected(ui.ctx(),false);ui.close();}
                if ui.button("⎘ Paste").clicked() {self.paste_from_system_clipboard();ui.close();}
                if ui.button("✎ Edit Text").clicked() {self.start_shape_edit(id);ui.close();}
                if ui.button("⧉ Duplicate").clicked(){self.duplicate_canvas_selection();ui.close();}
                if self.doc.shape(id).is_some_and(|s|s.kind!=ShapeKind::Frame) {
                }
                if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Frame) {
                    let count=self.doc.shape(id).map_or(0,|s|s.members.len());
                    ui.label(format!("Items in this frame: {count}"));
                    let selected_others=self.canvas_selection.iter().filter(|&&t|t!=CanvasTarget::Shape(id)).count();
                    if ui.add_enabled(selected_others>0,egui::Button::new(format!("Add {selected_others} Selected Items to Frame"))).clicked(){
                        self.attach_selection_to_frame(id);ui.close();
                    }
                    if ui.button("Add Objects Inside Frame").clicked(){
                        self.attach_contained_to_frame(id);ui.close();
                    }
                    if ui.button("Remove All from Frame").clicked() {
                        self.checkpoint();if let Some(frame)=self.doc.shape_mut(id){frame.members.clear();}
                        self.refresh_frame_grid(id);ui.close();
                    }
                    ui.menu_button("◯ 枠Line Color",|ui|{
                        for (label,color) in [("Blue",[75,124,225]),("Green",[58,159,104]),("赤",[223,85,98]),("Gray",[126,150,198])] {
                            if ui.button(label).clicked(){self.checkpoint();if let Some(frame)=self.doc.shape_mut(id){frame.style.outline=Some(color);}ui.close();}
                        }
                    });
                }
                if ui.button("▢ Create Frame from Selection").clicked(){self.create_frame_for_selection();ui.close();}
                ui.menu_button("☷ Align",|ui|self.arrangement_menu(ui));
                if ui.button("⌫ Delete").clicked() {self.select_shape(id,false);self.delete_selected();ui.close();}
                ui.separator();
                if ui.button("▣ Group").clicked() {self.group_selection();ui.close();}
                if ui.button("▤ Ungroup").clicked() {self.ungroup_selection();ui.close();}
                ui.menu_button("◉ 色",|ui| {
                    for (i,label) in COLOR_LABELS.iter().enumerate() {
                        if Self::color_tile(ui,TINTS[i].fill,TINTS[i].border,label) {
                            self.draw_selected_sticky_color(rgb_of(TINTS[i].fill));
                            ui.close();
                        }
                    }
                });
                ui.menu_button("枠Lines",|ui|{
                    if ui.button("No Border").clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.outline=None;}ui.close();}
                    for (label,color) in [("Blue",[80,126,222]),("Green",[65,164,110]),("Gray",[110,126,155]),("Black",[38,44,60])]{
                        if ui.button(label).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.outline=Some(color);}ui.close();}
                    }
                    ui.separator();
                    for width in [1.0_f32,2.0,3.0,5.0]{
                        if ui.button(format!("{width:.0} px")).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.stroke_width=width;}ui.close();}
                    }
                });
                ui.menu_button("Text Alignment and Size",|ui|{
                    ui.label("Horizontal Alignment");
                    for (label,h) in [("Left",TextHorizontal::Left),("Center",TextHorizontal::Center),("Right",TextHorizontal::Right)]{
                        if ui.button(label).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.text_horizontal=h;}ui.close();}
                    }
                    ui.label("Vertical Alignment");
                    for (label,v) in [("Top",TextVertical::Top),("Center",TextVertical::Center),("Bottom",TextVertical::Bottom)]{
                        if ui.button(label).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.text_vertical=v;}ui.close();}
                    }
                    ui.label("Font Size");
                    for size in [12.0_f32,15.0,18.0,24.0,32.0]{
                        if ui.button(format!("{size:.0} pt")).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.text_size=size;}ui.close();}
                    }
                });
                if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::StickyNote) {
                    if let Some(frame_id)=self.sticky_frame(id) {
                        ui.menu_button("▦ Grid Size",|ui| {
                            for (label,w,h) in [("1×1",1,1),("2×1",2,1),("1×2",1,2),("2×2",2,2)] {
                                if ui.button(label).clicked() {
                                    self.set_grid_sticky_span(id,w,h);
                                    ui.close();
                                }
                            }
                            ui.label(format!("Frame ID: {frame_id}"));
                        });
                    }
                    ui.menu_button("A 文字色",|ui|{
                        for (label,fg) in [("Automatic / Dark",[43,48,60]),("White",[255,255,255]),("Black",[0,0,0]),("濃紺",[35,51,91])] {
                            if ui.button(label).clicked(){
                                let chosen:Vec<_>=self.canvas_selection.iter().filter_map(|t|match t{CanvasTarget::Shape(i)=>Some(*i),_=>None}).collect();
                                self.checkpoint();
                                for i in chosen{if let Some(shape)=self.doc.shape_mut(i){shape.style.text=fg;}}
                                ui.close();
                            }
                        }
                    });
                }
                ui.separator();
                if ui.button("⇧ Bring to Front").clicked() {self.change_selected_shape_z(2);ui.close();}
                if ui.button("↑ Bring Forward").clicked() {self.change_selected_shape_z(1);ui.close();}
                if ui.button("↓ Send Backward").clicked() {self.change_selected_shape_z(-1);ui.close();}
                if ui.button("⇩ Send to Back").clicked() {self.change_selected_shape_z(-2);ui.close();}
                if ui.button(if self.doc.shape(id).is_some_and(|s|s.locked){"Unlock"}else{"Lock"}).clicked() {
                    self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.locked=!s.locked;}ui.close();
                }
            } else if let Some(id)=self.context_line {
                if ui.button("⧉ Duplicate").clicked(){self.duplicate_canvas_selection();ui.close();}
                if ui.button("▣ Group").clicked(){self.group_selection();ui.close();}
                if ui.button("▤ Ungroup").clicked(){self.ungroup_selection();ui.close();}
                if ui.button("⌫ Delete").clicked(){self.delete_selected();ui.close();}
                ui.menu_button("◉ Line Color",|ui| {
                    for (label,color) in [("Gray",[109,128,162]),("Blue",[75,124,225]),("Green",[58,159,104]),("赤",[223,85,98]),("Black",[38,42,53])] {
                        if ui.button(label).clicked(){self.checkpoint();if let Some(l)=self.doc.line_mut(id){l.color=color;}ui.close();}
                    }
                });
                ui.menu_button("━ Line Width",|ui|{
                    for width in [1.0_f32,2.0,3.0,5.0,8.0] {
                        if ui.button(format!("{width:.0}px")).clicked(){self.checkpoint();if let Some(l)=self.doc.line_mut(id){l.width=width;}ui.close();}
                    }
                });
                if ui.button("┄ Solid / Dashed").clicked(){self.checkpoint();if let Some(l)=self.doc.line_mut(id){l.dashed=!l.dashed;}ui.close();}
                ui.menu_button("▧ Arrange Order",|ui|{
                    for (label,d) in [("Bring to Front",2),("一つ前",1),("一つ後ろ",-1),("Send to Back",-2)] {
                        if ui.button(label).clicked(){self.change_selected_line_z(d);ui.close();}
                    }
                });
                ui.menu_button("☷ Align",|ui|self.arrangement_menu(ui));
            } else if let Some(id)=self.context_connector {
                self.connector_context_menu(ui,id,&positions,origin);
            } else if let Some(id) = self.context_text_box {
                if ui.button("✎ Edit Text").clicked() { self.start_text_box_edit(id, false); ui.close(); }
                ui.menu_button("◉ Background Color", |ui| self.text_box_palette(ui, false));
                ui.menu_button("◯ 枠Line Color", |ui| self.text_box_palette(ui, true));
                if ui.button("⌫ Delete").clicked() { self.select_text_box(id); self.delete_selected(); ui.close(); }
            } else if let Some(id) = self.context_target {
                self.context_color(ui);
                if self.selection.len() > 1 && ui.button("☷ Arrange Selection").clicked() { self.align_selection(); ui.close(); }
                ui.separator();
                let attached:Vec<_>=self.doc.links.iter().copied()
                    .filter(|l|l.from==id || l.to==id).collect();
                if !attached.is_empty() {
                    ui.menu_button("⛓ Link解除",|ui|{
                        if ui.button(format!("Remove All ({})",attached.len())).clicked() {
                            self.checkpoint();
                            for l in &attached {self.doc.remove_link(l.from,l.to);}
                            self.selected_link=None;
                            ui.close();
                        }
                        ui.separator();
                        for &link in &attached {
                            let neighbor=if link.from==id {link.to}else{link.from};
                            let name=self.doc.node(neighbor).map(|n|n.title.clone())
                                .unwrap_or_else(||format!("Node {neighbor}"));
                            if ui.button(format!("⛓ {name}")).clicked() {
                                self.checkpoint();self.doc.remove_link(link.from,link.to);
                                self.selected_link=None;ui.close();
                            }
                        }
                    });
                }
                if ui.button("↗ Create Link from This Node").clicked() {
                    self.begin_link(id); ui.close();
                }
                if self.selection.len() == 2 && ui.button("⛓ Link Selected Nodes").clicked() {
                    self.link_two_selected(); ui.close();
                }
                ui.separator();
                if ui.button("▣ Group").clicked() { self.group_selection(); ui.close(); }
                if ui.button("▤ Ungroup").clicked() { self.ungroup_selection(); ui.close(); }
                ui.separator();
                if ui.button("⧉ Copy（選択Nodeのみ）").clicked() { self.copy_selected(ui.ctx(), false); ui.close(); }
                if ui.button("⧉ Copy（子孫を含む）").clicked() { self.copy_selected(ui.ctx(), true); ui.close(); }
                if ui.button("⎘ Paste").clicked() { self.paste_from_system_clipboard(); ui.close(); }
                ui.separator();
                if ui.button("＋ Add Child").clicked() { self.select(id); self.add_child(); ui.close(); }
                if self.doc.parent(id).is_some() {
                    if ui.button("⊕ Add Sibling").clicked() { self.select(id); self.add_sibling(); ui.close(); }
                }
                if id != ROOT {
                    if ui.button("⌫ Delete").clicked() { self.delete_selected(); ui.close(); }
                }
            } else if let Some(link) = self.context_link {
                ui.menu_button("◉ Link Color", |ui| {
                    for (label, rgb) in [("Blue", [80,126,222]), ("Green", [65,164,110]),
                                         ("赤", [217,91,106]), ("Purple", [153,105,217]),
                                         ("Gray", [120,135,160])] {
                        if ui.button(label).clicked() {
                            self.checkpoint();
                            self.doc.set_link_color(link.from, link.to, rgb);
                            ui.close();
                        }
                    }
                });
                if ui.button("⌫ Delete Link").clicked() {
                    self.selected_link = Some(link);
                    self.delete_selected_link(); ui.close();
                }
            } else {
                if ui.button("▨ Place Sticky Note").clicked() {self.insert_shape_mode=Some(ShapeKind::StickyNote);ui.close();}
                if ui.button("／ Draw Line").clicked() {self.insert_shape_mode=None;self.insert_line_mode=true;ui.close();}
                if ui.button("▢ Place Frame").clicked() {self.insert_shape_mode=Some(ShapeKind::Frame);ui.close();}
                if ui.button("▭ Place Rectangle").clicked() {self.insert_shape_mode=Some(ShapeKind::Rectangle);ui.close();}
                if ui.button("◯ Place Ellipse").clicked() {self.insert_shape_mode=Some(ShapeKind::Ellipse);ui.close();}
                if ui.button("T Place Text Box").clicked() { self.insert_shape_mode=None;self.insert_text_box_mode = true; ui.close(); }
            }
        }); }
        if ctx.input(|i|i.pointer.button_released(PointerButton::Secondary)) {
            self.suppress_canvas_context_once=false;
        }
    }
}
