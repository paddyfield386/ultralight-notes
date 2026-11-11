//! Canvas rendering and drag/pan/zoom interactions.
use super::*;

#[cfg(test)]
mod smart_guide_input_tests {
    use super::*;
    #[test]
    fn gradual_drag_can_escape_a_smart_guide(){
        let mut app=MindmapApp::new(Instant::now(),None,false);
        app.doc.nodes.clear();app.selection.clear();app.canvas_selection.clear();app.zoom=1.0;app.snap_to_grid=false;
        let moving=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:100.0,y:-120.0});
        app.doc.add_shape(ShapeKind::Rectangle,Offset{x:150.0,y:100.0});
        app.select_shape(moving,false);
        let ctx=egui::Context::default();
        let mut start=Pos2::ZERO;
        let mut run=|app:&mut MindmapApp,events:Vec<egui::Event>|{
            let input=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(900.0,700.0))),events,..Default::default()};
            let output=ctx.run_ui(input,|ui|{
                let rect=ui.available_rect_before_wrap();let origin=Pos2::new(rect.left()+152.0,rect.center().y);
                if start==Pos2::ZERO {start=app.shape_screen_rect(moving,origin).unwrap().center();}
                app.canvas(ui);
            });output.drop_without_applying_deltas();
            start
        };
        let start=run(&mut app,vec![]);
        run(&mut app,vec![egui::Event::PointerMoved(start),egui::Event::PointerButton{pos:start,button:PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE}]);
        for x in 1..=70 {run(&mut app,vec![egui::Event::PointerMoved(start+egui::vec2(x as f32,0.0))]);}
        let x=app.doc.shape(moving).unwrap().position.x;
        assert!(x>160.0,"shape remained stuck at x={x}");
    }
}

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
        if let Some((rows,cols))=self.pending_table_create.take() {
            self.finish_shape_edit(true);
            self.checkpoint();
            let width=TABLE_CELL_WIDTH*cols as f32;let height=TABLE_CELL_HEIGHT*rows as f32;
            let center=(bounds.center()-origin)/self.zoom;
            let id=self.doc.add_shape(ShapeKind::Table,Offset{x:center.x-width*0.5,y:center.y-height*0.5});
            if let Some(shape)=self.doc.shape_mut(id) {
                shape.width=width;shape.height=height;
                if let Some(table)=shape.table.as_mut(){table.cells=vec![vec![String::new();cols];rows];}
            }
            self.insert_shape_mode=None;self.insert_text_box_mode=false;self.insert_line_mode=false;self.insert_connector_mode=false;
            self.select_shape(id,false);
        }
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
        let resize_hover = hover.and_then(|p| self.selected_text_box.and_then(|id|
            self.text_box_resize_handles(id, origin)?.into_iter()
                .find_map(|(edge, rect)| rect.contains(p).then_some((id, edge)))));
        // Once a drag crosses into the object, the current pointer is no
        // longer over the small handle. Preserve the press-origin hit so
        // inward resizes remain reliable.
        let press_resize = ctx.input(|i| i.pointer.press_origin()).and_then(|p| {
            if let Some(id) = self.selected_shape {
                self.resize_handle_at(id, origin, p).map(|edge| (id, edge))
            } else if let Some(id) = self.selected_text_box {
                self.text_box_resize_handles(id, origin)?.into_iter()
                    .find_map(|(edge, rect)| rect.contains(p).then_some((id, edge)))
            } else { None }
        });
        let modifiers = ctx.input(|i| i.modifiers);
        let table_move_handle_id = ctx.input(|i| i.pointer.press_origin().or_else(||i.pointer.interact_pos()))
            .and_then(|p| self.selected_shape.filter(|id| self.table_move_handle_at(*id, p, origin)))
            .filter(|id| self.doc.shape(*id).is_some_and(|s| s.kind == ShapeKind::Table));
        let table_fill_handle_id=ctx.input(|i|i.pointer.press_origin())
            .and_then(|p|self.selected_shape.filter(|id|self.table_fill_handle_at(*id,p,origin)));
        // Keep ordinary clicks in the active cell editor behaving like text
        // editing. Once the pointer moves far enough, convert the gesture to
        // a table range drag and commit the edited cell.
        if ctx.input(|i| i.pointer.button_pressed(PointerButton::Primary)) {
            self.table_selection_press = self.editing_shape
                .filter(|id| self.doc.shape(*id).is_some_and(|s| s.kind == ShapeKind::Table))
                .and_then(|id| ctx.input(|i| i.pointer.press_origin())
                    .and_then(|p| self.table_cell_at(id, p, origin).map(|(r,c)|(id,r,c,p))));
        }
        if let Some((id, start_row, start_col, start_pos)) = self.table_selection_press {
            let (pointer, down) = ctx.input(|i| (i.pointer.interact_pos(), i.pointer.button_down(PointerButton::Primary)));
            if down && pointer.is_some_and(|p| p.distance_sq(start_pos) >= 36.0) {
                self.finish_shape_edit(true);
                self.select_shape(id, false);
                self.selected_table_cell = Some((id, start_row, start_col));
                self.selected_table_range = Some((id, start_row, start_col, start_row, start_col));
                self.table_selection_drag = Some((id, start_row, start_col));
                self.table_selection_press = None;
            }
            if !down && !ctx.input(|i| i.pointer.button_pressed(PointerButton::Primary)) {
                self.table_selection_press = None;
            }
        }
        let mut inserted_cross_table_reference=false;
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
            } else if self.insert_shape_mode == Some(ShapeKind::StickyNote) && hovered_shape.is_none() && hovered_text_box.is_none()
                && shape_resize_hover.is_none() && resize_hover.is_none() {
                if let Some(pointer) = hover { self.add_shape_at(ShapeKind::StickyNote, pointer, origin); }
            } else if self.insert_shape_mode.is_some() && hovered_shape.is_none() && hovered_text_box.is_none()
                && shape_resize_hover.is_none() && resize_hover.is_none() {
                // Shapes are created on drag release, never on a click.
            } else if self.insert_line_mode {
                // A free line requires a nonzero-length drag.
            } else if self.insert_text_box_mode && hovered_shape.is_none() && hovered_text_box.is_none() {
                // Text boxes are created on drag release, never on a click.
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
                let cross_table_reference=self.editing_shape.filter(|source|
                    self.doc.shape(*source).is_some_and(|s|s.kind==ShapeKind::Table)
                    &&self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Table)
                    &&self.shape_edit_text.trim_start().starts_with('='))
                    .and_then(|source|{
                        let pointer=ctx.input(|i|i.pointer.interact_pos())?;
                        let (r,c)=self.table_cell_at(id,pointer,origin)?;
                        if source==id&&self.editing_table_cell==Some((r,c)){return None;}
                        Some((source,r,c))
                    });
                if let Some((source,r,c))=cross_table_reference {
                    inserted_cross_table_reference=true;
                    let (sr,sc)=self.editing_table_cell.unwrap_or((0,0));
                    let edit_id=egui::Id::new(("inline-table-cell",source,sr,sc));
                    let state_before=egui::TextEdit::load_state(&ctx,edit_id).unwrap_or_default();
                    let cursor_range=state_before.cursor.char_range();
                    let start_char=cursor_range.map_or_else(||self.shape_edit_text.chars().count(),|r|r.primary.index.0.min(r.secondary.index.0));
                    let end_char=cursor_range.map_or(start_char,|r|r.primary.index.0.max(r.secondary.index.0));
                    let start_byte=self.shape_edit_text.char_indices().nth(start_char).map_or(self.shape_edit_text.len(),|(b,_)|b);
                    let end_byte=self.shape_edit_text.char_indices().nth(end_char).map_or(self.shape_edit_text.len(),|(b,_)|b);
                    let mut col=c+1;let mut letters=String::new();while col>0{let rem=(col-1)%26;letters.insert(0,char::from(b'A'+rem as u8));col=(col-1)/26;}
                    let reference=if source==id{format!("{}{}",letters,r+1)}else{format!("{}:{}{}",id,letters,r+1)};
                    self.shape_edit_text.replace_range(start_byte..end_byte,&reference);
                    let token_end=start_byte+reference.len();
                    let cursor=self.shape_edit_text[..token_end].chars().count();
                    let mut state=state_before;
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor))));state.store(&ctx,edit_id);
                    self.shape_edit_focus_pending=true;
                    self.formula_reference_range=Some((start_byte,token_end));
                    self.formula_reference_mode=true;
                    self.formula_reference_target=Some((id,r,c));
                    self.formula_reference_anchor=Some((id,r,c));
                } else {
                let keep_table_editor=self.selected_shape==Some(id)
                    &&self.editing_shape==Some(id)
                    &&self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Table);
                if !keep_table_editor {self.select_shape(id,modifiers.shift||modifiers.command);}
                }
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
        if response.clicked() && !self.middle_pan && !inserted_cross_table_reference {
            if let Some(id)=hovered_shape.filter(|id|self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table)) {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                if let Some((row,col))=self.table_cell_at(id,pointer,origin) {
                    if modifiers.shift {
                        self.finish_shape_edit(true);
                        let (start_row,start_col)=self.selected_table_range.filter(|(table,_,_,_,_)|*table==id)
                            .map(|(_,r0,c0,_,_)|(r0,c0)).or_else(||self.selected_table_cell.filter(|(table,_,_)|*table==id).map(|(_,r,c)|(r,c)))
                            .unwrap_or((row,col));
                        self.select_shape(id,false);
                        self.selected_table_cell=Some((id,row,col));
                        self.selected_table_range=Some((id,start_row,start_col,row,col));
                    } else if self.editing_shape!=Some(id)||self.editing_table_cell!=Some((row,col)) {
                        self.finish_shape_edit(true);
                        self.select_shape(id,false);
                        self.selected_table_cell=Some((id,row,col));
                        self.selected_table_range=Some((id,row,col,row,col));
                    }
                }
            }
        }
        if response.clicked() && !self.middle_pan && !inserted_cross_table_reference {
            if let Some(id)=table_move_handle_id {
                self.finish_shape_edit(true);
                self.select_shape(id, false);
                self.selected_table_cell = None;
                self.selected_table_range = None;
            }
            if let (Some((id, _, _)), Some(pointer)) = (self.selected_table_cell, ctx.input(|i| i.pointer.interact_pos())) {
                if self.shape_screen_rect(id, origin).is_some_and(|rect| !rect.contains(pointer)) {
                    self.finish_shape_edit(true);
                    self.selected_table_cell = None;
                    self.selected_table_range = None;
                    self.table_selection_press = None;
                }
            }
        }
        // A primary drag that starts inside a table selects a rectangular cell
        // range instead of moving the table object.
        if response.drag_started_by(PointerButton::Primary) && !self.middle_pan && table_fill_handle_id.is_some() {
            if let Some(id)=table_fill_handle_id {
                if let Some((_,r0,c0,r1,c1))=self.selected_table_range.filter(|(table,_,_,_,_)|*table==id) {
                    let (top,bottom,left,right)=(r0.min(r1),r0.max(r1),c0.min(c1),c0.max(c1));
                    self.finish_shape_edit(true);
                    self.table_fill_drag=Some((id,top,left,bottom,right,bottom,right));
                }
            }
        }
        if response.drag_started_by(PointerButton::Primary) && !self.middle_pan && table_move_handle_id.is_none() && table_fill_handle_id.is_none() {
            let table_at_press = ctx.input(|i| i.pointer.press_origin())
                .and_then(|p| self.hit_shape(p, origin))
                .filter(|id| self.doc.shape(*id).is_some_and(|s| s.kind == ShapeKind::Table));
            if let Some(id) = table_at_press {
                if let Some(start) = ctx.input(|i| i.pointer.press_origin())
                    .and_then(|p| self.table_cell_at(id, p, origin))
                {
                    self.finish_shape_edit(true);
                    self.select_shape(id, false);
                    self.selected_table_cell = Some((id, start.0, start.1));
                    self.selected_table_range = Some((id, start.0, start.1, start.0, start.1));
                    self.table_selection_drag = Some((id, start.0, start.1));
                }
            }
        }
        if response.dragged_by(PointerButton::Primary) && !self.middle_pan {
            if let Some((id,top,left,bottom,right,_,_))=self.table_fill_drag {
                if let Some((row,col))=ctx.input(|i|i.pointer.interact_pos()).and_then(|p|self.table_cell_at(id,p,origin)) {
                    self.table_fill_drag=Some((id,top,left,bottom,right,row,col));
                    self.selected_table_range=Some((id,top.min(row),left.min(col),bottom.max(row),right.max(col)));
                    self.selected_table_cell=Some((id,row,col));
                }
            }
            if let Some((id, start_row, start_col)) = self.table_selection_drag {
                if let Some((row, col)) = ctx.input(|i| i.pointer.interact_pos())
                    .and_then(|p| self.table_cell_at(id, p, origin))
                {
                    self.selected_table_cell = Some((id, row, col));
                    self.selected_table_range = Some((id, start_row, start_col, row, col));
                }
            }
        }
        if response.double_clicked() && !self.middle_pan {
            if let Some(id)=hovered_shape.filter(|id|self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table)) {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                if let Some((row,col))=self.table_cell_at(id,pointer,origin){self.start_table_cell_edit(id,row,col);}
            } else if let Some(id)=hovered_shape {self.start_shape_edit(id);}
            else if let Some(id)=hovered_connector {self.begin_connector_edit(id);}
            else if let Some(id)=hovered_text_box {self.start_text_box_edit(id,false);}
            else if let Some(id)=hovered {self.start_edit(id,false);}
        }
        // At the drag threshold the current pointer may already have moved off
        // the small circle. Identify its source from the original press position.
        let pressed_anchor=ctx.input(|i|i.pointer.press_origin())
            .filter(|p|bounds.contains(*p))
            .and_then(|p|self.find_anchor(p,&positions,origin));
        if response.drag_started_by(PointerButton::Primary) && !self.middle_pan && self.table_selection_drag.is_none() && self.table_fill_drag.is_none() {
            self.smart_snap_delta=Vec2::ZERO;
            // A press on an existing object cancels insertion mode so the
            // same gesture selects, resizes, or moves that object instead.
            if self.insert_shape_mode.is_some() &&
                (hovered_shape.is_some() || shape_resize_hover.is_some() || hovered_text_box.is_some() || resize_hover.is_some()) {
                self.insert_shape_mode = None;
            }
            if self.insert_text_box_mode && (hovered_shape.is_some() || hovered_text_box.is_some()) {
                self.insert_text_box_mode = false;
            }
            self.dragging = Some(if let Some(kind)=self.insert_shape_mode.filter(|kind| *kind != ShapeKind::StickyNote && *kind != ShapeKind::Table) {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                Drag::CreateShape{kind,start:pointer,current:pointer}
            } else if self.insert_text_box_mode {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                Drag::CreateTextBox{start:pointer,current:pointer}
            } else if self.insert_line_mode {
                let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(bounds.center());
                Drag::CreateLine{start:pointer,current:pointer}
            } else if let Some(id)=table_move_handle_id {
                self.finish_shape_edit(true);
                self.selected_table_cell=None;
                self.selected_table_range=None;
                self.select_shape(id,false);
                self.selected_table_cell=None;
                self.selected_table_range=None;
                self.drag_moving=self.selection_roots();
                self.checkpoint();
                Drag::Shape(id)
            } else if let Some((id,index))=connector_waypoint_hover {
                self.begin_connector_waypoint_drag(id,&positions,origin);Drag::ConnectorWaypoint(id,index)
            } else if let Some(id)=connector_label_hover.filter(|id|self.selected_connector==Some(*id)) {
                self.checkpoint();Drag::ConnectorLabel(id)
            } else if let Some((id,is_start))=line_endpoint_hover {
                self.checkpoint();Drag::LineEndpoint(id,is_start)
            } else if let Some((id,edge)) = press_resize.or(shape_resize_hover) {
                // Selected-shape sides are square resize handles, not ports.
                self.shape_resize_aspect=self.doc.shape(id).map(|s|s.width/s.height.max(1.0));
                self.checkpoint();
                Drag::ResizeShape(id,edge)
            } else if let Some(from) = pressed_anchor.or(anchor_hover) {
                Drag::Connector(from)
            } else if let Some((id,is_from)) = endpoint_hover {
                Drag::RetargetConnector(id,is_from)
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
            } else if let Some((id, edge)) = press_resize.or(resize_hover) {
                self.finish_text_box_edit(true);
                self.checkpoint();
                Drag::ResizeTextBox(id, edge)
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
        if response.dragged_by(PointerButton::Primary) && !self.middle_pan && self.table_selection_drag.is_none() && self.table_fill_drag.is_none() {
            match self.dragging {
                Some(Drag::CreateShape{kind,start,..})=>{
                    let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(start);
                    self.dragging=Some(Drag::CreateShape{kind,start,current:pointer});
                }
                Some(Drag::CreateTextBox{start,..})=>{
                    let pointer=ctx.input(|i|i.pointer.interact_pos()).unwrap_or(start);
                    self.dragging=Some(Drag::CreateTextBox{start,current:pointer});
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
                    let delta=delta-self.smart_snap_delta;self.smart_snap_delta=Vec2::ZERO;
                    let ids = self.drag_moving.clone();
                    for id in &ids {
                        if let Some(n) = self.doc.node_mut(*id) {
                            n.offset.x += delta.x / self.zoom;
                            n.offset.y += delta.y / self.zoom;
                        }
                    }
                    self.move_canvas_selection(delta);
                    if let Some(id)=ids.first().copied().filter(|_|!modifiers.alt) {
                        let (snap,guides)=self.node_alignment_guides(id,origin);
                        if snap.length_sq()>0.0 {
                            for moved in &ids {if let Some(n)=self.doc.node_mut(*moved){n.offset.x+=snap.x/self.zoom;n.offset.y+=snap.y/self.zoom;}}
                            self.move_canvas_selection(snap);
                            self.smart_snap_delta=snap;
                        }
                        self.snap_guides=guides;
                    }
                }
                Some(Drag::Shape(id)) => {
                    let delta=delta-self.smart_snap_delta;self.smart_snap_delta=Vec2::ZERO;
                    self.move_canvas_selection(delta);
                    for id in self.drag_moving.clone() {
                        if let Some(n) = self.doc.node_mut(id) {
                            n.offset.x += delta.x/self.zoom;
                            n.offset.y += delta.y/self.zoom;
                        }
                    }
                    if !modifiers.alt {
                        self.smart_snap_delta=self.apply_alignment_snap(id,origin);
                        for moved in &self.drag_moving {if let Some(n)=self.doc.node_mut(*moved){n.offset.x+=self.smart_snap_delta.x/self.zoom;n.offset.y+=self.smart_snap_delta.y/self.zoom;}}
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
                    let delta=delta-self.smart_snap_delta;self.smart_snap_delta=Vec2::ZERO;
                    if let Some(box_) = self.doc.text_box_mut(id) {
                        box_.position.x += delta.x / self.zoom;
                        box_.position.y += delta.y / self.zoom;
                    }
                    if !modifiers.alt {
                        let (snap,_)=self.object_alignment_guides(CanvasTarget::TextBox(id),origin);
                        if let Some(box_)=self.doc.text_box_mut(id){box_.position.x+=snap.x;box_.position.y+=snap.y;}
                        self.smart_snap_delta=snap*self.zoom;
                    }
                }
                Some(Drag::ResizeTextBox(id, edge)) => {
                    if let Some(box_) = self.doc.text_box_mut(id) {
                        let dx = delta.x / self.zoom;
                        let dy = delta.y / self.zoom;
                        if edge.left() {
                            let next = (box_.width - dx).clamp(120.0, 2500.0);
                            box_.position.x += box_.width - next;
                            box_.width = next;
                        } else if edge.right() {
                            box_.width = (box_.width + dx).clamp(120.0, 2500.0);
                        }
                        if edge.top() {
                            let next = (box_.height - dy).clamp(50.0, 2500.0);
                            box_.position.y += box_.height - next;
                            box_.height = next;
                        } else if edge.bottom() {
                            box_.height = (box_.height + dy).clamp(50.0, 2500.0);
                        }
                    }
                }
                None => {}
            }
            origin = base + self.pan;
            ctx.request_repaint();
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            if let Some(fill)=self.table_fill_drag.take(){self.apply_table_fill(fill);}
            self.table_selection_drag = None;
            self.table_selection_press = None;
            self.smart_snap_delta=Vec2::ZERO;
            match self.dragging {
                Some(Drag::CreateShape{kind,start,current})=>{
                    if current.distance_sq(start)>16.0 {
                        self.add_dragged_shape(kind,start,current,origin);
                    }
                }
                Some(Drag::CreateTextBox{start,current})=>{
                    if current.distance_sq(start)>16.0 {
                        self.insert_text_box_dragged(start,current,origin);
                    }
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
                                // Preserve the exact source port that was dragged;
                                // never replace it with the geometrically nearest port.
                                let mut created=false;
                                if let Some(existing)=self.doc.connectors.iter().find(|c|c.from==from&&c.to==to).map(|c|c.id) {
                                    // Do not create a second coincident route.
                                    self.selected_connector=Some(existing);
                                } else {
                                    self.selected_connector=self.doc.add_connector(from,to,false);
                                    created=true;
                                }
                                if created { if let Some(id)=self.selected_connector {if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.route=self.insert_connector_route;c.arrow=self.insert_connector_arrow;}} }
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
            }else if let Some(Drag::ResizeShape(id,_))=self.dragging {
                self.alignment_guides_for(id,origin).1
            }else if let Some(Drag::Node(id))=self.dragging {
                self.node_alignment_guides(id,origin).1
            }else if let Some(Drag::TextBox(id))=self.dragging {
                self.object_alignment_guides(CanvasTarget::TextBox(id),origin).1
            }else if let Some(Drag::ResizeTextBox(id,_))=self.dragging {
                self.object_alignment_guides(CanvasTarget::TextBox(id),origin).1
            }else{Vec::new()}
        }else{Vec::new()};
        if self.grid.visible { self.draw_dotted_grid(&painter, bounds, origin); }
        for (a,b) in &self.snap_guides {
            painter.line_segment([*a,*b],Stroke::new(1.0,Color32::from_rgb(58,158,213)));
        }
        if let Some(kind)=self.insert_shape_mode {
            if let Some(Drag::CreateShape{start,current,..})=self.dragging {
                Self::draw_shape_outline(&painter,Rect::from_two_pos(start,current),kind,Stroke::new(1.8,ACCENT));
            }
        }
        if let Some(Drag::CreateLine{start,current})=self.dragging {
            painter.line_segment([start,current],Stroke::new(2.0,ACCENT));
            painter.circle_filled(start,3.5,ACCENT);
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
                let source=from;
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
        // Paint the hierarchy links as the original smooth mindmap curves.
        for node in &self.doc.nodes {
            let Some(parent) = node.parent else { continue; };
            let (Some(&a), Some(&b)) = (positions.get(&parent), positions.get(&node.id)) else { continue; };
            let parent_node = self.doc.node(parent).unwrap();
            let parent_rect = self.screen_rect(parent_node, a, origin);
            let child_rect = self.screen_rect(node, b, origin);
            let start = parent_rect.right_center();
            let end = child_rect.left_center();
            let tint = TINTS[self.tint_index(node.id)];
            let bend = ((end.x - start.x) * 0.48).max(24.0);
            let c1 = start + egui::vec2(bend, 0.0);
            let c2 = end - egui::vec2(bend, 0.0);
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
                if let Some(handles) = self.text_box_resize_handles(box_.id, origin) {
                    for (_, handle) in handles {
                        let hovered = hover.is_some_and(|pointer| handle.contains(pointer));
                        painter.rect_filled(handle, 2.0, if hovered { ACCENT } else { Color32::WHITE });
                        painter.rect_stroke(handle, 2.0, Stroke::new(1.5, if hovered { Color32::WHITE } else { ACCENT }), StrokeKind::Inside);
                    }
                }
            }
            if self.editing_text_box != Some(box_.id) {
                let inset = egui::vec2(15.0, 13.0) * self.zoom;
                let area = Rect::from_min_max(rect.min + inset, rect.max - inset);
                let clipped = painter.with_clip_rect(area);
                let text = if box_.text.is_empty() && selected {
                    "Enter text".to_owned()
                } else { box_.text.clone() };
                let text_color = if box_.fill_color.is_some() { INK }
                    else if self.dark { Color32::WHITE } else { INK };
                let galley = clipped.layout(text, FontId::proportional(15.5 * self.zoom),
                    text_color, area.width().max(20.0));
                let x = match box_.text_horizontal {
                    TextHorizontal::Left => area.left(),
                    TextHorizontal::Center => area.center().x - galley.size().x * 0.5,
                    TextHorizontal::Right => area.right() - galley.size().x,
                };
                let y = match box_.text_vertical {
                    TextVertical::Top => area.top(),
                    TextVertical::Center => area.center().y - galley.size().y * 0.5,
                    TextVertical::Bottom => area.bottom() - galley.size().y,
                };
                clipped.galley(Pos2::new(x, y), galley, text_color);
            }
        }
        // Free shapes are drawn above legacy annotations; hit-testing uses the same priority.
        self.refresh_spreadsheet_cache();
        self.paint_canvas_shapes(&painter, origin, hover);
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
                let alignment = self.doc.text_box(id).map(|b| match (b.text_horizontal, b.text_vertical) {
                    (TextHorizontal::Left, TextVertical::Top) => Align2::LEFT_TOP,
                    (TextHorizontal::Left, TextVertical::Center) => Align2::LEFT_CENTER,
                    (TextHorizontal::Left, TextVertical::Bottom) => Align2::LEFT_BOTTOM,
                    (TextHorizontal::Center, TextVertical::Top) => Align2::CENTER_TOP,
                    (TextHorizontal::Center, TextVertical::Center) => Align2::CENTER_CENTER,
                    (TextHorizontal::Center, TextVertical::Bottom) => Align2::CENTER_BOTTOM,
                    (TextHorizontal::Right, TextVertical::Top) => Align2::RIGHT_TOP,
                    (TextHorizontal::Right, TextVertical::Center) => Align2::RIGHT_CENTER,
                    (TextHorizontal::Right, TextVertical::Bottom) => Align2::RIGHT_BOTTOM,
                }).unwrap_or(Align2::LEFT_TOP);
                // Keep the editor's IME/text origin identical to the committed
                // painter (15px horizontal, 13px vertical inset in world UI).
                let pad_x = (15.0 * self.zoom).round().clamp(8.0, 32.0) as i8;
                let pad_y = (13.0 * self.zoom).round().clamp(6.0, 28.0) as i8;
                let frame_stroke = Stroke::new(1.6, editing_border);
                // TextEdit ignores `.margin()` when a custom frame is set.
                // Put the inset on the frame itself so the IME cursor and the
                // committed canvas text use the same origin and wrap width.
                let frame_margin = egui::Margin::symmetric(pad_x, pad_y)
                    - egui::Margin::from(frame_stroke.width);
                let input = egui::TextEdit::multiline(&mut self.text_box_editor)
                    .id(egui::Id::unique(("annotation-text-edit", id)))
                    .font(FontId::proportional(15.5 * self.zoom))
                    .text_color(if custom_fill { INK } else if self.dark { Color32::WHITE } else { INK })
                    .align(alignment)
                    .desired_width(rect.width())
                    .desired_rows(((rect.height() / (23.0 * self.zoom)).floor() as usize).max(2))
                    .frame(egui::Frame::new()
                        .fill(editing_bg)
                        .stroke(frame_stroke)
                        .inner_margin(frame_margin)
                        .corner_radius(5));
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
                if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Table) {
                    if ui.button(self.language.text("＋▤ Add Row", "＋▤ 行を追加")).clicked(){self.table_add_row(id);ui.close();}
                    if ui.button(self.language.text("＋▥ Add Column", "＋▥ 列を追加")).clicked(){self.table_add_column(id);ui.close();}
                    ui.separator();
                }
                if ui.button(self.language.text("⧉ Copy", "⧉ コピー")).clicked() {self.copy_selected(ui.ctx(),false);ui.close();}
                if ui.button(self.language.text("⎘ Paste", "⎘ 貼り付け")).clicked() {self.paste_from_system_clipboard();ui.close();}
                let is_table=self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Table);
                if !is_table && ui.button(self.language.text("✎ Edit Text", "✎ テキストを編集")).clicked() {self.start_shape_edit(id);ui.close();}
                if !is_table && ui.button(self.language.text("⧉ Duplicate", "⧉ 複製")).clicked(){self.duplicate_canvas_selection();ui.close();}
                if self.doc.shape(id).is_some_and(|s|s.kind!=ShapeKind::Frame) {
                }
                if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Frame) {
                    let count=self.doc.shape(id).map_or(0,|s|s.members.len());
                    ui.label(format!("Items in this frame: {count}"));
                    let selected_others=self.canvas_selection.iter().filter(|&&t|t!=CanvasTarget::Shape(id)).count();
                    if ui.add_enabled(selected_others>0,egui::Button::new(format!("Add {selected_others} Selected Items to Frame"))).clicked(){
                        self.attach_selection_to_frame(id);ui.close();
                    }
                    if ui.button(self.language.text("Add Objects Inside Frame", "フレーム内のオブジェクトを追加")).clicked(){
                        self.attach_contained_to_frame(id);ui.close();
                    }
                    if ui.button(self.language.text("Remove All from Frame", "フレームからすべて削除")).clicked() {
                        self.checkpoint();if let Some(frame)=self.doc.shape_mut(id){frame.members.clear();}
                        self.refresh_frame_grid(id);ui.close();
                    }
                    ui.menu_button("◯ 枠Line Color",|ui|{
                        for (label,color) in [("Blue",[75,124,225]),("Green",[58,159,104]),("赤",[223,85,98]),("Gray",[126,150,198])] {
                            if ui.button(label).clicked(){self.checkpoint();if let Some(frame)=self.doc.shape_mut(id){frame.style.outline=Some(color);}ui.close();}
                        }
                    });
                }
                if !is_table && ui.button(self.language.text("▢ Create Frame from Selection", "▢ 選択範囲からフレームを作成")).clicked(){self.create_frame_for_selection();ui.close();}
                if !is_table {ui.menu_button(self.language.text("☷ Align", "☷ 整列"),|ui|self.arrangement_menu(ui));}
                if ui.button(self.language.text("⌫ Delete", "⌫ 削除")).clicked() {self.select_shape(id,false);self.delete_selected();ui.close();}
                ui.separator();
                if ui.button(self.language.text("▣ Group", "▣ グループ化")).clicked() {self.group_selection();ui.close();}
                if ui.button(self.language.text("▤ Ungroup", "▤ グループ解除")).clicked() {self.ungroup_selection();ui.close();}
                ui.menu_button("◉ 色",|ui| {
                    for (i,label) in COLOR_LABELS.iter().enumerate() {
                        if Self::color_tile(ui,TINTS[i].fill,TINTS[i].border,label) {
                            self.draw_selected_sticky_color(rgb_of(TINTS[i].fill));
                            ui.close();
                        }
                    }
                });
                ui.menu_button(self.language.text("Border", "枠線"),|ui|{
                    if ui.button(self.language.text("No Border", "枠線なし")).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.outline=None;}ui.close();}
                    for (label,color) in [("Blue",[80,126,222]),("Green",[65,164,110]),("Gray",[110,126,155]),("Black",[38,44,60])]{
                        if ui.button(label).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.outline=Some(color);}ui.close();}
                    }
                    ui.separator();
                    for width in [1.0_f32,2.0,3.0,5.0]{
                        if ui.button(format!("{width:.0} px")).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.stroke_width=width;}ui.close();}
                    }
                });
                if !is_table {ui.menu_button(self.language.text("Text alignment", "文字揃え"), |ui| self.text_alignment_menu(ui));}
                if !is_table {ui.menu_button(self.language.text("Text Alignment and Size", "文字の配置とサイズ"),|ui|{
                    ui.label(self.language.text("Horizontal Alignment", "横位置"));
                    for (label,h) in [("Left",TextHorizontal::Left),("Center",TextHorizontal::Center),("Right",TextHorizontal::Right)]{
                        if ui.button(label).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.text_horizontal=h;}ui.close();}
                    }
                    ui.label(self.language.text("Vertical Alignment", "縦位置"));
                    for (label,v) in [("Top",TextVertical::Top),("Center",TextVertical::Center),("Bottom",TextVertical::Bottom)]{
                        if ui.button(label).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.text_vertical=v;}ui.close();}
                    }
                    ui.label(self.language.text("Font Size", "文字サイズ"));
                    for size in [12.0_f32,15.0,18.0,24.0,32.0]{
                        if ui.button(format!("{size:.0} pt")).clicked(){self.checkpoint();if let Some(s)=self.doc.shape_mut(id){s.style.text_size=size;}ui.close();}
                    }
                });}
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
                if ui.button("⇧ 最前面へ").clicked() {self.change_selected_shape_z(2);ui.close();}
                if ui.button("↑ 前面へ").clicked() {self.change_selected_shape_z(1);ui.close();}
                if ui.button("↓ 背面へ").clicked() {self.change_selected_shape_z(-1);ui.close();}
                if ui.button("⇩ 最背面へ").clicked() {self.change_selected_shape_z(-2);ui.close();}
                if ui.button(if self.doc.shape(id).is_some_and(|s|s.locked){"ロック解除"}else{"ロック"}).clicked() {
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
                    for (label,d) in [("最前面へ",2),("一つ前",1),("一つ後ろ",-1),("最背面へ",-2)] {
                        if ui.button(label).clicked(){self.change_selected_line_z(d);ui.close();}
                    }
                });
                ui.menu_button("☷ Align",|ui|self.arrangement_menu(ui));
            } else if let Some(id)=self.context_connector {
                self.connector_context_menu(ui,id,&positions,origin);
            } else if let Some(id) = self.context_text_box {
                if ui.button(self.language.text("✎ Edit Text", "✎ テキストを編集")).clicked() { self.start_text_box_edit(id, false); ui.close(); }
                ui.menu_button(self.language.text("Text alignment", "文字揃え"), |ui| self.text_alignment_menu(ui));
                ui.menu_button(self.language.text("◉ Background Color", "◉ 背景色"), |ui| self.text_box_palette(ui, false));
                ui.menu_button(self.language.text("◯ Border Color", "◯ 枠線色"), |ui| self.text_box_palette(ui, true));
                if ui.button(self.language.text("⌫ Delete", "⌫ 削除")).clicked() { self.select_text_box(id); self.delete_selected(); ui.close(); }
            } else if let Some(id) = self.context_target {
                self.context_color(ui);
                if self.selection.len() > 1 && ui.button(self.language.text("☷ Arrange Selection", "☷ 選択範囲を整列")).clicked() { self.align_selection(); ui.close(); }
                ui.separator();
                let attached:Vec<_>=self.doc.links.iter().copied()
                    .filter(|l|l.from==id || l.to==id).collect();
                if !attached.is_empty() {
                    ui.menu_button("⛓ Link解除",|ui|{
                        if ui.button(self.language.text("Remove All", "すべて削除").to_owned() + &format!(" ({})", attached.len())).clicked() {
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
                if ui.button(self.language.text("↗ Create Link from This Node", "↗ このノードからリンクを作成")).clicked() {
                    self.begin_link(id); ui.close();
                }
                if self.selection.len() == 2 && ui.button(self.language.text("⛓ Link Selected Nodes", "⛓ 選択したノードをリンク")).clicked() {
                    self.link_two_selected(); ui.close();
                }
                ui.separator();
                if ui.button(self.language.text("▣ Group", "▣ グループ化")).clicked() { self.group_selection(); ui.close(); }
                if ui.button(self.language.text("▤ Ungroup", "▤ グループ解除")).clicked() { self.ungroup_selection(); ui.close(); }
                ui.separator();
                if ui.button(self.language.text("⧉ Copy (This Node Only)", "⧉ このノードのみコピー")).clicked() { self.copy_selected(ui.ctx(), false); ui.close(); }
                if ui.button(self.language.text("⧉ Copy (Include Descendants)", "⧉ 子孫を含めてコピー")).clicked() { self.copy_selected(ui.ctx(), true); ui.close(); }
                if ui.button(self.language.text("⎘ Paste", "⎘ 貼り付け")).clicked() { self.paste_from_system_clipboard(); ui.close(); }
                ui.separator();
                if ui.button(self.language.text("＋ Add Child", "＋ 子を追加")).clicked() { self.select(id); self.add_child(); ui.close(); }
                if self.doc.parent(id).is_some() {
                    if ui.button(self.language.text("⊕ Add Sibling", "⊕ 同階層を追加")).clicked() { self.select(id); self.add_sibling(); ui.close(); }
                }
                if ui.button(self.language.text("⌫ Delete", "⌫ 削除")).clicked() { self.delete_selected(); ui.close(); }
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
                if ui.button(self.language.text("⌫ Delete Link", "⌫ リンクを削除")).clicked() {
                    self.selected_link = Some(link);
                    self.delete_selected_link(); ui.close();
                }
            } else {
                if ui.button(self.language.text("▨ Place Sticky Note", "▨ 付箋を配置")).clicked() {self.insert_shape_mode=Some(ShapeKind::StickyNote);ui.close();}
                if ui.button(self.language.text("／ Draw Line", "／ 線を描画")).clicked() {self.insert_shape_mode=None;self.insert_line_mode=true;ui.close();}
                if ui.button(self.language.text("▢ Place Frame", "▢ フレームを配置")).clicked() {self.insert_shape_mode=Some(ShapeKind::Frame);ui.close();}
                if ui.button(self.language.text("▭ Place Rectangle", "▭ 長方形を配置")).clicked() {self.insert_shape_mode=Some(ShapeKind::Rectangle);ui.close();}
                if ui.button(self.language.text("◯ Place Ellipse", "◯ 楕円を配置")).clicked() {self.insert_shape_mode=Some(ShapeKind::Ellipse);ui.close();}
                if ui.button(self.language.text("T Place Text Box", "T テキストBOXを配置")).clicked() { self.insert_shape_mode=None;self.insert_text_box_mode = true; ui.close(); }
            }
        }); }
        if ctx.input(|i|i.pointer.button_released(PointerButton::Secondary)) {
            self.suppress_canvas_context_once=false;
        }
    }
}
