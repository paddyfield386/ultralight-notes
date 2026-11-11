//! Generic selection, shapes, connector anchors, and z-order operations.
//! The tree remains owned by Document; CanvasTarget is the shared interaction interface.
use super::*;
use crate::spreadsheet::{CellCoord, TableInput, display_literal, evaluate_workbook, translate_formula};
use std::hash::{Hash, Hasher};

fn table_default_border(color:[u8;3],width:f32)->TableBorder {TableBorder{color,width:width.max(0.8),dash:TableBorderDash::Solid}}
fn paint_table_border(painter:&egui::Painter,a:Pos2,b:Pos2,border:TableBorder,zoom:f32) {
    let stroke=Stroke::new((border.width*zoom).max(0.6),color_from_rgb(border.color));
    if border.dash==TableBorderDash::Solid {painter.line_segment([a,b],stroke);return;}
    let delta=b-a;let length=delta.length();if length<=0.0{return;}let direction=delta/length;
    match border.dash {
        TableBorderDash::Dashed=>{let mut offset=0.0;while offset<length{let end=(offset+6.0*zoom).min(length);painter.line_segment([a+direction*offset,a+direction*end],stroke);offset+=10.0*zoom;}}
        TableBorderDash::Dotted=>{let mut offset=0.0;while offset<=length{painter.circle_filled(a+direction*offset,(stroke.width*0.65).max(0.7),stroke.color);offset+=4.0*zoom.max(0.35);}}
        TableBorderDash::Solid=>{}
    }
}

