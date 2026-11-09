//! The Frame grid is persisted on Frame shapes; this adapter applies it to
//! live world-space shapes and computes non-mutating drag previews.
use super::*;
use crate::frame_grid::{FrameGrid, GridSlot};

#[derive(Clone)]
pub(super) struct GridDropPreview {
    pub frame: u64,
    pub grid: FrameGrid,
    pub slot: GridSlot,
    pub attaching: bool,
}

impl MindmapApp {
    pub(super) fn sticky_frame(&self,note_id:u64)->Option<u64> {
        self.doc.shapes.iter().find(|f|f.kind==ShapeKind::Frame &&
            f.members.contains(&CanvasTarget::Shape(note_id))).map(|f|f.id)
    }
    /// A legacy v2 frame without grid data acquires a grid on the next edit.
    /// Existing member IDs are retained, but all stickies snap to cells.
    fn populated_frame_grid(&self,frame_id:u64)->Option<FrameGrid> {
        let frame=self.doc.shape(frame_id)?;
        if frame.kind!=ShapeKind::Frame{return None;}
        let mut grid=frame.grid.clone().unwrap_or_default();
        grid.slots.retain(|s|frame.members.contains(&s.target) &&
            matches!(s.target,CanvasTarget::Shape(id) if self.doc.shape(id).is_some_and(|t|t.kind==ShapeKind::StickyNote)));
        // Upgrade old members in document order. Preserve any already occupied cells.
        for member in &frame.members {
            if let CanvasTarget::Shape(id)=*member {
                if self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::StickyNote){grid.add_unplaced(*member);}
            }
        }
        Some(grid)
    }
    fn set_frame_grid(&mut self,frame_id:u64,grid:FrameGrid) {
        let Some(frame)=self.doc.shape(frame_id) else{return;};
        let origin=frame.position;
        let (width,height)=grid.dimensions();
        let positions:Vec<_>=grid.slots.iter().map(|s| {
            let (x,y,w,h)=grid.cell_rect(*s);
            (s.target,origin.x+x,origin.y+y,w,h)
        }).collect();
        if let Some(frame)=self.doc.shape_mut(frame_id) {
            frame.grid=Some(grid);
            frame.width=width;
            frame.height=height;
        }
        for (t,x,y,w,h) in positions {
            if let CanvasTarget::Shape(id)=t {
                if let Some(s)=self.doc.shape_mut(id) {
                    s.position.x=x;s.position.y=y;s.width=w;s.height=h;
                }
            }
        }
    }
    pub(super) fn refresh_frame_grid(&mut self,frame_id:u64) {
        let Some(mut grid)=self.populated_frame_grid(frame_id) else {return;};
        grid.rearrange(None,true);
        self.set_frame_grid(frame_id,grid);
    }
    /// The pointer selects a column and a row (including the first new column
    /// to the right or first new row below). The preview does not change Document.
    pub(super) fn preview_grid_insert(&self,frame_id:u64,note_id:u64,
                                      pointer:Pos2,canvas_origin:Pos2)->Option<GridDropPreview> {
        let frame=self.doc.shape(frame_id)?;
        let note=self.doc.shape(note_id)?;
        if frame.kind!=ShapeKind::Frame || note.kind!=ShapeKind::StickyNote{return None;}
        let mut grid=self.populated_frame_grid(frame_id)?;
        let local=(pointer-canvas_origin)/self.zoom-egui::vec2(frame.position.x,frame.position.y);
        let desired_c=((local.x-grid.padding)/(grid.cell_width+grid.gap)).floor().max(0.0) as u16;
        let desired_r=((local.y-grid.header-grid.padding)/(grid.cell_height+grid.gap)).floor().max(0.0) as u16;
        let incoming=grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(note_id)).copied();
        let original=self.sticky_frame(note_id).and_then(|old|self.doc.shape(old))
            .and_then(|old|old.grid.as_ref())
            .and_then(|g|g.slots.iter().find(|s|s.target==CanvasTarget::Shape(note_id)).copied());
        let width=incoming.or(original).map(|s|s.cols).unwrap_or(1);
        let height=incoming.or(original).map(|s|s.rows).unwrap_or(1);
        // Clamp to bounded grid size while allowing expansion beyond current frame.
        let col=desired_c.min(16-u16::from(width));
        let row=desired_r.min(260);
        grid.rearrange(Some((CanvasTarget::Shape(note_id),col,row,width,height)),false);
        let slot=grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(note_id)).copied()?;
        Some(GridDropPreview{frame:frame_id,grid,slot,attaching:self.sticky_frame(note_id)!=Some(frame_id)})
    }
    /// Applying a preview is a single undoable operation: the drag's checkpoint
    /// was created before any notes moved, so the whole grid restores together.
    pub(super) fn apply_grid_insert(&mut self,frame_id:u64,note_id:u64,
                                    pointer:Pos2,canvas_origin:Pos2) {
        let Some(preview)=self.preview_grid_insert(frame_id,note_id,pointer,canvas_origin) else{return;};
        let target=CanvasTarget::Shape(note_id);
        let previous:Vec<u64>=self.doc.shapes.iter().filter(|f|f.kind==ShapeKind::Frame &&
            f.id!=frame_id && f.members.contains(&target)).map(|f|f.id).collect();
        for id in &previous {
            if let Some(f)=self.doc.shape_mut(*id) {f.members.retain(|&t|t!=target);}
        }
        if let Some(f)=self.doc.shape_mut(frame_id) {
            if !f.members.contains(&target) {f.members.push(target);}
        }
        self.set_frame_grid(frame_id,preview.grid);
        for id in previous {self.refresh_frame_grid(id);}
    }
    /// Explicit context-menu membership operations use the same unique-owner
    /// rule as drag-and-drop. Only sticky notes are constrained by the grid;
    /// other legacy Frame members keep their existing freely positioned form.
    pub(super) fn transfer_stickies_to_frame(&mut self,frame_id:u64,targets:&[CanvasTarget]) {
        let moving:Vec<_>=targets.iter().copied().filter(|t|matches!(t,
            CanvasTarget::Shape(id) if self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::StickyNote))).collect();
        if moving.is_empty(){return;}
        let old:Vec<_>=self.doc.shapes.iter().filter(|s|s.kind==ShapeKind::Frame && s.id!=frame_id
            && s.members.iter().any(|m|moving.contains(m))).map(|s|s.id).collect();
        for &id in &old {
            if let Some(frame)=self.doc.shape_mut(id) {frame.members.retain(|m|!moving.contains(m));}
        }
        for id in old {self.refresh_frame_grid(id);}
    }
    pub(super) fn detach_grid_sticky(&mut self,note_id:u64) {
        let target=CanvasTarget::Shape(note_id);
        let parents:Vec<_>=self.doc.shapes.iter().filter(|f|f.kind==ShapeKind::Frame &&
            f.members.contains(&target)).map(|f|f.id).collect();
        for id in parents {
            if let Some(f)=self.doc.shape_mut(id){f.members.retain(|&t|t!=target);}
            self.refresh_frame_grid(id);
        }
        // Keep the dragged note's current position on the free canvas.
    }
    pub(super) fn frame_grid_shrink_preview(&self,frame_id:u64,note_id:u64)->Option<(f32,f32)> {
        let mut grid=self.populated_frame_grid(frame_id)?;
        grid.remove_and_compact(CanvasTarget::Shape(note_id));
        Some(grid.dimensions())
    }
    /// Snap an attached note's final resize to cell counts (1x1, 2x1, 1x2, 2x2...).
    pub(super) fn finish_grid_sticky_resize(&mut self,note_id:u64) {
        let Some(frame_id)=self.sticky_frame(note_id) else{return;};
        let Some(shape)=self.doc.shape(note_id) else{return;};
        let (w,h)=(shape.width,shape.height);
        let Some(mut grid)=self.populated_frame_grid(frame_id) else{return;};
        let Some(slot)=grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(note_id)).copied() else{return;};
        let cols=((w+grid.gap)/(grid.cell_width+grid.gap)).round().clamp(1.0,8.0) as u8;
        let rows=((h+grid.gap)/(grid.cell_height+grid.gap)).round().clamp(1.0,8.0) as u8;
        grid.rearrange(Some((slot.target,slot.col,slot.row,cols,rows)),false);
        self.set_frame_grid(frame_id,grid);
    }
    /// Resizing a Frame changes the available column count, then reconciles
    /// all occupants to valid cells instead of allowing arbitrary positions.
    pub(super) fn finish_frame_grid_resize(&mut self,frame_id:u64) {
        let Some(frame)=self.doc.shape(frame_id) else{return;};
        if frame.kind!=ShapeKind::Frame{return;}
        let requested=frame.width;
        let Some(mut grid)=self.populated_frame_grid(frame_id) else{return;};
        let cols=((requested-2.0*grid.padding+grid.gap)/(grid.cell_width+grid.gap))
            .round().clamp(1.0,16.0) as u16;
        grid.columns=cols;
        grid.rearrange(None,true);
        grid.columns=grid.columns.max(cols);
        self.set_frame_grid(frame_id,grid);
    }
    pub(super) fn set_grid_sticky_span(&mut self,note_id:u64,cols:u8,rows:u8) {
        let Some(frame_id)=self.sticky_frame(note_id) else{return;};
        let Some(mut grid)=self.populated_frame_grid(frame_id) else{return;};
        let Some(slot)=grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(note_id)).copied() else{return;};
        self.checkpoint();
        grid.rearrange(Some((slot.target,slot.col,slot.row,cols,rows)),false);
        self.set_frame_grid(frame_id,grid);
    }
}

