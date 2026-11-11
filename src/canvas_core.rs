//! Shared, GUI-independent canvas object and connector data (document schema v2).
//! Mindmap nodes and older text annotations participate through `CanvasTarget` adapters.
use crate::mindmap_core::Offset;
use crate::frame_grid::FrameGrid;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum CanvasTarget {
    Node(u64),
    TextBox(u64),
    Shape(u64),
    Line(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind { Rectangle, RoundedRectangle, Ellipse, Diamond, Parallelogram, Cylinder, Hexagon, Document, Text, StickyNote, Frame, Table }

/// Table cells follow a fixed logical size, approximating a common spreadsheet's default
/// 8.43-character column and 15-point row at 100% display scale.
pub const TABLE_CELL_WIDTH: f32 = 84.0;
pub const TABLE_CELL_HEIGHT: f32 = 24.0;
pub const TABLE_INITIAL_ROWS: usize = 3;
pub const TABLE_INITIAL_COLUMNS: usize = 3;

/// Cell contents for a canvas table. Rows are stored in document order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableData {
    #[serde(default = "default_table_cells")]
    pub cells: Vec<Vec<String>>,
    /// Coordinates of cells whose stored source text is a formula. Empty or
    /// missing metadata keeps legacy strings such as `=SUM(A1:A2)` literal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formula_cells: Vec<(usize, usize)>,
    /// Row indices explicitly hidden by the user. Filtered rows are derived
    /// from saved criteria rather than persisted as a second state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manually_hidden_rows: Vec<usize>,
    /// Saved criteria used to derive rows excluded by filters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<TableFilter>,
    #[serde(default)]
    pub header_row: bool,
    #[serde(default)]
    pub banded_rows: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_alignments: Option<Vec<Vec<TextHorizontal>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged_cells: Vec<TableMerge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_heights: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<TableBorders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_fills: Option<Vec<Vec<Option<[u8;3]>>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_text_colors: Option<Vec<Vec<Option<[u8;3]>>>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableBorders {
    pub horizontal: Vec<Vec<Option<TableBorder>>>,
    pub vertical: Vec<Vec<Option<TableBorder>>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TableBorder {
    pub color: [u8;3],
    pub width: f32,
    pub dash: TableBorderDash,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all="snake_case")]
pub enum TableBorderDash { #[default] Solid, Dashed, Dotted }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMerge {
    pub row: usize,
    pub column: usize,
    pub rows: usize,
    pub columns: usize,
}

/// One active filter per table column.
#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
pub struct TableFilter {
    pub column: usize,
    pub criterion: TableFilterCriterion,
}

#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TableFilterCriterion {
    /// Keep only cells whose raw value is included in this list.
    Values { included: Vec<String> },
    /// Compare cell values to a user-supplied criterion.
    Condition { operator: TableFilterOperator, value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableFilterOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
}

fn default_table_cells() -> Vec<Vec<String>> {
    vec![vec![String::new(); TABLE_INITIAL_COLUMNS]; TABLE_INITIAL_ROWS]
}
impl Default for TableData {
    fn default() -> Self { Self { cells: default_table_cells(), formula_cells:Vec::new(), manually_hidden_rows:Vec::new(), filters:Vec::new(), header_row:false, banded_rows:false, cell_alignments:None, merged_cells:Vec::new(), row_heights:Vec::new(),borders:None,cell_fills:None,cell_text_colors:None } }
}
impl TableData {
    pub fn row_height(&self,row:usize)->f32 {self.row_heights.get(row).copied().unwrap_or(TABLE_CELL_HEIGHT)}
    pub fn row_offset(&self,row:usize)->f32 {(0..row).map(|i|self.row_height(i)).sum()}
    pub fn total_height(&self)->f32 {(0..self.cells.len()).map(|i|self.row_height(i)).sum()}
    pub fn merge_at(&self,row:usize,column:usize)->Option<TableMerge> {
        self.merged_cells.iter().copied().find(|merge|
            merge.row.checked_add(merge.rows).is_some_and(|end|row>=merge.row&&row<end)
                && merge.column.checked_add(merge.columns).is_some_and(|end|column>=merge.column&&column<end))
    }
    pub fn row_is_filtered_out(&self,row:usize)->bool {
        let Some(cells)=self.cells.get(row) else{return false};
        self.filters.iter().any(|filter|{
            let value=cells.get(filter.column).map(String::as_str).unwrap_or("");
            match &filter.criterion {
                TableFilterCriterion::Values{included}=>!included.iter().any(|candidate|candidate==value),
                TableFilterCriterion::Condition{operator,value:condition}=>{
                    let ordering=value.trim().parse::<f64>().ok().zip(condition.trim().parse::<f64>().ok())
                        .map(|(left,right)|left.partial_cmp(&right));
                    let equal=ordering.flatten().map_or_else(||value.eq_ignore_ascii_case(condition),|order|order.is_eq());
                    let matches=match operator {
                        TableFilterOperator::Equal=>equal,
                        TableFilterOperator::NotEqual=>!equal,
                        TableFilterOperator::GreaterThan=>ordering.flatten().map_or_else(||value.to_lowercase()>condition.to_lowercase(),|order|order.is_gt()),
                        TableFilterOperator::GreaterThanOrEqual=>ordering.flatten().map_or_else(||value.to_lowercase()>=condition.to_lowercase(),|order|!order.is_lt()),
                        TableFilterOperator::LessThan=>ordering.flatten().map_or_else(||value.to_lowercase()<condition.to_lowercase(),|order|order.is_lt()),
                        TableFilterOperator::LessThanOrEqual=>ordering.flatten().map_or_else(||value.to_lowercase()<=condition.to_lowercase(),|order|!order.is_gt()),
                        TableFilterOperator::Contains=>value.to_lowercase().contains(&condition.to_lowercase()),
                        TableFilterOperator::StartsWith=>value.to_lowercase().starts_with(&condition.to_lowercase()),
                        TableFilterOperator::EndsWith=>value.to_lowercase().ends_with(&condition.to_lowercase()),
                    };
                    !matches
                }
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor { Top, Right, Bottom, Left }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all="snake_case")]
pub enum ConnectorRoute { #[default] Straight, Orthogonal, Polyline }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all="snake_case")]
pub enum ConnectorMode { #[default] Auto, Manual }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all="snake_case")]
pub enum ConnectorDash { #[default] Solid, Dashed, Dotted }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all="snake_case")]
pub enum TextHorizontal { Left, #[default] Center, Right }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all="snake_case")]
pub enum TextVertical { Top, #[default] Center, Bottom }


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasStyle {
    pub fill: Option<[u8; 3]>,
    pub outline: Option<[u8; 3]>,
    pub text: [u8; 3],
    pub stroke_width: f32,
    #[serde(default)]
    pub text_horizontal: TextHorizontal,
    #[serde(default)]
    pub text_vertical: TextVertical,
    #[serde(default = "default_text_size")]
    pub text_size: f32,
}
fn default_text_size()->f32 {15.0}
impl Default for CanvasStyle {
    fn default() -> Self {
        Self { fill: Some([234, 241, 252]), outline: Some([128, 158, 210]),
               text: [38, 52, 75], stroke_width: 1.5,
               text_horizontal:TextHorizontal::Center, text_vertical:TextVertical::Center, text_size:15.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasGroup {
    pub id: u64,
    pub members: Vec<CanvasTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasShape {
    pub id: u64,
    pub kind: ShapeKind,
    pub position: Offset,
    pub width: f32,
    pub height: f32,
    pub z: i32,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub style: CanvasStyle,
    #[serde(default)]
    pub text: String,
    /// Explicit Frame membership. Visual containment never implies membership.
    #[serde(default, skip_serializing_if="Vec::is_empty")]
    pub members: Vec<CanvasTarget>,
    /// Occupied cells for grid-managed Frame objects; omitted by legacy JSON.
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub grid: Option<FrameGrid>,
    /// Present only for `ShapeKind::Table`; optional for compatibility with old documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableData>,
}
/// A free-standing world-space line, never attached to another object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasLine {
    pub id: u64,
    pub start: Offset,
    pub end: Offset,
    pub z: i32,
    #[serde(default = "connector_color")]
    pub color: [u8; 3],
    #[serde(default = "connector_width")]
    pub width: f32,
    #[serde(default)]
    pub dashed: bool,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
}
impl CanvasLine {
    #[allow(dead_code)]
    pub fn bounds(&self) -> (Offset, f32, f32) {
        (Offset { x:self.start.x.min(self.end.x), y:self.start.y.min(self.end.y) },
         (self.start.x-self.end.x).abs(), (self.start.y-self.end.y).abs())
    }
    pub fn move_by(&mut self, dx:f32, dy:f32) {
        self.start.x+=dx;self.start.y+=dy;self.end.x+=dx;self.end.y+=dy;
    }
}

fn yes() -> bool { true }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectorEnd { pub target: CanvasTarget, pub anchor: Anchor }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasConnector {
    pub id: u64,
    pub from: ConnectorEnd,
    pub to: ConnectorEnd,
    #[serde(default = "connector_color")]
    pub color: [u8; 3],
    #[serde(default = "connector_width")]
    pub width: f32,
    #[serde(default)]
    /// Legacy v0.7/v0.8 end-arrow flag. Preserved for file compatibility.
    pub arrow: bool,
    #[serde(default)]
    pub arrow_start: bool,
    #[serde(default)]
    pub route: ConnectorRoute,
    #[serde(default)]
    pub mode: ConnectorMode,
    #[serde(default)]
    pub waypoints: Vec<Offset>,
    #[serde(default)]
    pub dash: ConnectorDash,
    #[serde(default)]
    pub label: String,
    #[serde(default = "label_visible")]
    pub label_visible: bool,
    #[serde(default = "label_middle")]
    pub label_fraction: f32,
    #[serde(default)]
    pub label_offset: Offset,
    #[serde(default = "default_text_size")]
    pub label_size: f32,
    #[serde(default = "connector_color")]
    pub label_color: [u8;3],
    #[serde(default)]
    pub label_background: Option<[u8;3]>,
}
fn label_visible()->bool {true}
fn label_middle()->f32 {0.5}
impl CanvasConnector {
    pub fn new(id:u64, from:ConnectorEnd, to:ConnectorEnd, arrow:bool)->Self {
        Self {id,from,to,arrow,color:connector_color(),width:connector_width(),
            arrow_start:false,route:ConnectorRoute::Straight,mode:ConnectorMode::Auto,
            waypoints:Vec::new(),dash:ConnectorDash::Solid,label:String::new(),
            label_visible:true,label_fraction:0.5,label_offset:Offset::default(),
            label_size:15.0,label_color:[48,59,82],label_background:None}
    }
}
fn connector_color() -> [u8; 3] { [109, 128, 162] }
fn connector_width() -> f32 { 2.0 }

pub fn validate_shapes(shapes: &[CanvasShape], lines: &[CanvasLine], connectors: &[CanvasConnector],
                       node_ids: &HashSet<u64>, text_ids: &HashSet<u64>, next_id: u64)
                       -> Result<(), String> {
    let ids: HashSet<u64> = shapes.iter().map(|s| s.id).collect();
    if ids.len() != shapes.len() { return Err("CanvasオブジェクトのIDが重複しています".into()); }
    if ids.iter().any(|id| *id >= next_id) { return Err("next_canvas_idが不正です".into()); }
    for s in shapes {
        if !s.position.x.is_finite() || !s.position.y.is_finite() || !s.width.is_finite()
            || !s.height.is_finite() || !s.style.stroke_width.is_finite()
            || !s.style.text_size.is_finite() || !(7.0..=120.0).contains(&s.style.text_size)
            || !(32.0..=50_000.0).contains(&s.width)
            || !(24.0..=50_000.0).contains(&s.height)
            || !(0.0..=100.0).contains(&s.style.stroke_width) {
            return Err(format!("Canvasオブジェクト {} の座標・サイズが不正です", s.id));
        }
    }
    let mut line_ids=HashSet::new();
    for line in lines {
        if !line_ids.insert(line.id) || line.id>=next_id ||
           ![line.start.x,line.start.y,line.end.x,line.end.y,line.width].iter().all(|v|v.is_finite()) ||
           !(0.25..=32.0).contains(&line.width) ||
           (line.start.x-line.end.x).hypot(line.start.y-line.end.y)<0.01 {
            return Err("自由直LinesのID・座標・幅が不正です".into());
        }
    }
    if !line_ids.is_disjoint(&ids) {return Err("Shapesと直LinesのIDが重複しています".into());}
    for shape in shapes {
        if shape.kind!=ShapeKind::Frame && !shape.members.is_empty() {return Err("Frame以外の所属情報は不正です".into());}
        let mut members=HashSet::new();
        if let Some(grid)=&shape.grid {
            if shape.kind!=ShapeKind::Frame {return Err("Frame以外に格子データが設定されています".into());}
            grid.validate(&shape.members)?;
        }
        match (&shape.kind,&shape.table) {
            (ShapeKind::Table,Some(table)) => {
                let cols=table.cells.first().map_or(0,Vec::len);
                if table.cells.is_empty()||table.cells.len()>50||cols==0||cols>50
                    || table.cells.iter().any(|row|row.len()!=cols)
                    || table.cells.iter().flatten().any(|cell|cell.len()>16_384)
                    || table.formula_cells.iter().enumerate().any(|(index,(row,col))| {
                        *row>=table.cells.len()||*col>=cols
                            ||table.formula_cells.iter().skip(index+1).any(|other|other==&(*row,*col))
                    })
                    || table.manually_hidden_rows.iter().enumerate().any(|(index,row)| {
                        *row>=table.cells.len()||table.manually_hidden_rows.iter().skip(index+1).any(|other|other==row)
                    })
                    || table.filters.len()>cols
                    || table.filters.iter().enumerate().any(|(index,filter)| {
                        filter.column>=cols||table.filters.iter().skip(index+1).any(|other|other.column==filter.column)
                            ||match &filter.criterion {
                                TableFilterCriterion::Values{included}=>included.len()>2500
                                    ||included.iter().any(|value|value.len()>16_384)
                                    ||included.iter().enumerate().any(|(i,value)|included.iter().skip(i+1).any(|other|other==value)),
                                TableFilterCriterion::Condition{value,..}=>value.len()>16_384,
                            }
                    })
                    || (shape.width-cols as f32*TABLE_CELL_WIDTH).abs()>0.01
                    || (!table.row_heights.is_empty()&&(table.row_heights.len()!=table.cells.len()||table.row_heights.iter().any(|height|!height.is_finite()||*height<TABLE_CELL_HEIGHT)))
                    || (shape.height-table.total_height()).abs()>0.01
                    || table.cell_alignments.as_ref().is_some_and(|alignments|
                        alignments.len()!=table.cells.len()||alignments.iter().any(|row|row.len()!=cols))
                    || table.cell_fills.as_ref().is_some_and(|fills|fills.len()!=table.cells.len()||fills.iter().any(|row|row.len()!=cols))
                    || table.cell_text_colors.as_ref().is_some_and(|colors|colors.len()!=table.cells.len()||colors.iter().any(|row|row.len()!=cols))
                    || table.borders.as_ref().is_some_and(|borders|
                        borders.horizontal.len()!=table.cells.len()+1
                            ||borders.horizontal.iter().any(|row|row.len()!=cols)
                            ||borders.vertical.len()!=table.cells.len()
                            ||borders.vertical.iter().any(|row|row.len()!=cols+1)
                            ||borders.horizontal.iter().chain(&borders.vertical).flatten().flatten().any(|border|!border.width.is_finite()||border.width<=0.0||border.width>100.0))
                    || table.merged_cells.iter().enumerate().any(|(index,merge)| {
                        merge.rows==0||merge.columns==0||(merge.rows==1&&merge.columns==1)
                        || merge.row.checked_add(merge.rows).is_none_or(|end|end>table.cells.len())
                        || merge.column.checked_add(merge.columns).is_none_or(|end|end>cols)
                        || table.merged_cells.iter().skip(index+1).any(|other| {
                            let (Some(merge_row_end),Some(merge_col_end),Some(other_row_end),Some(other_col_end))=
                                (merge.row.checked_add(merge.rows),merge.column.checked_add(merge.columns),other.row.checked_add(other.rows),other.column.checked_add(other.columns)) else{return true};
                            merge.row<other_row_end&&other.row<merge_row_end
                                &&merge.column<other_col_end&&other.column<merge_col_end
                        })
                    }) {
                    return Err("Table cell data is invalid".into());
                }
            }
            (ShapeKind::Table,None) => return Err("Table data is missing".into()),
            (_,Some(_)) => return Err("Table data is attached to a non-table shape".into()),
            _=>{}
        }
        for member in &shape.members {
            if !members.insert(*member) || *member==CanvasTarget::Shape(shape.id) {return Err("Frameの所属が重複しています".into());}
            let valid=match member {
                CanvasTarget::Node(id)=>node_ids.contains(id),
                CanvasTarget::TextBox(id)=>text_ids.contains(id),
                CanvasTarget::Shape(id)=>ids.contains(id) && shapes.iter().any(|s|s.id==*id && s.kind!=ShapeKind::Frame),
                CanvasTarget::Line(id)=>line_ids.contains(id),
            };
            if !valid {return Err("Frameの所属先が存在しないか、入れ子になっています".into());}
        }
    }
    let mut conn_ids = HashSet::new();
    for c in connectors {
        if !conn_ids.insert(c.id) || c.id >= next_id || !c.width.is_finite()
            || !(0.25..=32.0).contains(&c.width) || c.from.target == c.to.target
            || !c.label_size.is_finite() || !(7.0..=120.0).contains(&c.label_size)
            || !c.label_fraction.is_finite() || !(0.0..=1.0).contains(&c.label_fraction)
            || !c.label_offset.x.is_finite() || !c.label_offset.y.is_finite()
            || c.waypoints.len() > 2048 || c.waypoints.iter().any(|p|!p.x.is_finite()||!p.y.is_finite()) {
            return Err("ConnectorLinesのID・幅または端点が不正です".into());
        }
        for endpoint in [c.from, c.to] {
            let valid = match endpoint.target {
                CanvasTarget::Node(id) => node_ids.contains(&id),
                CanvasTarget::TextBox(id) => text_ids.contains(&id),
                CanvasTarget::Shape(id) => ids.contains(&id),
                CanvasTarget::Line(_) => false,
            };
            if !valid { return Err("存在しないオブジェクトへのConnectorがあります".into()); }
        }
    }
    if !conn_ids.is_disjoint(&ids) || !conn_ids.is_disjoint(&line_ids) { return Err("CanvasIDが重複しています".into()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_geometry_and_reference_are_rejected() {
        let s = CanvasShape { id: 1, kind: ShapeKind::Ellipse, position: Offset::default(),
            width: 180.0, height: 80.0, z: 0, visible: true, locked: false,
            style: CanvasStyle::default(), text: String::new(), members:Vec::new(), grid:None, table:None };
        let nodes = HashSet::from([3]);
        assert!(validate_shapes(&[s.clone()], &[], &[], &nodes, &HashSet::new(), 10).is_ok());
        let mut bad=s.clone(); bad.width=f32::NAN;
        assert!(validate_shapes(&[bad], &[], &[], &nodes, &HashSet::new(), 10).is_err());
        let c = CanvasConnector { id: 2,
            from: ConnectorEnd {target: CanvasTarget::Shape(1), anchor: Anchor::Right},
            to: ConnectorEnd {target: CanvasTarget::Node(4), anchor: Anchor::Left},
            color: [0,0,0], width: 2.0, arrow: true,
            ..CanvasConnector::new(2, ConnectorEnd{target:CanvasTarget::Shape(1),anchor:Anchor::Right},
              ConnectorEnd{target:CanvasTarget::Node(4),anchor:Anchor::Left},true)};
        assert!(validate_shapes(&[s], &[], &[c], &nodes, &HashSet::new(), 10).is_err());
    }
}