impl MindmapApp {
    pub(super) fn refresh_spreadsheet_cache(&mut self) {
        let tables: Vec<_> = self.doc.shapes.iter().filter_map(|shape| {
            (shape.kind == ShapeKind::Table).then(|| shape.table.as_ref().map(|table|(shape.id,table)))?
        }).collect();
        let mut hasher=std::collections::hash_map::DefaultHasher::new();
        (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|elapsed|elapsed.as_secs()/60))
            .hash(&mut hasher);
        for (id,table) in &tables {
            id.hash(&mut hasher);
            table.cells.hash(&mut hasher);
            table.formula_cells.hash(&mut hasher);
            table.manually_hidden_rows.hash(&mut hasher);
            table.filters.hash(&mut hasher);
        }
        let key=hasher.finish();
        if key==self.spreadsheet_cache_key{return;}
        let formula_sets:Vec<HashSet<CellCoord>>=tables.iter().map(|(_,table)|table.formula_cells.iter()
            .map(|(row,column)|CellCoord{row:*row,column:*column}).collect()).collect();
        let manually_hidden_sets:Vec<HashSet<usize>>=tables.iter().map(|(_,table)|table.manually_hidden_rows.iter().copied().collect()).collect();
        let filtered_sets:Vec<HashSet<usize>>=tables.iter().map(|(_,table)|(0..table.cells.len()).filter(|row|table.row_is_filtered_out(*row)).collect()).collect();
        let merge_sets:Vec<Vec<_>>=tables.iter().map(|(_,table)|table.merged_cells.iter().map(|merge|
            (merge.row,merge.column,merge.rows,merge.columns)).collect()).collect();
        let inputs:Vec<_>=tables.iter().zip(formula_sets.iter()).zip(manually_hidden_sets.iter()).zip(filtered_sets.iter()).zip(merge_sets.iter())
            .map(|(((((id,table),formulas),hidden),filtered),merges)|TableInput{
                id:*id,cells:&table.cells,formula_cells:formulas,manually_hidden_rows:hidden,filtered_out_rows:filtered,merged_cells:merges,
            }).collect();
        self.spreadsheet_cache=evaluate_workbook(&inputs);
        self.spreadsheet_cache_key=key;
    }

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
        self.editing_table_cell = None;
        self.selected_table_cell = None;
        self.selected_table_range = None;
        self.shape_edit_text.clear();
    }
    pub(super) fn select_shape(&mut self, id: u64, additive: bool) {
        if self.doc.shape(id).is_none() { return; }
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        self.finish_shape_edit(true);
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
        if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Table) {
            self.ribbon_tab=2;
            if !additive && !self.selected_table_cell.is_some_and(|(table,_,_)|table==id) {
                self.selected_table_cell=Some((id,0,0));
                self.selected_table_range=Some((id,0,0,0,0));
            }
        }
        else {
            self.selected_table_cell=None;
            if self.ribbon_tab==2 {self.ribbon_tab=0;}
        }
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
        if item.kind == ShapeKind::Table {
            self.start_table_cell_edit(id, 0, 0);
            return;
        }
        self.shape_edit_text = item.text.clone();
        self.select_shape(id, false);
        self.editing_shape = Some(id);
        self.shape_edit_focus_pending=true;
    }
    pub(super) fn finish_shape_edit(&mut self, commit: bool) {
        if let Some(id) = self.editing_shape.take() {
            self.formula_candidates_hidden_prefix=None;
            self.formula_candidate_index=0;
            self.formula_reference_range=None;
            self.formula_reference_mode=false;
            self.formula_reference_target=None;
            self.formula_reference_anchor=None;
            self.table_cell_input_mode=false;
            self.table_overwrite_pending=false;
            self.table_hidden_overwrite_selection=false;
            let table_cell=self.editing_table_cell.take();
            let formula_value=table_cell.is_some()&&self.shape_edit_text.starts_with('=');
            let changed=if let Some((row,col))=table_cell {
                self.doc.shape(id).and_then(|s|s.table.as_ref()?.cells.get(row)?.get(col))
                    .is_some_and(|value|value != &self.shape_edit_text)
                    ||self.doc.shape(id).and_then(|s|s.table.as_ref())
                        .is_some_and(|table|table.formula_cells.contains(&(row,col))!=formula_value)
            } else { self.doc.shape(id).is_some_and(|s| s.text != self.shape_edit_text) };
            let row_target=table_cell.and_then(|(row,col)|self.table_row_target_height(id,row,commit.then(||(row,col,self.shape_edit_text.clone()))));
            let row_changed=row_target.is_some_and(|(row,height)|self.doc.shape(id).and_then(|s|s.table.as_ref()).is_some_and(|t|(t.row_height(row)-height).abs()>0.01));
            if commit && (changed||row_changed) {
                self.checkpoint();
            }
            if let Some(shape)=self.doc.shape_mut(id) {
                if let Some((row,col))=table_cell {
                    if commit {if let Some(table)=shape.table.as_mut(){
                        if let Some(value)=table.cells.get_mut(row).and_then(|r|r.get_mut(col)){*value=self.shape_edit_text.clone();}
                        table.formula_cells.retain(|coord|*coord!=(row,col));
                        if formula_value {table.formula_cells.push((row,col));}
                    }}
                    if let Some((target_row,target_height))=row_target {
                        if let Some(table)=shape.table.as_mut(){
                            if table.row_heights.is_empty(){table.row_heights=vec![TABLE_CELL_HEIGHT;table.cells.len()];}
                            let delta=target_height-table.row_heights[target_row];
                            table.row_heights[target_row]=target_height;shape.height+=delta;
                        }
                    }
                } else if commit&&changed {shape.text=self.shape_edit_text.clone();}
            }
        }
    }
    fn table_row_target_height(&self,id:u64,row:usize,edited:Option<(usize,usize,String)>)->Option<(usize,f32)> {
        let table=self.doc.shape(id)?.table.as_ref()?;
        let cells=table.cells.get(row)?;
        let lines=cells.iter().enumerate().map(|(col,value)|{
            let text=edited.as_ref().filter(|(edited_row,edited_col,_)|*edited_row==row&&*edited_col==col).map_or(value.as_str(),|(_,_,text)|text.as_str());
            text.split('\n').count().max(1)
        }).max().unwrap_or(1);
        Some((row,(lines as f32*TABLE_CELL_HEIGHT).min(50_000.0)))
    }
    pub(super) fn start_table_cell_edit(&mut self,id:u64,row:usize,col:usize) {
        self.finish_shape_edit(true);
        let Some(shape)=self.doc.shape(id).filter(|s|s.kind==ShapeKind::Table&&!s.locked) else{return};
        let Some(value)=shape.table.as_ref().and_then(|t|t.cells.get(row)).and_then(|r|r.get(col)) else{return};
        let text=value.clone();
        self.select_shape(id,false);
        self.shape_edit_text=text;
        self.formula_candidates_hidden_prefix=None;
        self.formula_candidate_index=0;
        self.formula_reference_range=None;
        self.formula_reference_mode=false;
        self.formula_reference_target=None;
        self.formula_reference_anchor=None;
        self.table_cell_input_mode=false;
        self.editing_shape=Some(id);
        self.editing_table_cell=Some((row,col));
        self.table_overwrite_pending=true;
        self.table_hidden_overwrite_selection=true;
        self.selected_table_cell=Some((id,row,col));
        self.selected_table_range=Some((id,row,col,row,col));
        self.shape_edit_focus_pending=true;
    }
    pub(super) fn table_cell_at(&self,id:u64,pointer:Pos2,origin:Pos2)->Option<(usize,usize)> {
        let shape=self.doc.shape(id)?;
        let cells=&shape.table.as_ref()?.cells;
        let rect=self.shape_screen_rect(id,origin)?;
        if !rect.contains(pointer)||cells.is_empty()||cells[0].is_empty(){return None;}
        let local_y=(pointer.y-rect.top())/self.zoom;
        let row=(0..cells.len()).find(|row|local_y<shape.table.as_ref().unwrap().row_offset(*row)+shape.table.as_ref().unwrap().row_height(*row)).unwrap_or(cells.len()-1);
        let col=(((pointer.x-rect.left())/rect.width()*cells[0].len() as f32).floor() as usize).min(cells[0].len()-1);
        let table=shape.table.as_ref()?;
        let (row,col)=self.spreadsheet_cache.spill_origin_at(id,row,col)
            .map_or((row,col),|origin|(origin.coord.row,origin.coord.column));
        Some(table.merge_at(row,col).map_or((row,col),|merge|(merge.row,merge.column)))
    }
    pub(super) fn move_table_cell_selection(&mut self,id:u64,row:usize,col:usize,dr:isize,dc:isize,extend:bool) {
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
        if rows==0||cols==0{return;}
        let merge=table.merge_at(row,col);
        let base_row=merge.map_or(row,|m|m.row);let base_col=merge.map_or(col,|m|m.column);
        let base_rows=merge.map_or(1,|m|m.rows);let base_cols=merge.map_or(1,|m|m.columns);
        let target_row=if dr<0{base_row.saturating_sub(1)}else if dr>0{(base_row+base_rows).min(rows-1)}else{base_row};
        let target_col=if dc<0{base_col.saturating_sub(1)}else if dc>0{(base_col+base_cols).min(cols-1)}else{base_col};
        let (next_row,next_col)=table.merge_at(target_row,target_col).map_or((target_row,target_col),|m|(m.row,m.column));
        let anchor=self.selected_table_range.filter(|(table_id,_,_,_,_)|*table_id==id)
            .map(|(_,r0,c0,_,_)|(r0,c0)).unwrap_or((row,col));
        self.selected_table_cell=Some((id,next_row,next_col));
        self.selected_table_range=Some(if extend{(id,anchor.0,anchor.1,next_row,next_col)}else{(id,next_row,next_col,next_row,next_col)});
    }
    pub(super) fn table_move_handle_at(&self,id:u64,pointer:Pos2,origin:Pos2)->bool {
        let Some(rect)=self.shape_screen_rect(id,origin) else{return false};
        let outer=rect.expand(10.0);
        outer.contains(pointer)&&!rect.contains(pointer)
    }
    fn selected_table_range_rect(&self,id:u64,origin:Pos2)->Option<Rect> {
        let (_,r0,c0,r1,c1)=self.selected_table_range.filter(|(table,_,_,_,_)|*table==id)?;
        let shape=self.doc.shape(id)?;
        let table=shape.table.as_ref()?;
        let rows=table.cells.len();let cols=table.cells.first()?.len();
        if rows==0||cols==0{return None;}
        let (top,bottom,left,right)=(r0.min(r1),r0.max(r1),c0.min(c1),c0.max(c1));
        if bottom>=rows||right>=cols{return None;}
        let bounds=self.shape_screen_rect(id,origin)?;
        Some(Rect::from_min_max(
            Pos2::new(bounds.left()+bounds.width()*left as f32/cols as f32,bounds.top()+self.zoom*table.row_offset(top)),
            Pos2::new(bounds.left()+bounds.width()*(right+1) as f32/cols as f32,bounds.top()+self.zoom*(table.row_offset(bottom)+table.row_height(bottom)))))
    }
    pub(super) fn formula_reference_rect(&self,id:u64,origin:Pos2)->Option<Rect> {
        let (target_id,tr,tc)=self.formula_reference_target?;
        if target_id!=id{return None;}
        let shape=self.doc.shape(id)?;let table=shape.table.as_ref()?;
        let rows=table.cells.len();let cols=table.cells.first()?.len();
        if rows==0||cols==0||tr>=rows||tc>=cols{return None;}
        let (anchor_id,ar,ac)=self.formula_reference_anchor.unwrap_or((id,tr,tc));
        let (ar,ac)=if anchor_id==id&&ar<rows&&ac<cols{(ar,ac)}else{(tr,tc)};
        let mut top=ar.min(tr);let mut bottom=ar.max(tr);let mut left=ac.min(tc);let mut right=ac.max(tc);
        for (row,col) in [(ar,ac),(tr,tc)] {
            if let Some(m)=table.merge_at(row,col){top=top.min(m.row);bottom=bottom.max(m.row+m.rows-1);left=left.min(m.column);right=right.max(m.column+m.columns-1);}
        }
        let bounds=self.shape_screen_rect(id,origin)?;
        Some(Rect::from_min_max(
            Pos2::new(bounds.left()+bounds.width()*left as f32/cols as f32,bounds.top()+self.zoom*table.row_offset(top)),
            Pos2::new(bounds.left()+bounds.width()*(right+1) as f32/cols as f32,bounds.top()+self.zoom*(table.row_offset(bottom)+table.row_height(bottom)))))
    }
    pub(super) fn table_fill_handle_at(&self,id:u64,pointer:Pos2,origin:Pos2)->bool {
        self.selected_table_range_rect(id,origin).is_some_and(|rect|
            Rect::from_center_size(rect.right_bottom(),egui::vec2(13.0,13.0)).contains(pointer))
    }
    pub(super) fn apply_table_fill(&mut self,drag:(u64,usize,usize,usize,usize,usize,usize)) {
        let (id,r0,c0,r1,c1,target_row,target_col)=drag;
        let Some(shape)=self.doc.shape(id) else{return};
        let Some(table)=shape.table.as_ref() else{return};
        let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
        if rows==0||cols==0{return;}
        let (top,bottom,left,right)=(r0.min(r1),r0.max(r1),c0.min(c1),c0.max(c1));
        let (fill_top,fill_bottom,fill_left,fill_right)=(top.min(target_row),bottom.max(target_row),left.min(target_col),right.max(target_col));
        let source=(top..=bottom).map(|r|table.cells[r][left..=right].to_vec()).collect::<Vec<_>>();
        if source.is_empty(){return;}
        let single=source.len()==1&&source[0].len()==1;
        let seed=single.then(||source[0][0].trim().parse::<f64>().ok()).flatten();
        let vertical_series=(left==right&&source.len()>1).then(||source.iter().map(|r|r[0].trim().parse::<f64>().ok()).collect::<Option<Vec<_>>>());
        let vertical_series=vertical_series.flatten().map(|values|(values[0],*values.last().unwrap(),values[1]-values[0]));
        let horizontal_series=(top==bottom&&source[0].len()>1).then(||source[0].iter().map(|v|v.trim().parse::<f64>().ok()).collect::<Option<Vec<_>>>());
        let horizontal_series=horizontal_series.flatten().map(|values|(values[0],*values.last().unwrap(),values[1]-values[0]));
        let vertical_extension=target_row<top||target_row>bottom;
        let horizontal_extension=target_col<left||target_col>right;
        let vertical_distance=if target_row<top{top-target_row}else{target_row.saturating_sub(bottom)};
        let horizontal_distance=if target_col<left{left-target_col}else{target_col.saturating_sub(right)};
        let vertical=vertical_extension&&(!horizontal_extension||vertical_distance>=horizontal_distance);
        let source_number=if !single {None} else {seed};
        let mut updates=Vec::new();
        for row in fill_top..=fill_bottom.min(rows-1) {
            for col in fill_left..=fill_right.min(cols-1) {
                if row>=top&&row<=bottom&&col>=left&&col<=right{continue;}
                let source_row=top+(row-top)%source.len();
                let source_col=left+(col-left)%source[0].len();
                let source_formula=table.formula_cells.contains(&(source_row,source_col));
                let (value,is_formula)=if source_formula {
                    let delta_row=row as isize-source_row as isize;
                    let delta_col=col as isize-source_col as isize;
                    match translate_formula(&source[source_row-top][source_col-left],delta_row as i32,delta_col as i32,id) {
                        Ok(formula)=>(formula,true),Err(_)=>("#REF!".to_owned(),false),
                    }
                } else if let Some(number)=source_number.filter(|_|vertical||horizontal_extension) {
                    let offset=if vertical {
                        if target_row>bottom {row.saturating_sub(bottom) as isize} else {-(top.saturating_sub(row) as isize)}
                    } else if target_col>right {col.saturating_sub(right) as isize}
                    else {-(left.saturating_sub(col) as isize)};
                    let value=number+offset as f64;
                    (if (value-value.round()).abs()<0.000_001 {format!("{value:.0}")} else {value.to_string()},false)
                } else if vertical&&vertical_extension {
                    let value=vertical_series.map(|(first,last,step)|{
                        let number=if target_row>bottom{last+step*(row.saturating_sub(bottom) as f64)}
                            else{first-step*(top.saturating_sub(row) as f64)};
                        if (number-number.round()).abs()<0.000_001{format!("{number:.0}")}else{number.to_string()}
                    }).unwrap_or_else(||source[(row-top)%source.len()][(col-left)%source[0].len()].clone())
                    ;(value,false)
                } else if !vertical&&horizontal_extension {
                    let value=horizontal_series.map(|(first,last,step)|{
                        let number=if target_col>right{last+step*(col.saturating_sub(right) as f64)}
                            else{first-step*(left.saturating_sub(col) as f64)};
                        if (number-number.round()).abs()<0.000_001{format!("{number:.0}")}else{number.to_string()}
                    }).unwrap_or_else(||source[(row-top)%source.len()][(col-left)%source[0].len()].clone())
                    ;(value,false)
                } else {
                    (source[(row-top)%source.len()][(col-left)%source[0].len()].clone(),false)
                };
                updates.push((row,col,value,is_formula));
            }
        }
        if updates.is_empty(){return;}
        self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {
            for (row,col,value,is_formula) in updates {
                table.cells[row][col]=value;
                table.formula_cells.retain(|coord|*coord!=(row,col));
                if is_formula {table.formula_cells.push((row,col));}
            }
        }
        self.selected_table_cell=Some((id,target_row.min(rows-1),target_col.min(cols-1)));
        self.selected_table_range=Some((id,fill_top.min(rows-1),fill_left.min(cols-1),fill_bottom.min(rows-1),fill_right.min(cols-1)));
    }
    pub(super) fn table_add_row(&mut self,id:u64) {
        let rows=self.doc.shape(id).and_then(|s|s.table.as_ref()).map_or(0,|t|t.cells.len());
        self.table_insert_row(id,rows);
    }
    pub(super) fn table_add_column(&mut self,id:u64) {
        let cols=self.doc.shape(id).and_then(|s|s.table.as_ref())
            .and_then(|t|t.cells.first()).map_or(0,Vec::len);
        self.table_insert_column(id,cols);
    }
    pub(super) fn table_insert_row(&mut self,id:u64,index:usize) {
        self.finish_shape_edit(true);
        let Some(shape)=self.doc.shape(id).filter(|s|s.kind==ShapeKind::Table) else{return};
        let Some(table)=shape.table.as_ref() else{return};
        let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
        if rows>=50||cols==0{return;}
        let index=index.min(rows);
        self.checkpoint();
        if let Some(shape)=self.doc.shape_mut(id) {
            if let Some(table)=shape.table.as_mut() {
                for (row,_) in &mut table.formula_cells {if *row>=index {*row+=1;}}
                for row in &mut table.manually_hidden_rows {if *row>=index {*row+=1;}}
                table.cells.insert(index,vec![String::new();cols]);
                if !table.row_heights.is_empty(){table.row_heights.insert(index,TABLE_CELL_HEIGHT);}
                if let Some(borders)=table.borders.as_mut(){
                    let default=table_default_border(shape.style.outline.unwrap_or([128,158,210]),shape.style.stroke_width);
                    borders.horizontal.insert(index+1,vec![Some(default);cols]);
                    let source=if borders.vertical.is_empty(){Vec::new()}else{borders.vertical[index.saturating_sub(1).min(borders.vertical.len()-1)].clone()};
                    borders.vertical.insert(index,if source.is_empty(){vec![Some(default);cols+1]}else{source});
                }
                if let Some(fills)=table.cell_fills.as_mut(){fills.insert(index,vec![None;cols]);}
                if let Some(colors)=table.cell_text_colors.as_mut(){colors.insert(index,vec![None;cols]);}
                if let Some(alignments)=table.cell_alignments.as_mut(){alignments.insert(index,vec![TextHorizontal::Center;cols]);}
                for merge in &mut table.merged_cells {
                    if index<=merge.row {merge.row+=1;} else if index<merge.row+merge.rows {merge.rows+=1;}
                }
                shape.height+=TABLE_CELL_HEIGHT;
            }
        }
        if let Some((table,row,col))=self.selected_table_cell.filter(|(table,_,_)|*table==id) {
            self.selected_table_cell=Some((table,row+usize::from(row>=index),col));
        }
    }
    pub(super) fn table_insert_column(&mut self,id:u64,index:usize) {
        self.finish_shape_edit(true);
        let Some(shape)=self.doc.shape(id).filter(|s|s.kind==ShapeKind::Table) else{return};
        let Some(table)=shape.table.as_ref() else{return};
        let cols=table.cells.first().map_or(0,Vec::len);
        if cols>=50||table.cells.is_empty(){return;}
        let index=index.min(cols);
        self.checkpoint();
        if let Some(shape)=self.doc.shape_mut(id) {
            if let Some(table)=shape.table.as_mut() {
                for (_,column) in &mut table.formula_cells {if *column>=index {*column+=1;}}
                for filter in &mut table.filters {if filter.column>=index {filter.column+=1;}}
                for row in &mut table.cells {row.insert(index,String::new());}
                if let Some(borders)=table.borders.as_mut(){
                    let default=table_default_border(shape.style.outline.unwrap_or([128,158,210]),shape.style.stroke_width);
                    for edge in &mut borders.horizontal{edge.insert(index,Some(default));}
                    for row in &mut borders.vertical{row.insert(index+1,Some(default));}
                }
                if let Some(fills)=table.cell_fills.as_mut(){for row in fills{row.insert(index,None);}}
                if let Some(colors)=table.cell_text_colors.as_mut(){for row in colors{row.insert(index,None);}}
                if let Some(alignments)=table.cell_alignments.as_mut(){for row in alignments{row.insert(index,TextHorizontal::Center);}}
                for merge in &mut table.merged_cells {
                    if index<=merge.column {merge.column+=1;} else if index<merge.column+merge.columns {merge.columns+=1;}
                }
                shape.width+=TABLE_CELL_WIDTH;
            }
        }
        if let Some((table,row,col))=self.selected_table_cell.filter(|(table,_,_)|*table==id) {
            self.selected_table_cell=Some((table,row,col+usize::from(col>=index)));
        }
    }
    pub(super) fn table_delete_row(&mut self,id:u64,index:usize) {
        self.finish_shape_edit(true);
        let rows=self.doc.shape(id).and_then(|s|s.table.as_ref()).map_or(0,|t|t.cells.len());
        if rows<=1||index>=rows{return;}
        self.checkpoint();
        if let Some(shape)=self.doc.shape_mut(id) {
            if let Some(table)=shape.table.as_mut(){
                table.merged_cells.clear();
                table.formula_cells.retain(|(row,_)|*row!=index);
                for (row,_) in &mut table.formula_cells {if *row>index {*row-=1;}}
                table.manually_hidden_rows.retain(|row|*row!=index);
                for row in &mut table.manually_hidden_rows {if *row>index {*row-=1;}}
                let removed_height=table.row_height(index);
                table.cells.remove(index);
                if !table.row_heights.is_empty(){table.row_heights.remove(index);}
                if let Some(borders)=table.borders.as_mut(){borders.horizontal.remove(index+1);borders.vertical.remove(index);}
                if let Some(fills)=table.cell_fills.as_mut(){fills.remove(index);}
                if let Some(colors)=table.cell_text_colors.as_mut(){colors.remove(index);}
                if let Some(alignments)=table.cell_alignments.as_mut(){alignments.remove(index);}
                shape.height-=removed_height;
            }
        }
        if let Some((table,row,col))=self.selected_table_cell.filter(|(table,_,_)|*table==id) {
            let next=if row>index {row-1}else{row.min(rows-2)};
            self.selected_table_cell=Some((table,next,col));
        }
    }
    pub(super) fn table_delete_column(&mut self,id:u64,index:usize) {
        self.finish_shape_edit(true);
        let cols=self.doc.shape(id).and_then(|s|s.table.as_ref()).and_then(|t|t.cells.first()).map_or(0,Vec::len);
        if cols<=1||index>=cols{return;}
        self.checkpoint();
        if let Some(shape)=self.doc.shape_mut(id) {
            if let Some(table)=shape.table.as_mut(){
                table.merged_cells.clear();
                table.formula_cells.retain(|(_,column)|*column!=index);
                for (_,column) in &mut table.formula_cells {if *column>index {*column-=1;}}
                table.filters.retain(|filter|filter.column!=index);
                for filter in &mut table.filters {if filter.column>index {filter.column-=1;}}
                for row in &mut table.cells {row.remove(index);}
                if let Some(borders)=table.borders.as_mut(){for edge in &mut borders.horizontal{edge.remove(index);}for row in &mut borders.vertical{row.remove(index+1);}}
                if let Some(fills)=table.cell_fills.as_mut(){for row in fills{row.remove(index);}}
                if let Some(colors)=table.cell_text_colors.as_mut(){for row in colors{row.remove(index);}}
                if let Some(alignments)=table.cell_alignments.as_mut(){for row in alignments{row.remove(index);}}
                shape.width-=TABLE_CELL_WIDTH;
            }
        }
        if let Some((table,row,col))=self.selected_table_cell.filter(|(table,_,_)|*table==id) {
            let next=if col>index {col-1}else{col.min(cols-2)};
            self.selected_table_cell=Some((table,row,next));
        }
    }
    pub(super) fn set_selected_table_alignment(&mut self,alignment:TextHorizontal) {
        let Some((id,row,col))=self.selected_table_cell else{return};
        if self.selected_shape!=Some(id){return;}
        self.finish_shape_edit(true);
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        if row>=table.cells.len()||col>=table.cells[0].len(){return;}
        let current=table.cell_alignments.as_ref().and_then(|a|a.get(row)).and_then(|a|a.get(col))
            .copied().unwrap_or(TextHorizontal::Center);
        if current==alignment{return;}
        self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {
            let rows=table.cells.len();let cols=table.cells[0].len();
            let alignments=table.cell_alignments.get_or_insert_with(||vec![vec![TextHorizontal::Center;cols];rows]);
            alignments[row][col]=alignment;
        }
    }
    pub(super) fn set_selected_table_fill(&mut self,color:Option<[u8;3]>) {
        let Some((id,r0,c0,r1,c1))=self.selected_table_range.filter(|r|self.selected_shape==Some(r.0)) else{return};
        self.finish_shape_edit(true);
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
        let (top,bottom,left,right)=(r0.min(r1).min(rows-1),r0.max(r1).min(rows-1),c0.min(c1).min(cols-1),c0.max(c1).min(cols-1));
        if table.cell_fills.as_ref().is_some_and(|fills|(top..=bottom).all(|r|(left..=right).all(|c|fills[r][c]==color))){return;}
        self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {
            let fills=table.cell_fills.get_or_insert_with(||vec![vec![None;cols];rows]);
            for row in top..=bottom{for col in left..=right{fills[row][col]=color;}}
        }
    }
    pub(super) fn set_selected_table_text_color(&mut self,color:Option<[u8;3]>) {
        let Some((id,r0,c0,r1,c1))=self.selected_table_range.filter(|r|self.selected_shape==Some(r.0)) else{return};
        self.finish_shape_edit(true);
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
        let (top,bottom,left,right)=(r0.min(r1).min(rows-1),r0.max(r1).min(rows-1),c0.min(c1).min(cols-1),c0.max(c1).min(cols-1));
        if table.cell_text_colors.as_ref().is_some_and(|colors|(top..=bottom).all(|r|(left..=right).all(|c|colors[r][c]==color))){return;}
        self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {
            let colors=table.cell_text_colors.get_or_insert_with(||vec![vec![None;cols];rows]);
            for row in top..=bottom{for col in left..=right{colors[row][col]=color;}}
        }
    }
    pub(super) fn toggle_table_header(&mut self) {
        let Some(id)=self.selected_shape.filter(|id|self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table)) else{return};
        self.finish_shape_edit(true);
        self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()){table.header_row=!table.header_row;}
    }
    pub(super) fn toggle_table_banded_rows(&mut self) {
        let Some(id)=self.selected_shape.filter(|id|self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table)) else{return};
        self.finish_shape_edit(true);
        self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()){table.banded_rows=!table.banded_rows;}
    }
    pub(super) fn open_table_filter_popup(&mut self,ctx:&egui::Context,id:u64,column:usize) {
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        if table.cells.first().is_none_or(|row|column>=row.len()){return;}
        let values:HashSet<String>=table.cells.iter().map(|row|row.get(column).cloned().unwrap_or_default()).collect();
        let existing=table.filters.iter().find(|filter|filter.column==column);
        let included=existing.and_then(|filter|match &filter.criterion{
            TableFilterCriterion::Values{included}=>Some(included.iter().cloned().collect()),
            TableFilterCriterion::Condition{..}=>None,
        }).unwrap_or(values);
        let condition=existing.and_then(|filter|match &filter.criterion{
            TableFilterCriterion::Condition{operator,value}=>Some((*operator,value.clone())),
            TableFilterCriterion::Values{..}=>None,
        });
        let pos=ctx.input(|i|i.pointer.hover_pos()).unwrap_or(Pos2::new(24.0,80.0));
        self.table_filter_popup=Some((id,column,pos+Vec2::new(8.0,8.0),included,condition));
    }
    pub(super) fn apply_table_filter_values(&mut self,id:u64,column:usize,included:HashSet<String>) {
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        let values:HashSet<String>=table.cells.iter().map(|row|row.get(column).cloned().unwrap_or_default()).collect();
        let all=included==values;
        let existing=table.filters.iter().find(|filter|filter.column==column).map(|filter|filter.criterion.clone());
        let desired=(!all).then(||TableFilterCriterion::Values{included:included.into_iter().collect()});
        if existing==desired{return;}
        self.finish_shape_edit(true);self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()){
            table.filters.retain(|filter|filter.column!=column);
            if let Some(criterion)=desired{table.filters.push(TableFilter{column,criterion});}
        }
    }
    pub(super) fn apply_table_filter_condition(&mut self,id:u64,column:usize,operator:TableFilterOperator,value:String) {
        let desired=Some(TableFilterCriterion::Condition{operator,value});
        let existing=self.doc.shape(id).and_then(|s|s.table.as_ref())
            .and_then(|table|table.filters.iter().find(|filter|filter.column==column)).map(|filter|filter.criterion.clone());
        if existing==desired{return;}
        self.finish_shape_edit(true);self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()){
            table.filters.retain(|filter|filter.column!=column);
            if let Some(criterion)=desired{table.filters.push(TableFilter{column,criterion});}
        }
    }
    pub(super) fn clear_table_filters(&mut self,id:u64) {
        if self.doc.shape(id).and_then(|s|s.table.as_ref()).is_none_or(|table|table.filters.is_empty()){return;}
        self.finish_shape_edit(true);self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()){table.filters.clear();}
    }
    pub(super) fn toggle_table_row_hidden(&mut self,id:u64,row:usize) {
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        if row>=table.cells.len(){return;}
        let hidden=table.manually_hidden_rows.contains(&row);
        self.finish_shape_edit(true);self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {
            if hidden{table.manually_hidden_rows.retain(|hidden_row|*hidden_row!=row);}
            else{table.manually_hidden_rows.push(row);}
        }
    }
    pub(super) fn expand_table_row_for_newline(&mut self,id:u64,row:usize) {
        let Some((_,target_height))=self.table_row_target_height(id,row,Some((row,self.editing_table_cell.map_or(0,|(_,col)|col),self.shape_edit_text.clone()))) else{return};
        let Some(current_height)=self.doc.shape(id).and_then(|s|s.table.as_ref()).map(|t|t.row_height(row)) else{return};
        if (target_height-current_height).abs()<0.01{return;}
        self.checkpoint();
        if let Some(shape)=self.doc.shape_mut(id) {if let Some(table)=shape.table.as_mut(){
            if table.row_heights.is_empty(){table.row_heights=vec![TABLE_CELL_HEIGHT;table.cells.len()];}
            let delta=target_height-table.row_heights[row];table.row_heights[row]=target_height;shape.height+=delta;
        }}
    }
    pub(super) fn merge_selected_table_cells(&mut self) {
        let Some((id,r0,c0,r1,c1))=self.selected_table_range.filter(|r|self.selected_shape==Some(r.0)) else{return};
        let (top,bottom,left,right)=(r0.min(r1),r0.max(r1),c0.min(c1),c0.max(c1));
        if top==bottom&&left==right{return;}
        let Some(table)=self.doc.shape(id).and_then(|s|s.table.as_ref()) else{return};
        if (top..=bottom).any(|r|(left..=right).any(|c|table.merge_at(r,c).is_some())){return;}
        self.finish_shape_edit(true);self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {
            for r in top..=bottom {for c in left..=right {if r!=top||c!=left {
                table.cells[r][c].clear();
                table.formula_cells.retain(|coord|*coord!=(r,c));
            }}}
            table.merged_cells.push(TableMerge{row:top,column:left,rows:bottom-top+1,columns:right-left+1});
        }
        self.selected_table_cell=Some((id,top,left));self.selected_table_range=Some((id,top,left,bottom,right));
    }
    pub(super) fn unmerge_selected_table_cells(&mut self) {
        let Some((id,row,col))=self.selected_table_cell.filter(|(id,_,_)|self.selected_shape==Some(*id)) else{return};
        let Some(merge)=self.doc.shape(id).and_then(|s|s.table.as_ref()).and_then(|t|t.merge_at(row,col)) else{return};
        self.finish_shape_edit(true);self.checkpoint();
        if let Some(table)=self.doc.shape_mut(id).and_then(|s|s.table.as_mut()) {table.merged_cells.retain(|m|*m!=merge);}
        self.selected_table_range=Some((id,merge.row,merge.column,merge.row+merge.rows-1,merge.column+merge.columns-1));
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
        if !s.visible || s.locked || s.kind==ShapeKind::Table || !self.canvas_selection.contains(&CanvasTarget::Shape(id)) { return None; }
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
            Self::paint_shape_kind(painter,s.kind,r,fill,if s.kind==ShapeKind::Table{Stroke::NONE}else{stroke});
            if s.kind==ShapeKind::Table {
                if let Some(table)=s.table.as_ref() {
                    let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
                    if rows>0&&cols>0 {
                        for row in 0..rows {
                            for col in 0..cols {
                                if table.merge_at(row,col).is_some_and(|m|m.row!=row||m.column!=col){continue;}
                                let merge=table.merge_at(row,col);
                                let span_rows=merge.map_or(1,|m|m.rows);let span_cols=merge.map_or(1,|m|m.columns);
                                let cell=Rect::from_min_max(
                                    egui::pos2(r.left()+r.width()*col as f32/cols as f32,r.top()+self.zoom*table.row_offset(row)),
                                    egui::pos2(r.left()+r.width()*(col+span_cols) as f32/cols as f32,r.top()+self.zoom*(table.row_offset(row)+((row..row+span_rows).map(|i|table.row_height(i)).sum::<f32>()))));
                                let hidden_row=table.manually_hidden_rows.contains(&row)||table.row_is_filtered_out(row);
                                let cell_fill=if hidden_row{if self.dark{Color32::from_gray(52)}else{Color32::from_gray(232)}}else{table.cell_fills.as_ref().and_then(|fills|fills.get(row)).and_then(|r|r.get(col)).copied().flatten().map(color_from_rgb)
                                    .unwrap_or_else(||if table.header_row&&row==0 {if self.dark{Color32::from_rgb(65,76,103)}else{Color32::from_rgb(218,229,252)}}
                                        else if table.banded_rows&&row%2==1 {if self.dark{Color32::from_rgb(45,51,66)}else{Color32::from_rgb(241,244,250)}}else{fill})};
                                painter.rect_filled(cell,0.0,cell_fill);
                                if hidden_row{continue;}
                                if self.editing_shape==Some(s.id)&&self.editing_table_cell.is_some_and(|(er,ec)|table.merge_at(er,ec).map_or((er,ec),|m|(m.row,m.column))==(row,col)){continue;}
                                let align=table.cell_alignments.as_ref().and_then(|a|a.get(row)).and_then(|a|a.get(col))
                                    .copied().unwrap_or(TextHorizontal::Center);
                                let anchor=match align {TextHorizontal::Left=>Align2::LEFT_CENTER,TextHorizontal::Center=>Align2::CENTER_CENTER,TextHorizontal::Right=>Align2::RIGHT_CENTER};
                                let position=match align {TextHorizontal::Left=>cell.left_center()+egui::vec2(4.0*self.zoom,0.0),TextHorizontal::Center=>cell.center(),TextHorizontal::Right=>cell.right_center()-egui::vec2(4.0*self.zoom,0.0)};
                                let text_color=table.cell_text_colors.as_ref().and_then(|colors|colors.get(row)).and_then(|r|r.get(col)).copied().flatten().unwrap_or(s.style.text);
                                let shown=self.spreadsheet_cache.display_value_at(s.id,row,col)
                                    .map(|value|value.display_text()).unwrap_or_else(||display_literal(&table.cells[row][col]));
                                painter.with_clip_rect(cell.shrink(2.0)).text(position,anchor,shown,FontId::proportional(s.style.text_size*self.zoom),color_from_rgb(text_color));
                            }
                        }
                        let default_border=table_default_border(s.style.outline.unwrap_or(if self.dark{[145,145,145]}else{[155,155,155]}),s.style.stroke_width);
                        for boundary in 0..=rows {for col in 0..cols {
                            if table.merged_cells.iter().any(|m|boundary>m.row&&boundary<m.row+m.rows&&col>=m.column&&col<m.column+m.columns){continue;}
                            let border=if let Some(grid)=&table.borders{grid.horizontal[boundary][col]}else{Some(default_border)};
                            if let Some(border)=border {let x0=r.left()+r.width()*col as f32/cols as f32;let x1=r.left()+r.width()*(col+1) as f32/cols as f32;let y=r.top()+self.zoom*table.row_offset(boundary);paint_table_border(painter,Pos2::new(x0,y),Pos2::new(x1,y),border,self.zoom);}
                        }}
                        for row in 0..rows {for boundary in 0..=cols {
                            if table.merged_cells.iter().any(|m|boundary>m.column&&boundary<m.column+m.columns&&row>=m.row&&row<m.row+m.rows){continue;}
                            let border=if let Some(grid)=&table.borders{grid.vertical[row][boundary]}else{Some(default_border)};
                            if let Some(border)=border {let x=r.left()+r.width()*boundary as f32/cols as f32;let y0=r.top()+self.zoom*table.row_offset(row);let y1=y0+self.zoom*table.row_height(row);paint_table_border(painter,Pos2::new(x,y0),Pos2::new(x,y1),border,self.zoom);}
                        }}
                        if let Some((_,row,col))=self.selected_table_cell.filter(|(id,_,_)|*id==s.id) {
                            if row<rows&&col<cols {
                                let merge=table.merge_at(row,col);
                                let (mr,mc,mrows,mcols)=merge.map_or((row,col,1,1),|m|(m.row,m.column,m.rows,m.columns));
                                let cell=Rect::from_min_max(
                                    egui::pos2(r.left()+r.width()*mc as f32/cols as f32,r.top()+self.zoom*table.row_offset(mr)),
                                    egui::pos2(r.left()+r.width()*(mc+mcols) as f32/cols as f32,r.top()+self.zoom*(table.row_offset(mr)+(mr..mr+mrows).map(|i|table.row_height(i)).sum::<f32>())));
                                painter.rect_stroke(cell.shrink(1.0),0.0,Stroke::new(1.6,ACCENT),StrokeKind::Inside);
                                if self.spreadsheet_cache.error_at(s.id,row,col).is_some(){
                                    painter.circle_filled(cell.right_top()-egui::vec2(5.0,5.0),2.5,Color32::from_rgb(214,64,64));
                                }
                            }
                        }
                        if let Some((_,r0,c0,r1,c1))=self.selected_table_range.filter(|(id,_,_,_,_)|*id==s.id) {
                            if r0<rows&&r1<rows&&c0<cols&&c1<cols {
                                let first=r0.min(r1);let last=r0.max(r1);
                                let area=Rect::from_min_max(egui::pos2(r.left()+r.width()*c0.min(c1) as f32/cols as f32,r.top()+self.zoom*table.row_offset(first)),egui::pos2(r.left()+r.width()*(c0.max(c1)+1) as f32/cols as f32,r.top()+self.zoom*(table.row_offset(last)+table.row_height(last))));
                                painter.rect_stroke(area.shrink(1.0),0.0,Stroke::new(1.6,ACCENT),StrokeKind::Inside);
                            }
                        }
                        if let Some(reference_rect)=self.formula_reference_rect(s.id,origin) {
                            let color=Color32::from_rgb(39,128,228);
                            painter.rect_stroke(reference_rect.shrink(1.5),0.0,Stroke::new(2.3,color),StrokeKind::Inside);
                            let handle=Rect::from_center_size(reference_rect.right_bottom(),egui::vec2(5.0,5.0));
                            painter.rect_filled(handle,0.0,color);
                        }
                        if let Some(range)=self.selected_table_range_rect(s.id,origin) {
                            let handle=Rect::from_center_size(range.right_bottom(),egui::vec2(7.0,7.0));
                            painter.rect_filled(handle,0.0,if self.dark{Color32::WHITE}else{Color32::from_rgb(32,45,39)});
                            painter.rect_stroke(handle,0.0,Stroke::new(1.0,if self.dark{Color32::BLACK}else{Color32::WHITE}),StrokeKind::Inside);
                        }
                    }
                }
            }
            let chosen=self.canvas_selection.contains(&CanvasTarget::Shape(s.id));
            if chosen {
                painter.rect_stroke(r.expand(3.0),4.0,Stroke::new(1.6,ACCENT),StrokeKind::Outside);
                if s.kind==ShapeKind::Table {
                    // The 10px band outside the table is its move/select
                    // perimeter. Keep it visible as a light frame around all
                    // four edges so it is easy to grab without hitting a cell.
                    let outer=r.expand(10.0);
                    painter.rect_stroke(outer,2.0,Stroke::new(1.2,ACCENT.gamma_multiply(0.72)),StrokeKind::Outside);
                }
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
            if s.kind!=ShapeKind::Table {self.paint_shape_text(painter,r,s);}
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