#[cfg(test)]
mod grid_regressions {
    use super::*;
    fn new_app()->MindmapApp {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.zoom=1.0;
        app
    }
    fn new_frame_and_notes(app:&mut MindmapApp)->(u64,u64,u64) {
        let frame=app.doc.add_shape(ShapeKind::Frame,Offset{x:100.0,y:100.0});
        let first=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:50.0,y:60.0});
        let second=app.doc.add_shape(ShapeKind::StickyNote,Offset{x:70.0,y:80.0});
        (frame,first,second)
    }
    #[test]
    fn ghost_matches_insert_and_notes_do_not_overlap() {
        let mut app=new_app();
        let (frame,a,b)=new_frame_and_notes(&mut app);
        let cursor=Pos2::new(165.0,195.0);
        let first_plan=app.preview_grid_insert(frame,a,cursor,Pos2::ZERO).unwrap();
        assert!(app.doc.shape(frame).unwrap().members.is_empty());
        app.apply_grid_insert(frame,a,cursor,Pos2::ZERO);
        let actual=app.doc.shape(a).unwrap();
        let slot=first_plan.grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(a)).unwrap();
        let (x,y,w,h)=first_plan.grid.cell_rect(*slot);
        assert_eq!((actual.position.x,actual.position.y,actual.width,actual.height),(100.0+x,100.0+y,w,h));
        app.apply_grid_insert(frame,b,cursor,Pos2::ZERO);
        let f=app.doc.shape(frame).unwrap();
        let grid=f.grid.as_ref().unwrap();
        assert!(grid.validate(&f.members).is_ok());
        assert_ne!(app.doc.shape(a).unwrap().position,app.doc.shape(b).unwrap().position);
    }
    #[test]
    fn drop_right_creates_column_and_removing_shrinks_frame() {
        let mut app=new_app();
        let (frame,a,b)=new_frame_and_notes(&mut app);
        app.apply_grid_insert(frame,a,Pos2::new(160.0,198.0),Pos2::ZERO);
        let before=app.doc.shape(frame).unwrap().width;
        let right_edge=app.doc.shape(frame).unwrap().position.x+before+10.0;
        let y=app.doc.shape(frame).unwrap().position.y+100.0;
        let preview=app.preview_grid_insert(frame,b,Pos2::new(right_edge,y),Pos2::ZERO).unwrap();
        assert!(preview.grid.dimensions().0>before);
        app.apply_grid_insert(frame,b,Pos2::new(right_edge,y),Pos2::ZERO);
        assert_eq!(app.doc.shape(frame).unwrap().width,preview.grid.dimensions().0);
        app.detach_grid_sticky(b);
        assert_eq!(app.doc.shape(frame).unwrap().width,before);
        assert!(!app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(b)));
    }
    #[test]
    fn adjacent_right_drop_zone_expands_instead_of_rejecting() {
        let mut app=new_app();
        let (frame,sticky,_)=new_frame_and_notes(&mut app);
        let f=app.doc.shape(frame).unwrap();
        let pointer=Pos2::new(f.position.x+f.width+105.0,f.position.y+100.0);
        assert_eq!(app.frame_drop_target_for_note(sticky,pointer,Pos2::ZERO),Some(frame));
        let preview=app.preview_grid_insert(frame,sticky,pointer,Pos2::ZERO).unwrap();
        assert!(preview.grid.dimensions().0>f.width);
        app.update_frame_membership_on_drop(sticky,pointer,Pos2::ZERO);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(sticky)));
    }
    #[test]
    fn spans_persist_and_resize_to_cell_dimensions() {
        let mut app=new_app();
        let (frame,a,_)=new_frame_and_notes(&mut app);
        app.apply_grid_insert(frame,a,Pos2::new(165.0,195.0),Pos2::ZERO);
        app.set_grid_sticky_span(a,2,2);
        let f=app.doc.shape(frame).unwrap();
        let grid=f.grid.as_ref().unwrap();
        let slot=grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(a)).unwrap();
        assert_eq!((slot.cols,slot.rows),(2,2));
        let (_,_,w,h)=grid.cell_rect(*slot);
        assert_eq!((app.doc.shape(a).unwrap().width,app.doc.shape(a).unwrap().height),(w,h));
        let json=serde_json::to_string(&app.doc).unwrap();
        let loaded:crate::mindmap_core::Document=serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.shape(frame).unwrap().grid.as_ref().unwrap().slots[0].cols,2);
    }
    #[test]
    fn drag_frame_moves_sticky_without_breaking_grid_local_coordinates() {
        let mut app=new_app();
        let (frame,a,_)=new_frame_and_notes(&mut app);
        app.apply_grid_insert(frame,a,Pos2::new(150.0,190.0),Pos2::ZERO);
        let before=app.doc.shape(a).unwrap().position;
        app.canvas_selection.clear();app.canvas_selection.insert(CanvasTarget::Shape(frame));
        app.move_canvas_selection(egui::vec2(40.0,25.0));
        assert_eq!(app.doc.shape(a).unwrap().position.x,before.x+40.0);
        assert_eq!(app.doc.shape(a).unwrap().position.y,before.y+25.0);
        let grid=app.doc.shape(frame).unwrap().grid.as_ref().unwrap();
        assert!(grid.validate(&app.doc.shape(frame).unwrap().members).is_ok());
    }
    #[test]
    fn transfer_between_frames_has_one_owner_and_is_undoable() {
        let mut app=new_app();
        let (frame,sticky,_)=new_frame_and_notes(&mut app);
        let other=app.doc.add_shape(ShapeKind::Frame,Offset{x:620.0,y:100.0});
        app.apply_grid_insert(frame,sticky,Pos2::new(156.0,195.0),Pos2::ZERO);
        let first=app.doc.shape(sticky).unwrap().position;
        app.checkpoint();
        app.apply_grid_insert(other,sticky,Pos2::new(660.0,195.0),Pos2::ZERO);
        assert!(!app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(sticky)));
        assert!(app.doc.shape(other).unwrap().members.contains(&CanvasTarget::Shape(sticky)));
        assert!(app.doc.validate().is_ok());
        app.undo();
        assert_eq!(app.doc.shape(sticky).unwrap().position,first);
        assert!(app.doc.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(sticky)));
        assert!(!app.doc.shape(other).unwrap().members.contains(&CanvasTarget::Shape(sticky)));
    }
    #[test]
    fn deleting_member_compacts_grid_without_dangling_reference() {
        let mut app=new_app();
        let (frame,a,b)=new_frame_and_notes(&mut app);
        app.apply_grid_insert(frame,a,Pos2::new(160.0,195.0),Pos2::ZERO);
        app.apply_grid_insert(frame,b,Pos2::new(390.0,195.0),Pos2::ZERO);
        let before=app.doc.shape(frame).unwrap().width;
        assert!(app.doc.delete_shape(b));
        assert!(app.doc.shape(frame).unwrap().width<before);
        assert!(app.doc.validate().is_ok());
    }
    #[test]
    fn resize_handle_snaps_sticky_to_two_cell_span() {
        let mut app=new_app();
        let (frame,a,_)=new_frame_and_notes(&mut app);
        app.apply_grid_insert(frame,a,Pos2::new(155.0,195.0),Pos2::ZERO);
        app.doc.shape_mut(a).unwrap().width=502.0;
        app.finish_grid_sticky_resize(a);
        let grid=app.doc.shape(frame).unwrap().grid.as_ref().unwrap();
        assert_eq!(grid.slots.iter().find(|s|s.target==CanvasTarget::Shape(a)).unwrap().cols,2);
        assert_eq!(app.doc.shape(a).unwrap().width,492.0);
    }

    #[test]
    fn duplicate_frame_and_sticky_remaps_grid_references() {
        let mut app=new_app();
        let (frame,sticky,_)=new_frame_and_notes(&mut app);
        app.apply_grid_insert(frame,sticky,Pos2::new(150.0,180.0),Pos2::ZERO);
        app.canvas_selection=std::collections::HashSet::from([
            CanvasTarget::Shape(frame),CanvasTarget::Shape(sticky)]);
        app.duplicate_canvas_selection();
        assert!(app.doc.validate().is_ok());
        let new_frame=app.doc.shapes.iter().find(|s|s.kind==ShapeKind::Frame && s.id!=frame).unwrap();
        assert_eq!(new_frame.grid.as_ref().unwrap().slots.len(),1);
        let target=new_frame.grid.as_ref().unwrap().slots[0].target;
        assert_ne!(target,CanvasTarget::Shape(sticky));
        assert!(new_frame.members.contains(&target));
    }

}
