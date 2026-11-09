//! Pure geometry and persistence for grid-managed Frame membership.
//! All dimensions are world coordinates; no egui dependency.
use crate::canvas_core::CanvasTarget;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSlot {
    pub target: CanvasTarget,
    pub col: u16,
    pub row: u16,
    #[serde(default = "one")]
    pub cols: u8,
    #[serde(default = "one")]
    pub rows: u8,
}
fn one() -> u8 {1}
impl GridSlot {
    pub fn new(target:CanvasTarget,col:u16,row:u16,cols:u8,rows:u8)->Self {
        Self {target,col,row,cols:cols.clamp(1,8),rows:rows.clamp(1,8)}
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameGrid {
    pub columns: u16,
    pub cell_width: f32,
    pub cell_height: f32,
    pub gap: f32,
    pub padding: f32,
    pub header: f32,
    pub slots: Vec<GridSlot>,
}
impl Default for FrameGrid {
    fn default() -> Self {
        Self {columns:1,cell_width:240.0,cell_height:160.0,gap:12.0,
            padding:16.0,header:44.0,slots:Vec::new()}
    }
}
impl FrameGrid {
    pub fn cell_rect(&self,slot:GridSlot)->(f32,f32,f32,f32) {
        (self.padding+f32::from(slot.col)*(self.cell_width+self.gap),
         self.header+self.padding+f32::from(slot.row)*(self.cell_height+self.gap),
         self.cell_width*f32::from(slot.cols)+self.gap*f32::from(slot.cols.saturating_sub(1)),
         self.cell_height*f32::from(slot.rows)+self.gap*f32::from(slot.rows.saturating_sub(1)))
    }
    pub fn dimensions(&self)->(f32,f32) {
        let cols=self.columns.max(self.slots.iter().map(|s|s.col.saturating_add(u16::from(s.cols))).max().unwrap_or(1)).max(1);
        let rows=self.slots.iter().map(|s|s.row.saturating_add(u16::from(s.rows))).max().unwrap_or(1).max(1);
        ((self.padding*2.0+f32::from(cols)*self.cell_width+f32::from(cols-1)*self.gap).max(280.0),
         (self.header+self.padding*2.0+f32::from(rows)*self.cell_height+f32::from(rows-1)*self.gap).max(220.0))
    }
    pub fn columns_needed(&self)->u16 {
        self.slots.iter().map(|s|s.col.saturating_add(u16::from(s.cols))).max().unwrap_or(1).max(1)
    }
    pub fn occupied(&self,slot:GridSlot)->bool {
        self.slots.iter().any(|s|overlaps(*s,slot))
    }
    fn free_at(&self,target:CanvasTarget,start_col:u16,start_row:u16,cols:u8,rows:u8)->GridSlot {
        let width=self.columns.max(u16::from(cols)).min(16);
        let mut i=u32::from(start_row)*u32::from(width)+u32::from(start_col.min(width-1));
        // Bounded by the document object count, not by all possible world coordinates.
        for _ in 0..50_000 {
            let c=(i%u32::from(width)) as u16;
            let r=(i/u32::from(width)) as u16;
            if c+u16::from(cols)<=width {
                let candidate=GridSlot::new(target,c,r,cols,rows);
                if !self.occupied(candidate) {return candidate;}
            }
            i+=1;
        }
        GridSlot::new(target,0,0,cols,rows)
    }
    /// Keep occupied slots stable whenever possible, pushing only collisions.
    /// When compact=true, fill gaps and trim unused rows and columns.
    pub fn rearrange(&mut self, target:Option<(CanvasTarget,u16,u16,u8,u8)>, compact:bool) {
        let incoming=target.map(|(t,c,r,w,h)|GridSlot::new(t,c,r,w,h));
        let mut old=std::mem::take(&mut self.slots);
        if let Some(s)=incoming {old.retain(|e|e.target!=s.target);}
        old.sort_by_key(|s|(s.row,s.col,s.target_id()));
        if let Some(s)=incoming {
            self.columns=self.columns.max(s.col.saturating_add(u16::from(s.cols))).clamp(1,16);
            // The target claims its exact proposed cell. Other objects slide forward.
            self.slots.push(s);
        }
        if compact {
            // Sort including incoming, then compact from the origin.
            let mut order=old;
            if let Some(s)=incoming {order.push(s);}
            order.sort_by_key(|s|(s.row,s.col,s.target_id()));
            self.slots.clear();
            for s in order {
                let place=self.free_at(s.target,0,0,s.cols,s.rows);
                self.slots.push(place);
            }
            self.columns=self.columns_needed();
        } else {
            for s in old {
                let candidate=GridSlot::new(s.target,s.col,s.row,s.cols,s.rows);
                let placed=if s.col+u16::from(s.cols)<=self.columns && !self.occupied(candidate) {candidate}
                    else {self.free_at(s.target,s.col,s.row,s.cols,s.rows)};
                self.slots.push(placed);
            }
        }
    }
    pub fn remove_and_compact(&mut self,target:CanvasTarget) {
        self.slots.retain(|s|s.target!=target);
        self.rearrange(None,true);
    }
    pub fn add_unplaced(&mut self,target:CanvasTarget) {
        if self.slots.iter().any(|s|s.target==target){return;}
        let place=self.free_at(target,0,0,1,1);
        self.slots.push(place);
    }
    pub fn validate(&self, members:&[CanvasTarget])->Result<(),String>{
        if !(1..=16).contains(&self.columns) || !self.cell_width.is_finite() || !self.cell_height.is_finite()
            || !self.gap.is_finite() || !self.padding.is_finite() || !self.header.is_finite()
            || !(80.0..=1000.0).contains(&self.cell_width) || !(60.0..=1000.0).contains(&self.cell_height)
            || !(0.0..=100.0).contains(&self.gap) || !(0.0..=100.0).contains(&self.padding)
            || !(0.0..=100.0).contains(&self.header) {return Err("Frame格子の設定が不正です".into());}
        let mut targets=HashSet::new();
        for slot in &self.slots {
            if !targets.insert(slot.target) || !members.contains(&slot.target)
                || slot.cols==0 || slot.rows==0 || slot.cols>8 || slot.rows>8
                || slot.col.saturating_add(u16::from(slot.cols))>16 || slot.col.saturating_add(u16::from(slot.cols))>self.columns
                || slot.row>4096 {return Err("Frame格子のセル参照が不正です".into());}
        }
        for (i,a) in self.slots.iter().enumerate() {
            if self.slots.iter().skip(i+1).any(|b|overlaps(*a,*b)) {
                return Err("Frame格子でSticky Noteが重なっています".into());
            }
        }
        Ok(())
    }
}
impl GridSlot {
    fn target_id(&self)->u64 { match self.target {CanvasTarget::Shape(id)|CanvasTarget::Node(id)|CanvasTarget::TextBox(id)|CanvasTarget::Line(id)=>id} }
}
fn overlaps(a:GridSlot,b:GridSlot)->bool {
    a.col < b.col.saturating_add(u16::from(b.cols)) && b.col < a.col.saturating_add(u16::from(a.cols))
    && a.row < b.row.saturating_add(u16::from(b.rows)) && b.row < a.row.saturating_add(u16::from(a.rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(n:u64)->CanvasTarget {CanvasTarget::Shape(n)}
    #[test]
    fn insertion_uses_requested_cell_and_pushes_collisions() {
        let mut g=FrameGrid::default();
        g.add_unplaced(target(1));
        g.rearrange(Some((target(2),0,0,1,1)),false);
        assert_eq!(g.slots.iter().find(|s|s.target==target(2)).unwrap().row,0);
        assert_eq!(g.slots.iter().find(|s|s.target==target(1)).unwrap().row,1);
        g.remove_and_compact(target(2));
        assert_eq!(g.slots[0].row,0);
        assert_eq!(g.dimensions(),(280.0,236.0));
    }
    #[test]
    fn multi_cell_note_and_roundtrip() {
        let mut g=FrameGrid::default();
        g.rearrange(Some((target(1),0,0,2,2)),false);
        assert_eq!(g.columns,2);
        g.rearrange(Some((target(2),0,1,1,1)),false);
        assert!(!g.occupied(GridSlot::new(target(5),0,1,1,1)) ||
           g.slots.iter().any(|s|s.target==target(2)&&s.row==1));
        assert_eq!(g.validate(&[target(1),target(2)]).is_ok(),true);
        let json=serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<FrameGrid>(&json).unwrap(),g);
    }
    #[test]
    fn overlap_is_rejected() {
        let mut g=FrameGrid::default();
        g.columns=2;
        g.slots.push(GridSlot::new(target(1),0,0,2,1));
        g.slots.push(GridSlot::new(target(2),1,0,1,1));
        assert!(g.validate(&[target(1),target(2)]).is_err());
    }
}
