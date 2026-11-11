//! Brainstorming canvas: free lines, frames, inline editing, alignment and duplication.
//! Geometry is stored in world units; all visual hit tolerance is in screen pixels.
use super::*;
use crate::spreadsheet::catalog;

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(super) enum Arrangement {Left, CenterX, Right, Top, CenterY, Bottom, EvenX, EvenY, Grid}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum FrameDropIntent {Attach(u64),Detach}

/// Screen-space line hit-test, shared by handles, hover and selection.
fn line_distance_sq(p:Pos2,a:Pos2,b:Pos2)->f32 {
    let d=b-a;
    let t=if d.length_sq()>0.001 {((p-a).dot(d)/d.length_sq()).clamp(0.0,1.0)} else {0.0};
    p.distance_sq(a+d*t)
}

impl MindmapApp {
    pub(super) fn line_screen_points(&self,id:u64,origin:Pos2)->Option<(Pos2,Pos2)> {
        let l=self.doc.line(id)?;
        Some((origin+egui::vec2(l.start.x,l.start.y)*self.zoom,
              origin+egui::vec2(l.end.x,l.end.y)*self.zoom))
    }
    pub(super) fn line_screen_rect(&self,id:u64,origin:Pos2)->Option<Rect> {
        let (a,b)=self.line_screen_points(id,origin)?;
        Some(Rect::from_two_pos(a,b).expand(7.0))
    }
    pub(super) fn hit_line(&self,p:Pos2,origin:Pos2)->Option<u64> {
        self.doc.lines.iter().filter(|l|l.visible)
            .filter_map(|l| {
                let (a,b)=self.line_screen_points(l.id,origin)?;
                (line_distance_sq(p,a,b)<100.0).then_some((l.z,l.id))
            }).max().map(|(_,id)|id)
    }
    pub(super) fn hit_line_endpoint(&self,p:Pos2,origin:Pos2)->Option<(u64,bool)> {
        self.canvas_selection.iter().filter_map(|t|match t{CanvasTarget::Line(id)=>Some(*id),_=>None})
            .filter_map(|id| {
                if self.doc.line(id).is_some_and(|l|l.locked){return None;}
                let (a,b)=self.line_screen_points(id,origin)?;
                if p.distance_sq(a)<180.0 {Some((id,true))}
                else if p.distance_sq(b)<180.0 {Some((id,false))} else {None}
            }).next()
    }
    pub(super) fn select_line(&mut self,id:u64,additive:bool) {
        if self.doc.line(id).is_none(){return;}
        self.finish_edit(true);self.finish_text_box_edit(true);self.finish_shape_edit(true);
        if !additive {self.canvas_selection.clear();self.selection.clear();}
        let target=CanvasTarget::Line(id);
        if additive && !self.canvas_selection.insert(target) {self.canvas_selection.remove(&target);}
        else {self.canvas_selection.insert(target);}
        if !additive {self.canvas_selection.extend(self.doc.canvas_group_members(target));}
        self.selected_line=Some(id);self.selected_shape=None;self.selected_text_box=None;
        self.selected_connector=None;self.selected_link=None;
    }
    pub(super) fn paint_single_line(&self,painter:&egui::Painter,origin:Pos2,id:u64) {
            let Some(line)=self.doc.line(id) else {return};
            if !line.visible{return;}
            let Some((a,b))=self.line_screen_points(line.id,origin) else {return};
            if !painter.clip_rect().intersects(Rect::from_two_pos(a,b).expand(12.0)){return;}
            let stroke=Stroke::new((line.width*self.zoom).max(0.75),color_from_rgb(line.color));
            if line.dashed {
                let vector=b-a;let length=vector.length();
                if length>1.0 {let dir=vector/length;let mut t=0.0;
                    while t<length {
                        let e=(t+9.0).min(length);
                        painter.line_segment([a+dir*t,a+dir*e],stroke);
                        t+=15.0;
                    }
                }
            } else {painter.line_segment([a,b],stroke);}
            if self.canvas_selection.contains(&CanvasTarget::Line(line.id)) {
                for point in [a,b] {
                    painter.circle_filled(point,5.0,Color32::WHITE);
                    painter.circle_stroke(point,5.0,Stroke::new(1.5,ACCENT));
                }
            }
    }
    pub(super) fn constrained_line_end(start:Pos2,end:Pos2,shift:bool)->Pos2 {
        if !shift{return end;}
        let d=end-start;let distance=d.length();
        if distance<0.01{return end;}
        let angle=(d.y.atan2(d.x)/std::f32::consts::FRAC_PI_4).round()*std::f32::consts::FRAC_PI_4;
        start+egui::vec2(angle.cos()*distance,angle.sin()*distance)
    }
    pub(super) fn add_line_screen(&mut self,start:Pos2,end:Pos2,origin:Pos2) {
        self.finish_shape_edit(true);
        let a=(start-origin)/self.zoom;let b=(end-origin)/self.zoom;
        self.checkpoint();
        match self.doc.add_line(Offset{x:a.x,y:a.y},Offset{x:b.x,y:b.y}) {
            Some(id)=>self.select_line(id,false),
            None=>{self.undo.pop();}
        }
        // Remain in free-line mode for the next stroke until right-click / Esc.
    }
    pub(super) fn draw_shape_outline(painter:&egui::Painter,rect:Rect,kind:ShapeKind,stroke:Stroke){
        Self::paint_shape_kind(painter,kind,rect,Color32::TRANSPARENT,stroke);
    }
    pub(super) fn add_dragged_shape(&mut self,kind:ShapeKind,start:Pos2,end:Pos2,origin:Pos2) {
        // A continuous insertion commits the previous sticky note first.
        self.finish_shape_edit(true);
        let rect=Rect::from_two_pos(start,end);
        let default=if kind==ShapeKind::Text {egui::vec2(260.0,90.0)} else {egui::vec2(180.0,100.0)};
        let (pos,size)=if rect.width()<10.0 || rect.height()<10.0 {(start,default*self.zoom)}
            else {(rect.min,rect.size())};
        self.checkpoint();
        let p=(pos-origin)/self.zoom;
        let id=self.doc.add_shape(kind,Offset{x:p.x,y:p.y});
        if let Some(shape)=self.doc.shape_mut(id) {
            shape.width=(size.x/self.zoom).clamp(32.0,50_000.0);
            shape.height=(size.y/self.zoom).clamp(24.0,50_000.0);
        }
        if kind==ShapeKind::StickyNote {
            if let Some(frame_id)=self.frame_at_drop(end,origin) {
                self.apply_grid_insert(frame_id,id,end,origin);
            }
        }
        self.select_shape(id,false);
        self.insert_shape_mode=Some(kind);
        if kind==ShapeKind::Text || kind==ShapeKind::StickyNote {self.start_shape_edit(id);}
    }
    pub(super) fn inline_shape_editor(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,origin:Pos2,just_committed_ime:bool) {
        let Some(id)=self.editing_shape else {return};
        let Some(shape)=self.doc.shape(id) else {self.editing_shape=None;return};
        let Some(bounds)=self.shape_screen_rect(id,origin) else {return};
        let kind=shape.kind;
        if kind==ShapeKind::Table {
            let Some((row,col))=self.editing_table_cell else{return};
            let Some(table)=shape.table.as_ref() else{return};
            let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
            if rows==0||cols==0{return;}
            let merge=table.merge_at(row,col).unwrap_or(TableMerge{row,column:col,rows:1,columns:1});
            let base_cell=Rect::from_min_max(
                egui::pos2(bounds.left()+bounds.width()*merge.column as f32/cols as f32,bounds.top()+self.zoom*table.row_offset(merge.row)),
                egui::pos2(bounds.left()+bounds.width()*(col+1) as f32/cols as f32,bounds.top()+bounds.height()*(row+1) as f32));
            let cell=Rect::from_min_size(base_cell.min,egui::vec2(
                bounds.width()*merge.columns as f32/cols as f32,self.zoom*(merge.row..merge.row+merge.rows).map(|i|table.row_height(i)).sum::<f32>()));
            let parent_clip=ui.clip_rect();
            let mut cell_ui=ui.new_child(egui::UiBuilder::new().max_rect(cell)
                .layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)));
            cell_ui.set_clip_rect(cell.intersect(parent_clip));
            cell_ui.spacing_mut().interact_size.y=cell.height();
            cell_ui.spacing_mut().item_spacing=Vec2::ZERO;
            cell_ui.painter().rect_filled(cell,0.0,Color32::WHITE);
            cell_ui.painter().rect_stroke(cell,0.0,Stroke::new(1.5,ACCENT),StrokeKind::Inside);
            let horizontal=table.cell_alignments.as_ref().and_then(|a|a.get(row)).and_then(|a|a.get(col))
                .copied().unwrap_or(TextHorizontal::Center);
            let cell_text_color=table.cell_text_colors.as_ref().and_then(|colors|colors.get(row)).and_then(|r|r.get(col)).copied().flatten().unwrap_or(shape.style.text);
            let click_to_position=!self.shape_edit_focus_pending&&ctx.input(|i|i.pointer.button_pressed(PointerButton::Primary)
                &&i.pointer.interact_pos().is_some_and(|p|cell.contains(p)));
            if click_to_position {self.table_hidden_overwrite_selection=false;self.table_cell_input_mode=false;self.formula_reference_mode=false;}
            if self.table_hidden_overwrite_selection {
                // Retain the full text selection for overwrite, but render it
                // with the ordinary text and cell colors.
                let visuals=cell_ui.visuals_mut();
                visuals.selection.bg_fill=Color32::TRANSPARENT;
                visuals.selection.stroke.color=color_from_rgb(cell_text_color);
            }
            let edit_align=match horizontal {TextHorizontal::Left=>Align2::LEFT_CENTER,TextHorizontal::Center=>Align2::CENTER_CENTER,TextHorizontal::Right=>Align2::RIGHT_CENTER};
            let edit_id=egui::Id::new(("inline-table-cell",id,row,col));
            let editor_focused=ctx.memory(|memory|memory.has_focus(edit_id));
            let cursor_char=egui::TextEdit::load_state(ctx,edit_id)
                .and_then(|state|state.cursor.char_range()).map(|range|range.primary.index.0)
                .unwrap_or_else(||self.shape_edit_text.chars().count());
            let cursor_byte=self.shape_edit_text.char_indices().nth(cursor_char)
                .map_or(self.shape_edit_text.len(),|(byte,_)|byte);
            let completion=if editor_focused&&self.shape_edit_text.starts_with('=') {catalog::function_prefix_at(&self.shape_edit_text,cursor_byte)
                .map(|prefix|(cursor_byte-prefix.len(),cursor_byte,prefix.to_owned()))}else{None};
            let candidates=completion.as_ref().map_or_else(Vec::new,|(_,_,prefix)|catalog::candidates(prefix));
            let candidates_hidden=completion.as_ref().is_some_and(|(_,_,prefix)|self.formula_candidates_hidden_prefix.as_ref()==Some(prefix));
            let candidates_visible=!candidates.is_empty()&&!candidates_hidden;
            if !candidates_visible {self.formula_candidate_index=0;}
            else {self.formula_candidate_index=self.formula_candidate_index.min(candidates.len()-1);}
            let mut candidate_key_consumed=false;
            // A direct replacement entry is committed by an arrow key, then
            // the table selection moves in that direction. In-place editing
            // keeps ordinary caret behavior unless the formula point mode is active.
            if editor_focused && self.table_cell_input_mode && !self.shape_edit_text.starts_with('=')
                && !self.ime_composing && !just_committed_ime {
                let direction=ctx.input(|i| {
                    if i.modifiers!=egui::Modifiers::NONE {
                        if i.modifiers.shift&&i.key_pressed(egui::Key::Enter){return Some(( -1isize,0isize,egui::Key::Enter,egui::Modifiers::SHIFT));}
                        return None;
                    }
                    if i.key_pressed(egui::Key::ArrowLeft){Some((0isize,-1isize,egui::Key::ArrowLeft,egui::Modifiers::NONE))}
                    else if i.key_pressed(egui::Key::ArrowRight){Some((0,1,egui::Key::ArrowRight,egui::Modifiers::NONE))}
                    else if i.key_pressed(egui::Key::ArrowUp){Some((-1,0,egui::Key::ArrowUp,egui::Modifiers::NONE))}
                    else if i.key_pressed(egui::Key::ArrowDown){Some((1,0,egui::Key::ArrowDown,egui::Modifiers::NONE))}else{None}
                });
                if let Some((dr,dc,key,mods))=direction {
                    let consumed=ctx.input_mut(|i|i.consume_key(mods,key));
                    if consumed {
                        self.finish_shape_edit(true);
                        self.move_table_cell_selection(id,row,col,dr,dc,false);
                        ctx.request_repaint();return;
                    }
                }
            }
            // In reference mode, arrow keys choose a cell reference
            // instead of moving the text caret. Repeated arrows replace the
            // previous reference token so the expression remains editable.
            if editor_focused && self.shape_edit_text.starts_with('=')
                && !self.ime_composing && !just_committed_ime {
                if ctx.input(|i|i.key_pressed(egui::Key::F2)&&i.modifiers==egui::Modifiers::NONE) {
                    ctx.input_mut(|i|{i.consume_key(egui::Modifiers::NONE,egui::Key::F2);});
                    self.formula_reference_mode=!self.formula_reference_mode;
                    candidate_key_consumed=true;
                } else if self.formula_reference_mode {
                    let direction=ctx.input(|i| {
                        let shift=i.modifiers.shift;
                        if i.modifiers.command||i.modifiers.ctrl||i.modifiers.alt{return None;}
                        if !shift&&candidates_visible&&(i.key_pressed(egui::Key::ArrowUp)||i.key_pressed(egui::Key::ArrowDown)){return None;}
                        let direction=if i.key_pressed(egui::Key::ArrowLeft){Some((0isize,-1isize,egui::Key::ArrowLeft))}
                        else if i.key_pressed(egui::Key::ArrowRight){Some((0,1,egui::Key::ArrowRight))}
                        else if i.key_pressed(egui::Key::ArrowUp){Some((-1,0,egui::Key::ArrowUp))}
                        else if i.key_pressed(egui::Key::ArrowDown){Some((1,0,egui::Key::ArrowDown))}else{None};
                        direction.map(|(dr,dc,key)|(dr,dc,key,shift))
                    });
                    if let Some((dr,dc,key,extend_range))=direction {
                        let modifiers=if extend_range{egui::Modifiers::SHIFT}else{egui::Modifiers::NONE};
                        let consumed=ctx.input_mut(|i|i.consume_key(modifiers,key));
                        if consumed {
                            let old_target=self.formula_reference_target.unwrap_or((id,row,col));
                            let (target_id,base_row,base_col)=old_target;
                            let (target_rows,target_cols)=self.doc.shape(target_id).and_then(|s|s.table.as_ref())
                                .map(|t|(t.cells.len(),t.cells.first().map_or(0,Vec::len))).unwrap_or((rows,cols));
                            let nr=(base_row as isize+dr).clamp(0,target_rows.saturating_sub(1) as isize) as usize;
                            let nc=(base_col as isize+dc).clamp(0,target_cols.saturating_sub(1) as isize) as usize;
                            let anchor=if extend_range {
                                self.formula_reference_anchor.unwrap_or(if self.formula_reference_target.is_some(){old_target}else{(id,row,col)})
                            } else {(target_id,nr,nc)};
                            self.formula_reference_anchor=Some(anchor);
                            let active=(target_id,nr,nc);
                            let column_name=|column:usize|{let mut text=String::new();let mut n=column+1;while n>0{let rem=(n-1)%26;text.insert(0,char::from(b'A'+rem as u8));n=(n-1)/26;}text};
                            let (token_start,token_end)=if extend_range {
                                let (anchor_id,ar,ac)=anchor;
                                let endpoint=if anchor_id==target_id{format!("{}{}",column_name(nc),nr+1)}else{format!("{}:{}{}",target_id,column_name(nc),nr+1)};
                                let startpoint=if anchor_id==id{format!("{}{}",column_name(ac),ar+1)}else{format!("{}:{}{}",anchor_id,column_name(ac),ar+1)};
                                (startpoint,endpoint)
                            }else{
                                let single=if target_id==id{format!("{}{}",column_name(nc),nr+1)}else{format!("{}:{}{}",target_id,column_name(nc),nr+1)};
                                (single.clone(),single)
                            };
                            let token=if token_start==token_end{token_start}else{format!("{}:{}",token_start,token_end)};
                            let (start,end)=self.formula_reference_range.filter(|(_,end)|*end==cursor_byte)
                                .unwrap_or((cursor_byte,cursor_byte));
                            if start<=end&&end<=self.shape_edit_text.len() {
                                self.shape_edit_text.replace_range(start..end,&token);
                                let next_end=start+token.len();self.formula_reference_range=Some((start,next_end));
                                let cursor_index=self.shape_edit_text[..next_end].chars().count();
                                let mut state=egui::TextEdit::load_state(ctx,edit_id).unwrap_or_default();
                                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor_index))));
                                state.store(ctx,edit_id);
                                self.shape_edit_focus_pending=true;
                            }
                            self.formula_reference_target=Some(active);
                            candidate_key_consumed=true;
                            ctx.request_repaint();
                        }
                    }
                }
            }
            if editor_focused&&!self.ime_composing&&!just_committed_ime {
                if candidates_visible {
                    let key=ctx.input(|i|{
                        if i.key_pressed(egui::Key::ArrowUp){Some(egui::Key::ArrowUp)}
                        else if i.key_pressed(egui::Key::ArrowDown){Some(egui::Key::ArrowDown)}
                        else if i.key_pressed(egui::Key::Escape){Some(egui::Key::Escape)}
                        else if i.key_pressed(egui::Key::Tab)&&!i.modifiers.shift{Some(egui::Key::Tab)}
                        else{None}
                    });
                    if let Some(key)=key {
                        match key {
                            egui::Key::ArrowUp=>self.formula_candidate_index=self.formula_candidate_index.saturating_sub(1),
                            egui::Key::ArrowDown=>self.formula_candidate_index=(self.formula_candidate_index+1).min(candidates.len()-1),
                            egui::Key::Escape=>self.formula_candidates_hidden_prefix=completion.as_ref().map(|(_,_,prefix)|prefix.clone()),
                            egui::Key::Tab=>if let Some((start,end,_))=completion.as_ref(){
                                let insertion=format!("{}(",candidates[self.formula_candidate_index].insert_name);
                                self.shape_edit_text.replace_range(*start..*end,&insertion);
                                let cursor_index=self.shape_edit_text[..*start].chars().count()+insertion.chars().count();
                                let mut state=egui::TextEdit::load_state(ctx,edit_id).unwrap_or_default();
                                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor_index))));
                                state.store(ctx,edit_id);
                                self.formula_candidate_index=0;
                                self.formula_candidates_hidden_prefix=None;
                            },
                            _=>{}
                        }
                        ctx.input_mut(|i|{i.consume_key(egui::Modifiers::NONE,key);});
                        candidate_key_consumed=true;
                    }
                }
            }
            if editor_focused&&!self.ime_composing&&!just_committed_ime&&!candidate_key_consumed {
                let (movement,modifiers,delete_cell)=ctx.input(|i|{
                    let movement=if i.key_pressed(egui::Key::Tab){Some((0,if i.modifiers.shift{-1}else{1},false,egui::Key::Tab))}
                    else if i.key_pressed(egui::Key::Enter)&&!i.modifiers.shift&&!i.modifiers.command{Some((1,0,false,egui::Key::Enter))}
                    else{None};(movement,i.modifiers,i.key_pressed(egui::Key::Delete)&&!i.modifiers.command&&!i.modifiers.alt)
                });
                if delete_cell {
                    ctx.input_mut(|i|{i.consume_key(modifiers,egui::Key::Delete);});
                    self.finish_shape_edit(true);
                    let has_contents=self.doc.shape(id).and_then(|s|s.table.as_ref()).and_then(|t|t.cells.get(row)).and_then(|r|r.get(col)).is_some_and(|text|!text.is_empty());
                    if has_contents {
                        self.checkpoint();
                        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()){
                            if let Some(value)=table.cells.get_mut(row).and_then(|r|r.get_mut(col)){value.clear();}
                            table.formula_cells.retain(|coord|*coord!=(row,col));
                        }
                    }
                    self.selected_table_cell=Some((id,row,col));
                    self.selected_table_range=Some((id,row,col,row,col));
                    ctx.request_repaint();
                    return;
                }
                if let Some((dr,dc,extend,key))=movement {
                    ctx.input_mut(|i|{i.consume_key(modifiers,key);});
                    self.finish_shape_edit(true);
                    self.move_table_cell_selection(id,row,col,dr,dc,extend);
                    ctx.request_repaint();
                    return;
                }
            }
            let edit=egui::TextEdit::multiline(&mut self.shape_edit_text).id(edit_id)
                .font(FontId::proportional(shape.style.text_size*self.zoom)).text_color(color_from_rgb(cell_text_color))
                .align(edit_align).desired_width(cell.width()).desired_rows(1)
                .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT,egui::Key::Enter)))
                .event_filter(egui::EventFilter{tab:true,horizontal_arrows:true,vertical_arrows:true,..Default::default()})
                .margin(egui::Margin::ZERO).frame(egui::Frame::NONE);
            let response=cell_ui.add_sized(cell.size(),edit);
            if let Some(reference_rect)=self.formula_reference_rect(id,origin) {
                let color=Color32::from_rgb(39,128,228);
                ui.painter().rect_stroke(reference_rect.shrink(1.5),0.0,Stroke::new(2.3,color),StrokeKind::Inside);
                let handle=Rect::from_center_size(reference_rect.right_bottom(),egui::vec2(5.0,5.0));
                ui.painter().rect_filled(handle,0.0,color);
            }
            if response.changed(){
                if self.table_hidden_overwrite_selection {self.table_cell_input_mode=true;}
                self.table_hidden_overwrite_selection=false;self.formula_candidates_hidden_prefix=None;self.formula_reference_range=None;self.formula_reference_anchor=None;
                if self.shape_edit_text=="=" {self.formula_reference_mode=true;}
                else if !self.shape_edit_text.starts_with('=') {self.formula_reference_mode=false;self.formula_reference_target=None;}
            }
            let mut suggestion_clicked=false;
            if candidates_visible {
                let mut clicked_candidate=None;
                egui::Area::new(egui::Id::unique(("table-formula-suggestions",id)))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(cell.left(),cell.bottom()+3.0))
                    .show(ctx,|popup|egui::Frame::popup(popup.style()).show(popup,|popup|{
                        popup.set_min_width(260.0);
                        popup.set_max_width(340.0);
                        egui::ScrollArea::vertical().max_height(160.0).show(popup,|list|{
                            for (index,candidate) in candidates.iter().enumerate() {
                                let label=format!("{}    {}",candidate.insert_name,candidate.function.japanese_description);
                                if list.selectable_label(index==self.formula_candidate_index,label).clicked(){clicked_candidate=Some(index);}
                            }
                        });
                        if let Some(candidate)=candidates.get(self.formula_candidate_index) {
                            popup.separator();
                            popup.strong(candidate.function.signature);
                            if let Some(argument)=candidate.function.arguments.first(){popup.label(format!("{}: {}",argument.name,argument.description));}
                            popup.weak(candidate.function.example);
                        }
                    }));
                if let Some(index)=clicked_candidate{self.formula_candidate_index=index;}
                suggestion_clicked=clicked_candidate.is_some();
                if suggestion_clicked {response.request_focus();ctx.request_repaint();}
                if let Some((start,end,_))=completion.as_ref(){
                    if ctx.input(|i|i.key_pressed(egui::Key::Escape))&&!self.ime_composing&&!just_committed_ime {
                        ctx.input_mut(|i|{i.consume_key(egui::Modifiers::NONE,egui::Key::Escape);});
                        self.formula_candidates_hidden_prefix=completion.as_ref().map(|(_,_,prefix)|prefix.clone());
                    }
                    let _=(start,end);
                }
            }
            if self.shape_edit_focus_pending {
                response.request_focus();
                if self.table_overwrite_pending {
                    let mut state=egui::TextEdit::load_state(ctx,edit_id).unwrap_or_default();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::select_all_str(&self.shape_edit_text)));
                    state.store(ctx,edit_id);
                    self.table_overwrite_pending=false;
                }
                self.shape_edit_focus_pending=false;
            }
            if ctx.input(|i|i.modifiers.shift&&i.key_pressed(egui::Key::Enter))&&!self.ime_composing&&!just_committed_ime {self.expand_table_row_for_newline(id,row);return;}
            let esc=ctx.input(|i|i.key_pressed(egui::Key::Escape));
            let commit=ctx.input(|i|i.modifiers.command&&i.key_pressed(egui::Key::Enter));
            if esc&&!self.ime_composing&&!just_committed_ime {self.finish_shape_edit(false);}
            else if (commit||(response.lost_focus()&&!suggestion_clicked))&&!self.ime_composing&&!just_committed_ime {self.finish_shape_edit(true);}
            return;
        }
        let fill=shape.style.fill.map(color_from_rgb).unwrap_or(if self.dark {Color32::from_rgb(34,40,54)}else{Color32::WHITE});
        let fg=color_from_rgb(shape.style.text);
        let area=if kind==ShapeKind::Frame {self.frame_header_screen_rect(id,origin).unwrap_or(bounds)}
                 else {bounds};
        // A sticky's edited and committed text must have identical origin,
        // wrap width and font. The sticker already paints its own background;
        // draw the TextEdit WITHOUT padding/frame on the *content* rect.
        let sticky=kind==ShapeKind::StickyNote;
        let custom_align=match (shape.style.text_horizontal,shape.style.text_vertical) {
            (TextHorizontal::Left,TextVertical::Top)=>Align2::LEFT_TOP,
            (TextHorizontal::Left,TextVertical::Center)=>Align2::LEFT_CENTER,
            (TextHorizontal::Left,TextVertical::Bottom)=>Align2::LEFT_BOTTOM,
            (TextHorizontal::Center,TextVertical::Top)=>Align2::CENTER_TOP,
            (TextHorizontal::Center,TextVertical::Center)=>Align2::CENTER_CENTER,
            (TextHorizontal::Center,TextVertical::Bottom)=>Align2::CENTER_BOTTOM,
            (TextHorizontal::Right,TextVertical::Top)=>Align2::RIGHT_TOP,
            (TextHorizontal::Right,TextVertical::Center)=>Align2::RIGHT_CENTER,
            (TextHorizontal::Right,TextVertical::Bottom)=>Align2::RIGHT_BOTTOM,
        };
        let content=if sticky {area.shrink(12.0*self.zoom)} else {area};
        let margin=if sticky {0} else if kind==ShapeKind::Frame {(12.0*self.zoom).round().clamp(5.0,30.0) as i8}
            else {(12.0*self.zoom).round().clamp(5.0,30.0) as i8};
        let edit=egui::TextEdit::multiline(&mut self.shape_edit_text)
            .id_salt(("inline-canvas-shape",id))
            .font(FontId::proportional(if sticky {16.0*self.zoom}else{shape.style.text_size*self.zoom}))
            .text_color(fg)
            .align(if kind==ShapeKind::Frame {Align2::LEFT_CENTER}else{custom_align})
            .margin(if kind==ShapeKind::Frame {egui::Margin::symmetric(margin,0)} else {egui::Margin::symmetric(margin,margin)})
            .desired_width((content.width()-2.0*f32::from(margin)).max(20.0))
            .desired_rows(((content.height()/(20.0*self.zoom)).floor() as usize).max(1))
            .frame(egui::Frame::new().fill(if sticky {Color32::TRANSPARENT}else{fill})
                .stroke(if sticky {Stroke::NONE}else{Stroke::new(1.5,ACCENT)}).corner_radius(4));
        let area=content;
        if sticky {ui.painter().rect_stroke(bounds,4.0,Stroke::new(1.5,ACCENT),StrokeKind::Inside);}
        let response=ui.place(area,edit);
        if self.shape_edit_focus_pending {response.request_focus();self.shape_edit_focus_pending=false;}
        let esc=ctx.input(|i|i.key_pressed(egui::Key::Escape));
        let commit=ctx.input(|i|i.modifiers.command&&i.key_pressed(egui::Key::Enter));
        if esc && !self.ime_composing && !just_committed_ime {self.finish_shape_edit(false);}
        else if (commit || response.lost_focus()) && !self.ime_composing && !just_committed_ime {self.finish_shape_edit(true);}
    }
    /// The title strip remains a hit target for selecting the Frame itself.
    /// Membership drops, however, use the *entire* Frame rectangle.
    pub(super) fn frame_header_screen_rect(&self,id:u64,origin:Pos2)->Option<Rect> {
        let frame=self.doc.shape(id)?;
        if frame.kind!=ShapeKind::Frame || !frame.visible {return None;}
        let rect=self.shape_screen_rect(id,origin)?;
        Some(Rect::from_min_size(rect.min,egui::vec2(rect.width(),
            (32.0*self.zoom).clamp(24.0,48.0).min(rect.height()))))
    }
    /// Locate the drop target using the whole Frame and a small screen-space
    /// tolerance. The expansion itself is based on the *note geometry*, not
    /// where the pointer happened to grab the note.
    pub(super) fn frame_at_drop(&self,point:Pos2,origin:Pos2)->Option<u64> {
        self.doc.shapes.iter().filter(|s|s.kind==ShapeKind::Frame && s.visible && !s.locked)
            .filter(|s|self.shape_screen_rect(s.id,origin).is_some_and(|r|r.expand(22.0).contains(point)))
            .max_by_key(|s|(s.z,s.id)).map(|s|s.id)
    }
    /// For a new member, allow a one-cell extension zone to the right/bottom
    /// so a user can drop *beside* a small Frame. For an existing member,
    /// crossing its current boundary means detach, not accidental growth.
    pub(super) fn frame_drop_target_for_note(&self,note_id:u64,point:Pos2,origin:Pos2)->Option<u64> {
        let owner=self.sticky_frame(note_id);
        self.doc.shapes.iter().filter(|s|s.kind==ShapeKind::Frame && s.visible && !s.locked)
            .filter(|frame| {
                let Some(r)=self.shape_screen_rect(frame.id,origin) else{return false;};
                if owner==Some(frame.id) {return r.contains(point);}
                let (cell_w,cell_h)=frame.grid.as_ref()
                    .map(|g|(g.cell_width,g.cell_height)).unwrap_or((240.0,160.0));
                let right=Rect::from_min_max(r.right_top()-egui::vec2(0.0,12.0),
                    r.right_bottom()+egui::vec2(cell_w*self.zoom*0.8,12.0));
                let bottom=Rect::from_min_max(r.left_bottom()-egui::vec2(12.0,0.0),
                    r.right_bottom()+egui::vec2(12.0,cell_h*self.zoom*0.8));
                r.expand(18.0).contains(point) || right.contains(point) || bottom.contains(point)
            }).max_by_key(|s|(s.z,s.id)).map(|s|s.id)
    }
    /// Determine whether this drag adds a sticky to a new Frame or removes it.
    /// A drag within the same Frame is a grid reorder, handled separately.
    #[allow(dead_code)]
    fn frame_drop_intent(&self,id:u64,point:Pos2,origin:Pos2)->Option<FrameDropIntent> {
        if !self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::StickyNote && !s.locked){return None;}
        let inside=self.frame_drop_target_for_note(id,point,origin);
        let current=self.sticky_frame(id);
        match (inside,current) {
            (Some(dest),previous) if previous!=Some(dest)=>Some(FrameDropIntent::Attach(dest)),
            (None,Some(_))=>Some(FrameDropIntent::Detach),
            _=>None,
        }
    }
    /// Used by the release handler; the grid plan and ghost preview share
    /// exactly the same placement calculation. Frame geometry is recalculated
    /// after attachment and after detachment (including shrink-to-fit).
    pub(super) fn update_frame_membership_on_drop(&mut self,id:u64,point:Pos2,origin:Pos2) {
        if !self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::StickyNote && !s.locked){return;}
        if let Some(frame_id)=self.frame_drop_target_for_note(id,point,origin) {
            self.apply_grid_insert(frame_id,id,point,origin);
        }else if self.sticky_frame(id).is_some() {
            self.detach_grid_sticky(id);
        }
    }
    /// Static dashes: no moving animation or repaint timer. Every edge uses
    /// a fixed screen-space dash/gap so the preview is calm and predictable.
    fn paint_marching_rect(painter:&egui::Painter,rect:Rect,color:Color32,_phase:f32,width:f32) {
        let points=[rect.left_top(),rect.right_top(),rect.right_bottom(),rect.left_bottom()];
        for side in 0..4 {
            let a=points[side];let b=points[(side+1)%4];
            let v=b-a;let len=v.length();if len<1.0 {continue;}
            let direction=v/len;
            let mut pos=0.0;
            while pos<len {
                let start=pos.max(0.0);
                let stop=(pos+10.0).min(len);
                if stop>start {
                    painter.line_segment([a+direction*start,a+direction*stop],Stroke::new(width,color));
                }
                pos+=18.0;
            }
        }
    }
    /// Predictive drop targets. Blue shows the *actual future grid cell* and
    /// enlarged Frame; red shows the detached note and the smaller Frame.
    /// The dashes are deliberately static (no idle repaints or animation).
    pub(super) fn paint_frame_drop_feedback(&self,painter:&egui::Painter,origin:Pos2,point:Option<Pos2>) {
        let (Some(Drag::Shape(id)),Some(pointer))=(self.dragging,point) else{return};
        let Some(note)=self.doc.shape(id).filter(|s|s.kind==ShapeKind::StickyNote) else{return};
        let red=Color32::from_rgb(222,92,110);
        let blue=Color32::from_rgb(67,139,228);
        let (ghost,frame_outline,color,caption)=if let Some(frame_id)=self.frame_drop_target_for_note(id,pointer,origin) {
            let Some(plan)=self.preview_grid_insert(frame_id,id,pointer,origin) else{return};
            let Some(frame)=self.doc.shape(plan.frame) else{return};
            let (x,y,w,h)=plan.grid.cell_rect(plan.slot);
            let (fw,fh)=plan.grid.dimensions();
            let cell=Rect::from_min_size(origin+egui::vec2(frame.position.x+x,frame.position.y+y)*self.zoom,
                egui::vec2(w,h)*self.zoom);
            let bounds=Rect::from_min_size(origin+egui::vec2(frame.position.x,frame.position.y)*self.zoom,
                egui::vec2(fw,fh)*self.zoom);
            (cell,Some(bounds),blue,if plan.attaching {"Add Here"}else{"Move Here"})
        } else if let Some(frame_id)=self.sticky_frame(id) {
            let Some(frame)=self.doc.shape(frame_id) else{return};
            let Some(note_rect)=self.shape_screen_rect(id,origin) else{return};
            let (width,height)=self.frame_grid_shrink_preview(frame_id,id).unwrap_or((frame.width,frame.height));
            let bounds=Rect::from_min_size(origin+egui::vec2(frame.position.x,frame.position.y)*self.zoom,
                egui::vec2(width,height)*self.zoom);
            (note_rect,Some(bounds),red,"Remove from Frame")
        }else{return;};
        painter.rect_filled(ghost,4.0,Color32::from_rgba_unmultiplied(color.r(),color.g(),color.b(),28));
        Self::paint_marching_rect(painter,ghost.expand(3.0),color,0.0,2.4);
        if let Some(frame_rect)=frame_outline {
            painter.rect_filled(frame_rect,4.0,Color32::from_rgba_unmultiplied(color.r(),color.g(),color.b(),14));
            Self::paint_marching_rect(painter,frame_rect.expand(2.0),color,0.0,2.0);
        }
        // Ghost dimensions are the exact grid-constrained size (possibly 1x2,
        // 2x1, 2x2), rather than the free-size note while it is being dragged.
        let text_position=ghost.left_top()+egui::vec2(9.0,-9.0);
        let galley=painter.layout(caption.to_owned(),FontId::proportional(12.0),Color32::WHITE,180.0);
        let label=Rect::from_min_size(text_position-egui::vec2(7.0,galley.size().y+4.0),
            galley.size()+egui::vec2(14.0,8.0));
        painter.rect_filled(label,4.0,color);
        painter.galley(label.left_top()+egui::vec2(7.0,4.0),galley,Color32::WHITE);
        let _=note; // note existence was validated above
    }
    pub(super) fn paint_frame_backgrounds(&self,painter:&egui::Painter,origin:Pos2) {
        for frame in self.doc.shapes.iter().filter(|s|s.visible&&s.kind==ShapeKind::Frame) {
            let Some(r)=self.shape_screen_rect(frame.id,origin) else {continue};
            if !r.intersects(painter.clip_rect()){continue;}
            let rgb=frame.style.fill.unwrap_or([240,244,255]);
            painter.rect_filled(r,5.0,Color32::from_rgba_unmultiplied(rgb[0],rgb[1],rgb[2],70));
        }
    }
    pub(super) fn paint_frame_headers(&self,painter:&egui::Painter,origin:Pos2) {
        for frame in self.doc.shapes.iter().filter(|s|s.visible&&s.kind==ShapeKind::Frame) {
            let Some(r)=self.shape_screen_rect(frame.id,origin) else {continue};
            if !r.intersects(painter.clip_rect()){continue;}
            let outline=frame.style.outline.map(color_from_rgb).unwrap_or(Color32::from_gray(153));
            painter.rect_stroke(r,5.0,Stroke::new((frame.style.stroke_width*self.zoom).max(1.0),outline),StrokeKind::Inside);
            if self.editing_shape!=Some(frame.id) {
                painter.text(r.left_top()+egui::vec2(12.0,15.0),Align2::LEFT_CENTER,&frame.text,
                    FontId::proportional(15.0*self.zoom),if self.dark{Color32::WHITE}else{INK});
            }
            if self.canvas_selection.contains(&CanvasTarget::Shape(frame.id)) {
                painter.rect_stroke(r.expand(3.0),5.0,Stroke::new(1.4,ACCENT),StrokeKind::Inside);
                if let Some(handles)=self.resize_handles(frame.id,origin){
                    for (_,hit) in handles {
                        let handle=Rect::from_center_size(hit.center(),egui::vec2(9.0,9.0));
                        painter.rect_filled(handle,2.0,Color32::WHITE);
                        painter.rect_stroke(handle,2.0,Stroke::new(1.4,ACCENT),StrokeKind::Inside);
                    }
                }
            }
        }
    }
    /// Compact alignment picker: actions are grouped by what they do, with
    /// a vector pictogram instead of ambiguous long text-only menu items.
    fn arrangement_icon(ui:&mut egui::Ui,label:&str,mode:Arrangement)->bool {
        let button=ui.add_sized([94.0,32.0],egui::Button::new(format!("    {label}")));
        let r=Rect::from_min_size(button.rect.left_center()+egui::vec2(6.0,-9.0),egui::vec2(16.0,18.0));
        let ink=ui.visuals().text_color();
        let stroke=Stroke::new(1.5,ink);
        let x0=r.left();let x1=r.right();let cy=r.center().y;
        let p=ui.painter();
        match mode {
            Arrangement::Left|Arrangement::CenterX|Arrangement::Right=>{
                let anchor=match mode {Arrangement::Left=>x0,Arrangement::CenterX=>(x0+x1)*0.5,_=>x1};
                p.line_segment([Pos2::new(anchor,r.top()),Pos2::new(anchor,r.bottom())],stroke);
                for (offset,y) in [(5.0,r.top()+4.0),(10.0,r.top()+11.0)] {
                    let start=match mode{Arrangement::Left=>anchor,Arrangement::CenterX=>anchor-offset/2.0,_=>anchor-offset};
                    p.line_segment([Pos2::new(start,y),Pos2::new(start+offset,y)],Stroke::new(3.0,ACCENT));
                }
            }
            Arrangement::Top|Arrangement::CenterY|Arrangement::Bottom=>{
                let anchor=match mode{Arrangement::Top=>r.top(),Arrangement::CenterY=>cy,_=>r.bottom()};
                p.line_segment([Pos2::new(x0,anchor),Pos2::new(x1,anchor)],stroke);
                for (width,x) in [(5.0,x0+4.0),(10.0,x0+11.0)] {
                    let top=match mode{Arrangement::Top=>anchor,Arrangement::CenterY=>anchor-width/2.0,_=>anchor-width};
                    p.line_segment([Pos2::new(x,top),Pos2::new(x,top+width)],Stroke::new(3.0,ACCENT));
                }
            }
            Arrangement::EvenX|Arrangement::EvenY|Arrangement::Grid=>{
                for i in 0..3 {
                    let (x,y)=if matches!(mode,Arrangement::EvenY){(x0+4.0,r.top()+2.0+i as f32*6.0)}
                        else {(x0+2.0+i as f32*6.0,r.top()+4.0)};
                    p.rect_filled(Rect::from_min_size(Pos2::new(x,y),egui::vec2(3.0,6.0)),1.0,ACCENT);
                }
            }
        }
        let explanation=match mode {
            Arrangement::Left=>"Align left edges and arrange vertically without overlap",
            Arrangement::CenterX=>"Align horizontal centers and arrange vertically without overlap",
            Arrangement::Right=>"Align right edges and arrange vertically without overlap",
            Arrangement::Top=>"Align top edges and arrange horizontally without overlap",
            Arrangement::CenterY=>"Align vertical centers and arrange horizontally without overlap",
            Arrangement::Bottom=>"Align bottom edges and arrange horizontally without overlap",
            Arrangement::EvenX=>"Distribute horizontally, accounting for object widths",
            Arrangement::EvenY=>"Distribute vertically, accounting for object heights",
            Arrangement::Grid=>"Arrange in a grid with the specified number of columns",
        };
        button.on_hover_text(explanation).clicked()
    }
    pub(super) fn arrangement_menu(&mut self,ui:&mut egui::Ui) {
        ui.label(self.language.text("Align", "整列"));
        ui.horizontal(|ui|{
            for (label,mode) in [(self.language.text("Left", "左"),Arrangement::Left),(self.language.text("Center", "中央"),Arrangement::CenterX),(self.language.text("Right", "右"),Arrangement::Right)] {
                if Self::arrangement_icon(ui,label,mode){self.arrange_selected(mode);ui.close();}
            }
        });
        ui.horizontal(|ui|{
            for (label,mode) in [(self.language.text("Top", "上"),Arrangement::Top),(self.language.text("Center", "中央"),Arrangement::CenterY),(self.language.text("Bottom", "下"),Arrangement::Bottom)] {
                if Self::arrangement_icon(ui,label,mode){self.arrange_selected(mode);ui.close();}
            }
        });

    }
    pub(super) fn change_selected_line_z(&mut self,direction:i32) {
        let chosen=self.canvas_selection.clone();
        let mut order:Vec<(i32,CanvasTarget)>=self.doc.shapes.iter().map(|s|(s.z,CanvasTarget::Shape(s.id)))
            .chain(self.doc.lines.iter().map(|l|(l.z,CanvasTarget::Line(l.id)))).collect();
        order.sort_by_key(|(z,t)|(*z,format!("{t:?}")));
        let before=order.iter().map(|(_,t)|*t).collect::<Vec<_>>();
        if direction.abs()==2 {
            order.sort_by_key(|(_,t)|if chosen.contains(t){if direction==2{1}else{0}}else{if direction==2{0}else{1}});
        } else if direction==1 {
            for i in (0..order.len().saturating_sub(1)).rev(){
                if chosen.contains(&order[i].1)&&!chosen.contains(&order[i+1].1){order.swap(i,i+1);}
            }
        } else if direction==-1 {
            for i in 1..order.len(){
                if chosen.contains(&order[i].1)&&!chosen.contains(&order[i-1].1){order.swap(i,i-1);}
            }
        }
        if order.iter().map(|(_,t)|*t).collect::<Vec<_>>()==before{return;}
        self.checkpoint();
        for (z,(_,target)) in order.into_iter().enumerate(){
            match target {
                CanvasTarget::Shape(id)=>{if let Some(s)=self.doc.shape_mut(id){s.z=z as i32;}},
                CanvasTarget::Line(id)=>{if let Some(l)=self.doc.line_mut(id){l.z=z as i32;}},
                _=>{},
            }
        }
    }
    pub(super) fn move_target_world(&mut self,target:CanvasTarget,dx:f32,dy:f32) {
        match target {
            CanvasTarget::Node(id)=>{if let Some(n)=self.doc.node_mut(id){n.offset.x+=dx;n.offset.y+=dy;}},
            CanvasTarget::TextBox(id)=>{if let Some(b)=self.doc.text_box_mut(id){b.position.x+=dx;b.position.y+=dy;}},
            CanvasTarget::Shape(id)=>{if let Some(s)=self.doc.shape_mut(id){if !s.locked{s.position.x+=dx;s.position.y+=dy;}}},
            CanvasTarget::Line(id)=>{if let Some(l)=self.doc.line_mut(id){if !l.locked{l.move_by(dx,dy);}}},
        }
    }
    pub(super) fn target_world_rect(&self,target:CanvasTarget,positions:&HashMap<NodeId,Position>)->Option<Rect> {
        match target {
            CanvasTarget::Node(id)=>{
                let node=self.doc.node(id)?;let pos=positions.get(&id)?;
                Some(Rect::from_center_size(Pos2::new(pos.x,pos.y),egui::vec2(node_width_text(&node.title,self.doc.depth(id)),card_height(node,self.doc.depth(id)))))
            }
            CanvasTarget::TextBox(id)=>{let b=self.doc.text_box(id)?;Some(Rect::from_min_size(Pos2::new(b.position.x,b.position.y),egui::vec2(b.width,b.height)))}
            CanvasTarget::Shape(id)=>{let s=self.doc.shape(id)?;Some(Rect::from_min_size(Pos2::new(s.position.x,s.position.y),egui::vec2(s.width,s.height)))}
            CanvasTarget::Line(id)=>{let l=self.doc.line(id)?;Some(Rect::from_two_pos(Pos2::new(l.start.x,l.start.y),Pos2::new(l.end.x,l.end.y)))},
        }
    }
    pub(super) fn arrange_selected(&mut self,how:Arrangement) {
        let positions=self.positions();
        let mut objects:Vec<_>=self.canvas_selection.iter().copied().filter_map(|t|{
            let r=self.target_world_rect(t,&positions)?;Some((t,r))
        }).collect();
        if objects.len()<2{return;}
        // Skip locked objects. Frames are moved separately from their members.
        objects.retain(|(t,_)|match t {CanvasTarget::Shape(id)=>self.doc.shape(*id).is_some_and(|s|!s.locked),CanvasTarget::Line(id)=>self.doc.line(*id).is_some_and(|l|!l.locked),_=>true});
        if objects.len()<2{return;}
        let union=objects.iter().fold(objects[0].1,|r,(_,b)|r.union(*b));
        let mut moves=Vec::new();
        match how {
            Arrangement::Left|Arrangement::CenterX|Arrangement::Right|
            Arrangement::Top|Arrangement::CenterY|Arrangement::Bottom=>{
                for &(t,r) in &objects {
                    let (dx,dy)=match how {
                        Arrangement::Left=>(union.left()-r.left(),0.0),
                        Arrangement::CenterX=>(union.center().x-r.center().x,0.0),
                        Arrangement::Right=>(union.right()-r.right(),0.0),
                        Arrangement::Top=>(0.0,union.top()-r.top()),
                        Arrangement::CenterY=>(0.0,union.center().y-r.center().y),
                        _=>(0.0,union.bottom()-r.bottom()),
                    };
                    moves.push((t,dx,dy));
                }
                // Align to an edge/center, then resolve collisions along the
                // orthogonal axis. Merely setting all X or all Y positions
                // previously piled up sticky notes on top of each other.
                let x_alignment=matches!(how,Arrangement::Left|Arrangement::CenterX|Arrangement::Right);
                let mut order:Vec<_>=(0..objects.len()).collect();
                order.sort_by(|&a,&b| {
                    let x=if x_alignment{objects[a].1.top()}else{objects[a].1.left()};
                    let y=if x_alignment{objects[b].1.top()}else{objects[b].1.left()};
                    x.total_cmp(&y)
                });
                let mut next=f32::NEG_INFINITY;
                for index in order {
                    let r=objects[index].1;
                    let (_target,dx,dy)=&mut moves[index];
                    let start=if x_alignment{r.top()+*dy}else{r.left()+*dx};
                    if start<next {
                        if x_alignment {*dy+=next-start;} else {*dx+=next-start;}
                    }
                    next=if x_alignment{r.bottom()+*dy}else{r.right()+*dx};
                    next+=24.0;
                }
            }
            Arrangement::EvenX|Arrangement::EvenY=>{
                if how_is_x(how){objects.sort_by(|a,b|a.1.center().x.total_cmp(&b.1.center().x));}
                else {objects.sort_by(|a,b|a.1.center().y.total_cmp(&b.1.center().y));}
                let n=objects.len();
                let size_sum:f32=objects.iter().map(|(_,r)|if how_is_x(how){r.width()}else{r.height()}).sum();
                let length=if how_is_x(how){union.width()}else{union.height()};
                // Negative gap means overlap. Keep a minimum 24 world-unit
                // separation, even when all input notes start at the same X/Y.
                let gap=((length-size_sum)/(n-1) as f32).max(24.0);
                let mut cursor=if how_is_x(how){union.left()}else{union.top()};
                for (t,r) in &objects {
                    let d=cursor-if how_is_x(how){r.left()}else{r.top()};
                    moves.push((*t,if how_is_x(how){d}else{0.0},if how_is_x(how){0.0}else{d}));
                    cursor+=if how_is_x(how){r.width()}else{r.height()};cursor+=gap;
                }
            }
            Arrangement::Grid=>{
                objects.sort_by(|a,b|a.1.top().total_cmp(&b.1.top()).then(a.1.left().total_cmp(&b.1.left())));
                let cols=self.arrange_columns.clamp(1,32);
                let cell_w=objects.iter().map(|(_,r)|r.width()).fold(0.0_f32,f32::max)+24.0;
                let cell_h=objects.iter().map(|(_,r)|r.height()).fold(0.0_f32,f32::max)+24.0;
                for (i,(t,r)) in objects.iter().enumerate() {
                    moves.push((*t,union.left()+(i%cols) as f32*cell_w-r.left(),union.top()+(i/cols) as f32*cell_h-r.top()));
                }
            }
        }
        if moves.iter().all(|(_,dx,dy)|dx.abs()<0.001&&dy.abs()<0.001){return;}
        self.checkpoint();
        let chosen:HashSet<_>=moves.iter().map(|(t,_,_)|*t).collect();
        // A Frame is a movable container, but members can also be explicitly selected.
        // Never translate a member twice as part of the same alignment operation.
        let mut member_moves=Vec::new();
        for (target,dx,dy) in &moves {
            if let CanvasTarget::Shape(id)=target {
                if let Some(frame)=self.doc.shape(*id).filter(|s|s.kind==ShapeKind::Frame) {
                    member_moves.extend(frame.members.iter().filter(|t|!chosen.contains(*t))
                        .map(|&member|(member,*dx,*dy)));
                }
            }
        }
        for (target,dx,dy) in moves.into_iter().chain(member_moves) {self.move_target_world(target,dx,dy);}
    }
    pub(super) fn create_frame_for_selection(&mut self) {
        let positions=self.positions();
        let objects:Vec<_>=self.canvas_selection.iter().filter(|&&t|match t {
            CanvasTarget::Shape(id)=>self.doc.shape(id).is_some_and(|s|s.kind!=ShapeKind::Frame),
            _=>true,
        })
            .filter_map(|t|self.target_world_rect(*t,&positions).map(|r|(*t,r))).collect();
        if objects.is_empty(){return;}
        let union=objects.iter().fold(objects[0].1,|r,(_,b)|r.union(*b)).expand(28.0);
        let targets:HashSet<_>=objects.iter().map(|(t,_)|*t).collect();
        self.checkpoint();
        if let Some(id)=self.doc.add_frame_for(&targets,Offset{x:union.left(),y:union.top()-24.0},union.width(),union.height()+24.0) {
            let members:Vec<_>=targets.iter().copied().collect();
            self.transfer_stickies_to_frame(id,&members);
            self.refresh_frame_grid(id);
            self.select_shape(id,false);
        }
    }
    pub(super) fn attach_selection_to_frame(&mut self,frame_id:u64) {
        if !self.doc.shape(frame_id).is_some_and(|s|s.kind==ShapeKind::Frame){return;}
        let chosen:Vec<_>=self.canvas_selection.iter().copied().filter(|t|match t {
            CanvasTarget::Shape(id)=>self.doc.shape(*id).is_some_and(|s|s.kind!=ShapeKind::Frame),
            _=>true,
        }).collect();
        let existing=self.doc.shape(frame_id).unwrap().members.clone();
        let additions:Vec<_>=chosen.into_iter().filter(|t|!existing.contains(t)).collect();
        if additions.is_empty(){return;}
        self.checkpoint();
        let count=additions.len();
        self.transfer_stickies_to_frame(frame_id,&additions);
        if let Some(frame)=self.doc.shape_mut(frame_id){frame.members.extend(additions);}
        self.refresh_frame_grid(frame_id);
        self.status=format!("Added {count} items to the frame");
    }
    pub(super) fn detach_selected_from_frames(&mut self) {
        let targets=self.canvas_selection.clone();
        if !self.doc.shapes.iter().any(|s|s.kind==ShapeKind::Frame&&s.members.iter().any(|m|targets.contains(m))){return;}
        self.checkpoint();
        let frames:Vec<_>=self.doc.shapes.iter().filter(|f|f.kind==ShapeKind::Frame).map(|f|f.id).collect();
        for frame in &mut self.doc.shapes {if frame.kind==ShapeKind::Frame {frame.members.retain(|m|!targets.contains(m));}}
        for id in frames {self.refresh_frame_grid(id);}
    }
    /// Explicitly attach objects whose center falls inside a Frame. This is never
    /// automatic: no membership is assigned simply by dragging an object across a border.
    pub(super) fn attach_contained_to_frame(&mut self,frame_id:u64) {
        let Some(frame)=self.doc.shape(frame_id) else {return;};
        if frame.kind!=ShapeKind::Frame {return;}
        let bounds=Rect::from_min_size(Pos2::new(frame.position.x,frame.position.y),
            egui::vec2(frame.width,frame.height));
        let positions=self.positions();
        let enclosed:Vec<_>=self.doc.shapes.iter().filter(|s|s.visible && s.id!=frame_id && s.kind!=ShapeKind::Frame)
            .map(|s|CanvasTarget::Shape(s.id))
            .chain(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)))
            .chain(self.doc.lines.iter().filter(|l|l.visible).map(|l|CanvasTarget::Line(l.id)))
            .chain(self.doc.nodes.iter().filter(|n|positions.contains_key(&n.id)).map(|n|CanvasTarget::Node(n.id)))
            .filter(|t|self.target_world_rect(*t,&positions).is_some_and(|r|bounds.contains(r.center())))
            .collect();
        let existing=self.doc.shape(frame_id).unwrap().members.clone();
        let added:Vec<_>=enclosed.into_iter().filter(|t|!existing.contains(t)).collect();
        if added.is_empty(){self.status="There are no unassigned objects inside the frame".into();return;}
        self.checkpoint();
        self.transfer_stickies_to_frame(frame_id,&added);
        if let Some(f)=self.doc.shape_mut(frame_id){f.members.extend(added.iter().copied());}
        self.refresh_frame_grid(frame_id);
        self.status=format!("Added {} items to the frame", added.len());
    }
    /// Attach selected objects to any existing Frame without deselecting them.
    /// Frames cannot be members of other Frames (version 2 invariant).
    pub(super) fn draw_selected_sticky_color(&mut self,fill:[u8;3]) {
        let ids:Vec<_>=self.canvas_selection.iter().filter_map(|t|match t{CanvasTarget::Shape(id)=>Some(*id),_=>None}).collect();
        if ids.is_empty(){return;}
        self.checkpoint();
        for id in ids {if let Some(shape)=self.doc.shape_mut(id){shape.style.fill=Some(fill);shape.style.text=if f32::from(fill[0])*0.299+f32::from(fill[1])*0.587+f32::from(fill[2])*0.114<140.0 {[255,255,255]}else{[43,48,60]};}}
    }
}
fn how_is_x(how:Arrangement)->bool{matches!(how,Arrangement::EvenX)}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_expands_on_note_drop_and_does_not_shrink_existing_extent() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.zoom=1.0;
        let f=app.doc.add_shape(ShapeKind::Frame,Offset{x:100.0,y:100.0});
        let n=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:200.0,y:200.0});
        {let frame=app.doc.shape_mut(f).unwrap();frame.width=120.0;frame.height=110.0;}
        {let note=app.doc.shape_mut(n).unwrap();note.width=240.0;note.height=160.0;}
        let preview=app.preview_grid_insert(f,n,Pos2::new(210.0,205.0),Pos2::ZERO).unwrap();
        assert!(preview.grid.dimensions().0>=280.0);
        assert!(preview.grid.dimensions().1>=220.0);
        assert!(app.doc.shape(f).unwrap().members.is_empty(),"preview must be read-only");
        app.update_frame_membership_on_drop(n,Pos2::new(210.0,205.0),Pos2::ZERO);
        let actual=app.doc.shape(f).unwrap();
        assert_eq!((actual.width,actual.height),preview.grid.dimensions());
        assert!(actual.members.contains(&CanvasTarget::Shape(n)));
        assert!(actual.grid.as_ref().unwrap().slots.iter().any(|s|s.target==CanvasTarget::Shape(n)));
    }

    #[test]
    fn frame_drop_anywhere_inside_adds_and_outside_removes_membership() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.zoom=1.0;
        let frame=app.doc.add_shape(ShapeKind::Frame,Offset{x:100.0,y:100.0});
        let note=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:140.0,y:140.0});
        let origin=Pos2::ZERO;
        // The title region supports attach, but any interior point works.
        app.update_frame_membership_on_drop(note,Pos2::new(160.0,112.0),origin);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(note)));
        // A release inside the frame body does not detach.
        app.update_frame_membership_on_drop(note,Pos2::new(180.0,190.0),origin);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(note)));
        // The whole Frame body, not just its title, is an attach target.
        app.doc.shape_mut(frame).unwrap().members.clear();
        assert_eq!(app.frame_drop_intent(note,Pos2::new(180.0,190.0),origin),
            Some(FrameDropIntent::Attach(frame)));
        assert!(app.doc.shape(frame).unwrap().members.is_empty(),"preview must not mutate membership");
        app.update_frame_membership_on_drop(note,Pos2::new(180.0,190.0),origin);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(note)));
        assert_eq!(app.frame_drop_intent(note,Pos2::new(180.0,190.0),origin),None);
        assert_eq!(app.frame_drop_intent(note,Pos2::new(600.0,600.0),origin),Some(FrameDropIntent::Detach));
        // Outside detaches and does not delete the sticker.
        app.update_frame_membership_on_drop(note,Pos2::new(600.0,600.0),origin);
        assert!(!app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(note)));
        assert!(app.doc.shape(note).is_some());
    }
    #[test]
    fn dragging_to_second_frame_transfers_membership_and_previews_blue() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.zoom=1.0;
        let first=app.doc.add_shape(ShapeKind::Frame,Offset{x:0.0,y:0.0});
        let second=app.doc.add_shape(ShapeKind::Frame,Offset{x:600.0,y:0.0});
        let note=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:15.0,y:60.0});
        app.update_frame_membership_on_drop(note,Pos2::new(45.0,75.0),Pos2::ZERO);
        assert!(app.doc.shape(first).unwrap().members.contains(&CanvasTarget::Shape(note)));
        assert_eq!(app.frame_drop_intent(note,Pos2::new(625.0,75.0),Pos2::ZERO),
            Some(FrameDropIntent::Attach(second)));
        app.update_frame_membership_on_drop(note,Pos2::new(625.0,75.0),Pos2::ZERO);
        assert!(!app.doc.shape(first).unwrap().members.contains(&CanvasTarget::Shape(note)));
        assert!(app.doc.shape(second).unwrap().members.contains(&CanvasTarget::Shape(note)));
    }
    #[test]
    fn frame_drag_itself_does_not_attach_itself() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let frame=app.doc.add_shape(ShapeKind::Frame,Offset{x:20.0,y:20.0});
        app.update_frame_membership_on_drop(frame,Pos2::new(30.0,30.0),Pos2::ZERO);
        assert!(app.doc.shape(frame).unwrap().members.is_empty());
    }
    #[test]
    fn line_geometry_hit_is_screen_space() {
        assert!(line_distance_sq(Pos2::new(50.0,8.0),Pos2::new(0.0,0.0),Pos2::new(100.0,0.0))<100.0);
        assert!(line_distance_sq(Pos2::new(50.0,12.0),Pos2::new(0.0,0.0),Pos2::new(100.0,0.0))>100.0);
    }
    #[test]
    fn constrained_creation_uses_45_degree_steps() {
        let end=MindmapApp::constrained_line_end(Pos2::ZERO,Pos2::new(100.0,25.0),true);
        assert!(end.y.abs()<0.01);
    }
    #[test]
    fn horizontal_distribution_does_not_overlap_when_initial_positions_collide() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let ids:Vec<_>=(0..4).map(|i|app.doc.add_shape(ShapeKind::StickyNote,
            Offset{x:10.0,y:i as f32*35.0})).collect();
        app.canvas_selection=ids.iter().map(|&id|CanvasTarget::Shape(id)).collect();
        app.arrange_selected(Arrangement::EvenX);
        let mut sorted=ids.iter().map(|&id|app.doc.shape(id).unwrap()).collect::<Vec<_>>();
        sorted.sort_by(|a,b|a.position.x.total_cmp(&b.position.x));
        for pair in sorted.windows(2){
            assert!(pair[1].position.x >= pair[0].position.x+pair[0].width+23.99,
                "distributed notes must not overlap");
        }
        app.undo();
        assert!(ids.iter().all(|&id|app.doc.shape(id).unwrap().position.x==10.0));
    }
    #[test]
    fn vertical_distribution_and_grid_keep_separation() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let ids:Vec<_>=(0..5).map(|i|app.doc.add_shape(ShapeKind::StickyNote,
            Offset{x:i as f32*15.0,y:0.0})).collect();
        app.canvas_selection=ids.iter().map(|&id|CanvasTarget::Shape(id)).collect();
        app.arrange_selected(Arrangement::EvenY);
        let mut sorted=ids.iter().map(|&id|app.doc.shape(id).unwrap()).collect::<Vec<_>>();
        sorted.sort_by(|a,b|a.position.y.total_cmp(&b.position.y));
        for pair in sorted.windows(2){
            assert!(pair[1].position.y >= pair[0].position.y+pair[0].height+23.99);
        }
        app.arrange_columns=2;
        app.arrange_selected(Arrangement::Grid);
        let objects:Vec<_>=ids.iter().map(|&id|app.doc.shape(id).unwrap()).collect();
        for i in 0..objects.len(){for j in i+1..objects.len(){
            let a=objects[i];let b=objects[j];
            let separated=a.position.x+a.width<=b.position.x || b.position.x+b.width<=a.position.x
                ||a.position.y+a.height<=b.position.y || b.position.y+b.height<=a.position.y;
            assert!(separated,"grid elements overlap: {i}, {j}");
        }}
    }
    #[test]
    fn alignment_auto_resolves_collisions_without_moving_unselected() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let a=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:30.0,y:30.0});
        let b=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:70.0,y:31.0});
        let c=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:110.0,y:32.0});
        let outside=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:600.0,y:500.0});
        app.canvas_selection=HashSet::from([CanvasTarget::Shape(a),CanvasTarget::Shape(b),CanvasTarget::Shape(c)]);
        app.arrange_selected(Arrangement::Left);
        let a=app.doc.shape(a).unwrap();let b=app.doc.shape(b).unwrap();let c=app.doc.shape(c).unwrap();
        assert_eq!(a.position.x,b.position.x);
        assert_eq!(b.position.x,c.position.x);
        assert!(b.position.y>=a.position.y+a.height+23.99);
        assert!(c.position.y>=b.position.y+b.height+23.99);
        assert_eq!(app.doc.shape(outside).unwrap().position,Offset{x:600.0,y:500.0});
    }
    #[test]
    fn frame_containment_can_be_attached_explicitly_and_moved() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let inside=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:120.0,y:120.0});
        let outside=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:800.0,y:800.0});
        let frame=app.doc.add_shape(ShapeKind::Frame,Offset{x:80.0,y:80.0});
        assert!(app.hit_frame_interior(Pos2::new(350.0,280.0),Pos2::ZERO).is_some());
        app.attach_contained_to_frame(frame);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(inside)));
        assert!(!app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(outside)));
        app.select_shape(frame,false);
        let prior=app.doc.shape(inside).unwrap().position;
        let frame_prior=app.doc.shape(frame).unwrap().position;
        app.checkpoint();
        // Drag deltas are screen pixels; movement is persisted in world units.
        // At the default zoom (0.92), 50 world units correspond to 46 screen pixels.
        app.move_canvas_selection(egui::vec2(50.0*app.zoom,0.0));
        let after=app.doc.shape(inside).unwrap().position;
        let moved_frame=app.doc.shape(frame).unwrap().position;
        assert!((after.x-(prior.x+50.0)).abs()<0.001);
        assert!((after.y-prior.y).abs()<0.001);
        assert!((moved_frame.x-(frame_prior.x+50.0)).abs()<0.001);
        assert_eq!(app.doc.shape(outside).unwrap().position.x,800.0);
        app.undo();
        assert_eq!(app.doc.shape(inside).unwrap().position,prior);
        assert_eq!(app.doc.shape(frame).unwrap().position,frame_prior);
    }
    #[test]
    fn continuous_sticky_commits_previous_text_before_creating_next() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.insert_shape_mode=Some(ShapeKind::StickyNote);
        app.add_shape_at(ShapeKind::StickyNote,Pos2::new(80.0,80.0),Pos2::ZERO);
        let first=app.editing_shape.expect("first note should start inline editor");
        app.shape_edit_text="日本語でブレインストーミング".to_string();
        app.add_shape_at(ShapeKind::StickyNote,Pos2::new(440.0,80.0),Pos2::ZERO);
        assert_eq!(app.doc.shape(first).unwrap().text,"日本語でブレインストーミング");
        assert_ne!(app.editing_shape,Some(first));
        assert_eq!(app.insert_shape_mode,Some(ShapeKind::StickyNote));
    }
    #[test]
    fn explicit_frame_membership_can_be_removed_with_undo() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let note=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:100.0,y:100.0});
        let frame=app.doc.add_shape(ShapeKind::Frame,Offset{x:30.0,y:30.0});
        app.select_shape(note,false);
        app.attach_selection_to_frame(frame);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(note)));
        app.detach_selected_from_frames();
        assert!(app.doc.shape(frame).unwrap().members.is_empty());
        app.undo();
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(note)));
    }
    #[test]
    fn drawing_tools_remain_active_until_explicitly_cancelled() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.insert_shape_mode=Some(ShapeKind::Rectangle);
        app.add_dragged_shape(ShapeKind::Rectangle,Pos2::new(10.0,10.0),Pos2::new(90.0,90.0),Pos2::ZERO);
        assert_eq!(app.insert_shape_mode,Some(ShapeKind::Rectangle));
        app.insert_shape_mode=None;
        app.insert_line_mode=true;
        app.add_line_screen(Pos2::new(0.0,0.0),Pos2::new(100.0,0.0),Pos2::ZERO);
        assert!(app.insert_line_mode);
    }
    #[test]
    fn dragging_frame_moves_explicit_members_not_unrelated_objects() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let a=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:100.0,y:100.0});
        let b=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:500.0,y:100.0});
        let frame=app.doc.add_frame_for(&HashSet::from([CanvasTarget::Shape(a)]),
            Offset{x:80.0,y:80.0},300.0,200.0).unwrap();
        app.select_shape(frame,false);
        let previous=app.doc.shape(a).unwrap().position;
        let untouched=app.doc.shape(b).unwrap().position;
        app.move_canvas_selection(egui::vec2(36.0,22.0));
        assert_ne!(app.doc.shape(a).unwrap().position,previous);
        assert_eq!(app.doc.shape(b).unwrap().position,untouched);
    }
    #[test]
    fn alignment_only_moves_selection() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let a=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:5.0,y:0.0});
        let b=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:480.0,y:120.0});
        let c=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:800.0,y:300.0});
        app.canvas_selection=HashSet::from([CanvasTarget::Shape(a),CanvasTarget::Shape(b)]);
        let other=app.doc.shape(c).unwrap().position;
        app.arrange_selected(Arrangement::Top);
        assert_eq!(app.doc.shape(a).unwrap().position.y,app.doc.shape(b).unwrap().position.y);
        assert_eq!(app.doc.shape(c).unwrap().position,other);
        app.undo();
        assert_eq!(app.doc.shape(b).unwrap().position.y,120.0);
    }

}
