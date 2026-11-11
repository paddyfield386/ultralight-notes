//! Full-board SVG/PNG export and native clipboard image copying.
//! Exports share one deterministic SVG scene and are independent of current pan/zoom.
use super::*;
use crate::diagram_core::{route_points,point_at_fraction,shape_vertices};
use crate::canvas_core::ConnectorDash;
use std::{borrow::Cow, fmt::Write as _};
use crate::canvas_core::{CanvasTarget, ShapeKind, Anchor};
use crate::spreadsheet::{CellCoord, TableInput, display_literal, evaluate_workbook};

const MARGIN: f32 = 48.0;

pub(super) struct Artwork {
    pub svg: String,
    pub width: f32,
    pub height: f32,
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

fn export_tint(doc: &Document, id: NodeId) -> Tint {
    let node = doc.node(id).expect("valid node id");
    if let Some(index) = node.color { return TINTS[index as usize % TINTS.len()]; }
    let root = doc.map_root(id).unwrap_or(ROOT);
    let branch = doc.root_branch(id).unwrap_or(root);
    let section = doc.children(root).iter().position(|&n| n == branch).unwrap_or(0);
    let board = doc.roots().iter().position(|&n| n == root).unwrap_or(0);
    TINTS[(section + board) % TINTS.len()]
}

fn world_anchor(doc:&Document,positions:&HashMap<NodeId,Position>,target:CanvasTarget,anchor:Anchor)->Option<(f32,f32)> {
    let (left,top,width,height)=match target {
        CanvasTarget::Node(id)=>{
            let n=doc.node(id)?;let p=positions.get(&id)?;let depth=doc.depth(id);
            let w=node_width_text(&n.title,depth);let h=card_height(n,depth);
            (p.x-w/2.0,p.y-h/2.0,w,h)
        }
        CanvasTarget::TextBox(id)=>{let b=doc.text_box(id)?;(b.position.x,b.position.y,b.width,b.height)}
        CanvasTarget::Shape(id)=>{let b=doc.shape(id)?;(b.position.x,b.position.y,b.width,b.height)}
        CanvasTarget::Line(_)=>return None,
    };
    Some(match anchor {
        Anchor::Top=>(left+width/2.0,top),
        Anchor::Right=>(left+width,top+height/2.0),
        Anchor::Bottom=>(left+width/2.0,top+height),
        Anchor::Left=>(left,top+height/2.0),
    })
}

fn geometry(doc: &Document, positions: &HashMap<NodeId, Position>) -> Result<(f32, f32, f32, f32), String> {
    let (mut left, mut top, mut right, mut bottom) =
        (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for node in &doc.nodes {
        let Some(pos) = positions.get(&node.id) else { continue };
        let depth = doc.depth(node.id);
        let width = node_width_text(&node.title, depth);
        let height = card_height(node, depth);
        left = left.min(pos.x - width / 2.0);
        right = right.max(pos.x + width / 2.0);
        top = top.min(pos.y - height / 2.0);
        bottom = bottom.max(pos.y + height / 2.0);
    }
    for b in &doc.text_boxes {
        left = left.min(b.position.x);
        right = right.max(b.position.x + b.width);
        top = top.min(b.position.y);
        bottom = bottom.max(b.position.y + b.height);
    }
    for s in &doc.shapes {
        if !s.visible {continue;}
        left=left.min(s.position.x);top=top.min(s.position.y);
        right=right.max(s.position.x+s.width);bottom=bottom.max(s.position.y+s.height);
    }
    for line in &doc.lines {
        if !line.visible{continue;}
        left=left.min(line.start.x.min(line.end.x));top=top.min(line.start.y.min(line.end.y));
        right=right.max(line.start.x.max(line.end.x));bottom=bottom.max(line.start.y.max(line.end.y));
    }
    for c in &doc.connectors {
        for waypoint in &c.waypoints {
            left=left.min(waypoint.x);right=right.max(waypoint.x);
            top=top.min(waypoint.y);bottom=bottom.max(waypoint.y);
        }
    }
    if !left.is_finite() || !top.is_finite() || !right.is_finite() || !bottom.is_finite() {
        return Err("There are no objects to export".into());
    }
    let w = right - left + MARGIN * 2.0;
    let h = bottom - top + MARGIN * 2.0;
    if w <= 0.0 || h <= 0.0 || w > 100_000.0 || h > 100_000.0 {
        return Err("The export area is too large".into());
    }
    Ok((left, top, w, h))
}

/// All visible nodes on all roots, detached text boxes and association links.
/// White background ensures consistent appearance in image viewers/PowerPoint.
pub(super) fn render_svg(doc: &Document) -> Result<Artwork, String> {
    doc.validate()?;
    let tables:Vec<_>=doc.shapes.iter().filter_map(|shape|{
        (shape.kind==ShapeKind::Table).then(||shape.table.as_ref().map(|table|(shape.id,table)))?
    }).collect();
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
    let spreadsheet=evaluate_workbook(&inputs);
    let positions = layout_with_widths(doc, |id, depth| {
        doc.node(id).map(|n| node_width_text(&n.title, depth)).unwrap_or(150.0)
    });
    let (left, top, width, height) = geometry(doc, &positions)?;
    let px = |x: f32| x - left + MARGIN;
    let py = |y: f32| y - top + MARGIN;
    let mut svg = String::new();
    writeln!(&mut svg, r###"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.1}" height="{height:.1}" viewBox="0 0 {width:.1} {height:.1}">"###).unwrap();
    writeln!(&mut svg, r###"<rect width="100%" height="100%" fill="#ffffff"/>"###).unwrap();
    // Frame fills are always behind their members. Frame membership is explicit.
    for frame in doc.shapes.iter().filter(|s|s.visible&&s.kind==ShapeKind::Frame) {
        let x=px(frame.position.x);let y=py(frame.position.y);
        let fill=frame.style.fill.unwrap_or([247,250,255]);
        let outline=frame.style.outline.unwrap_or([126,150,198]);
        writeln!(&mut svg,
            r###"<rect x="{x:.2}" y="{y:.2}" width="{:.2}" height="{:.2}" rx="5" fill="#{:02x}{:02x}{:02x}" fill-opacity="0.30" stroke="#{:02x}{:02x}{:02x}" stroke-width="{:.1}"/>"###,
            frame.width,frame.height,fill[0],fill[1],fill[2],outline[0],outline[1],outline[2],frame.style.stroke_width).unwrap();
    }
    // Free drawing lines are not connectors; export exactly their world endpoints.
    for line in doc.lines.iter().filter(|l|l.visible) {
        let dash=if line.dashed {"stroke-dasharray=\"9 6\""}else{""};
        writeln!(&mut svg,
            r###"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="#{:02x}{:02x}{:02x}" stroke-width="{:.2}" {dash}/>"###,
            px(line.start.x),py(line.start.y),px(line.end.x),py(line.end.y),
            line.color[0],line.color[1],line.color[2],line.width).unwrap();
    }
    // Export the same routed geometry as the interactive canvas.
    for c in &doc.connectors {
        let (Some((ax,ay)),Some((bx,by)))=(
            world_anchor(doc,&positions,c.from.target,c.from.anchor),
            world_anchor(doc,&positions,c.to.target,c.to.anchor)) else {continue};
        let points=route_points(c,Offset{x:ax,y:ay},Offset{x:bx,y:by});
        if points.len()<2 {continue;}
        let path=points.iter().enumerate().map(|(i,p)|
            format!("{} {:.2} {:.2}",if i==0 {"M"}else{"L"},px(p.x),py(p.y)))
            .collect::<Vec<_>>().join(" ");
        let stroke=hex(color_from_rgb(c.color));
        let dash=match c.dash {ConnectorDash::Solid=>"",ConnectorDash::Dashed=>" stroke-dasharray=\"9 6\"",
                                ConnectorDash::Dotted=>" stroke-dasharray=\"2 5\""};
        writeln!(&mut svg,
            r###"<path d="{path}" fill="none" stroke="{stroke}" stroke-width="{:.2}"{dash}/>"###,
            c.width).unwrap();
        let draw_arrow=|tip:Offset,back:Offset,svg:&mut String|{
            let dx=tip.x-back.x;let dy=tip.y-back.y;let len=dx.hypot(dy);
            if len<0.01 {return;}
            let ux=dx/len;let uy=dy/len;
            for sign in [-1.0_f32,1.0] {
                let ex=tip.x-ux*13.0+(-uy)*sign*5.5;
                let ey=tip.y-uy*13.0+ux*sign*5.5;
                writeln!(svg,
                    r###"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{:.2}"/>"###,
                    px(tip.x),py(tip.y),px(ex),py(ey),c.width).unwrap();
            }
        };
        if c.arrow {draw_arrow(*points.last().unwrap(),points[points.len()-2],&mut svg);}
        if c.arrow_start {draw_arrow(points[0],points[1],&mut svg);}
        if c.label_visible && !c.label.is_empty() {
            let mut label=point_at_fraction(&points,c.label_fraction);
            label.x+=c.label_offset.x;label.y+=c.label_offset.y;
            let fg=hex(color_from_rgb(c.label_color));
            if let Some(bg)=c.label_background {
                // Approximate background width; no fixed screen-coordinate label positions.
                let w=(c.label.chars().count() as f32*c.label_size*0.85+12.0).max(35.0);
                let bg=hex(color_from_rgb(bg));
                writeln!(&mut svg,r###"<rect x="{:.2}" y="{:.2}" width="{w:.2}" height="{:.2}" rx="3" fill="{bg}"/>"###,
                    px(label.x)-w/2.0,py(label.y)-c.label_size*0.8,c.label_size+9.0).unwrap();
            }
            writeln!(&mut svg,
                r###"<text x="{:.2}" y="{:.2}" text-anchor="middle" dominant-baseline="central" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="{:.2}" fill="{fg}">{}</text>"###,
                px(label.x),py(label.y),c.label_size,xml_escape(&c.label)).unwrap();
        }
    }
    // Tree connectors are smooth, solid Bézier curves.
    for node in &doc.nodes {
        let Some(parent) = node.parent else { continue };
        let (Some(a), Some(b)) = (positions.get(&parent), positions.get(&node.id)) else { continue };
        let parent_node = doc.node(parent).unwrap();
        let start_x = px(a.x + node_width_text(&parent_node.title, doc.depth(parent)) / 2.0);
        let end_x = px(b.x - node_width_text(&node.title, doc.depth(node.id)) / 2.0);
        let start_y = py(a.y);
        let end_y = py(b.y);
        let bend = ((end_x - start_x) * 0.48).max(24.0);
        writeln!(&mut svg,
            r###"<path d="M {start_x:.2} {start_y:.2} C {:.2} {start_y:.2}, {:.2} {end_y:.2}, {end_x:.2} {end_y:.2}" fill="none" stroke="{}" stroke-width="2.1"/>"###,
            start_x + bend, end_x - bend, hex(export_tint(doc, node.id).line)).unwrap();
    }
    // Association links are dashed and DO NOT change tree layout.
    for link in &doc.links {
        let (Some(a), Some(b), Some(from), Some(to)) = (
            positions.get(&link.from), positions.get(&link.to), doc.node(link.from), doc.node(link.to)
        ) else { continue };
        let a_w = node_width_text(&from.title, doc.depth(from.id));
        let b_w = node_width_text(&to.title, doc.depth(to.id));
        let (sx, ex) = if a.x < b.x {
            (a.x + a_w / 2.0, b.x - b_w / 2.0)
        } else { (a.x - a_w / 2.0, b.x + b_w / 2.0) };
        let dx = if ex >= sx { 1.0 } else { -1.0 };
        let bend = ((ex - sx).abs() * 0.42).max(24.0);
        writeln!(&mut svg,
            r###"<path d="M {:.2} {:.2} C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}" fill="none" stroke="{}" stroke-width="2" stroke-dasharray="9 6"/>"###,
            px(sx), py(a.y), px(sx + dx * bend), py(a.y), px(ex - dx * bend), py(b.y), px(ex), py(b.y),
            hex(color_from_rgb(doc.legacy_link_color(link.from, link.to).unwrap_or([134,146,169])))).unwrap();
    }
    // Keep cards above lines. Text is XML-escaped and uses Windows CJK fonts.
    for node in &doc.nodes {
        let Some(pos) = positions.get(&node.id) else { continue };
        let depth = doc.depth(node.id);
        let root = node.parent.is_none();
        let width = node_width_text(&node.title, depth);
        let height = card_height(node, depth);
        let x = px(pos.x - width / 2.0);
        let y = py(pos.y - height / 2.0);
        let tint = export_tint(doc, node.id);
        let root_color = node.color.map(|i| TINTS[i as usize % TINTS.len()].line).unwrap_or(ACCENT);
        let color = if root { root_color } else if depth >= 2 {
            blend_towards_white(tint.fill, 0.42)
        } else { tint.fill };
        let radius = if root { 24.0 } else if depth == 1 { 17.0 } else { 13.0 };
        writeln!(&mut svg,
            r###"<rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" rx="{radius:.1}" fill="{}" stroke="{}" stroke-width="1"/>"###,
            hex(color), hex(tint.border)).unwrap();
        let font_size = if root { 19.0 } else if depth == 1 { 16.5 } else { 15.0 };
        let fg = if root { "#ffffff" } else { "#1b273e" };
        let lines: Vec<&str> = node.title.split('\n').collect();
        let line_height = font_size * 1.35;
        let first_y = py(pos.y) - (lines.len().saturating_sub(1) as f32) * line_height * 0.5;
        for (index, line) in lines.iter().enumerate() {
            writeln!(&mut svg,
                r###"<text x="{:.2}" y="{:.2}" text-anchor="middle" dominant-baseline="central" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="{font_size}" fill="{fg}">{}</text>"###,
                px(pos.x), first_y + index as f32 * line_height, xml_escape(line)).unwrap();
        }
        if node.collapsed && !doc.children(node.id).is_empty() {
            writeln!(&mut svg,
                r###"<circle cx="{:.2}" cy="{:.2}" r="9" fill="{}"/>"###,
                px(pos.x + width / 2.0 + 10.0), py(pos.y), hex(ACCENT)).unwrap();
        }
    }
    for box_ in &doc.text_boxes {
        let x = px(box_.position.x);
        let y = py(box_.position.y);
        let fill = if box_.fill_transparent { "none".into() }
            else { box_.fill_color.map(|rgb| hex(color_from_rgb(rgb))).unwrap_or_else(|| "#ffffff".into()) };
        let border = if box_.border_none { "none".into() }
            else { box_.border_color.map(|rgb| hex(color_from_rgb(rgb))).unwrap_or_else(|| "#d7ddeb".into()) };
        writeln!(&mut svg,
            r###"<rect x="{x:.2}" y="{y:.2}" width="{:.2}" height="{:.2}" rx="5" fill="{fill}" stroke="{border}" stroke-width="1.2"/>"###,
            box_.width, box_.height).unwrap();
        for (line, row) in box_.text.lines().enumerate() {
            if line as f32 * 22.0 > box_.height - 15.0 { break; }
            writeln!(&mut svg,
                r###"<text x="{:.2}" y="{:.2}" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="15.5" fill="#1b273e">{}</text>"###,
                x + 15.0, y + 27.0 + line as f32 * 22.0, xml_escape(row)).unwrap();
        }
    }
    // Generic shapes share the same world coordinates as nodes and text boxes.
    let mut shapes:Vec<_>=doc.shapes.iter().filter(|s|s.visible).collect();
    shapes.sort_by_key(|s|(s.z,s.id));
    for shape in shapes {
        let x=px(shape.position.x);let y=py(shape.position.y);
        if shape.kind==ShapeKind::Frame {
            writeln!(&mut svg,
                r###"<text x="{:.2}" y="{:.2}" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="15" fill="#27324a">{}</text>"###,
                x+12.0,y+23.0,xml_escape(&shape.text)).unwrap();
            continue;
        }
        let fill=shape.style.fill.map(|c|format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2]))
            .unwrap_or_else(||"none".into());
        let stroke=shape.style.outline.map(|c|format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2]))
            .unwrap_or_else(||"none".into());
        if let Some(points)=shape_vertices(shape.kind,x,y,shape.width,shape.height) {
            let path=points.iter().map(|p|format!("{:.2},{:.2}",p.x,p.y)).collect::<Vec<_>>().join(" ");
            writeln!(&mut svg,
                r###"<polygon points="{path}" fill="{fill}" stroke="{stroke}" stroke-width="{:.2}"/>"###,
                shape.style.stroke_width).unwrap();
        } else if shape.kind==ShapeKind::Cylinder {
            let r=(shape.height.min(65.0)*0.14).max(2.0);
            writeln!(&mut svg,
                r###"<path d="M {x:.2} {:.2} A {:.2} {r:.2} 0 0 1 {:.2} {:.2} L {:.2} {:.2} A {:.2} {r:.2} 0 0 1 {x:.2} {:.2} Z" fill="{fill}" stroke="{stroke}" stroke-width="{:.2}"/>"###,
                y+r,shape.width/2.0,x+shape.width,y+r,x+shape.width,y+shape.height-r,
                shape.width/2.0,y+shape.height-r,shape.style.stroke_width).unwrap();
            writeln!(&mut svg,
                r###"<ellipse cx="{:.2}" cy="{:.2}" rx="{:.2}" ry="{r:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{:.2}"/>"###,
                x+shape.width/2.0,y+r,shape.width/2.0,shape.style.stroke_width).unwrap();
        } else if shape.kind==ShapeKind::Ellipse {
            writeln!(&mut svg,
                r###"<ellipse cx="{:.2}" cy="{:.2}" rx="{:.2}" ry="{:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{:.2}"/>"###,
                x+shape.width/2.0,y+shape.height/2.0,shape.width/2.0,shape.height/2.0,shape.style.stroke_width).unwrap();
        } else {
            let radius=if shape.kind==ShapeKind::RoundedRectangle {14.0}else{2.0};
            let shape_stroke=if shape.kind==ShapeKind::Table{"none"}else{stroke.as_str()};
            writeln!(&mut svg,
                r###"<rect x="{x:.2}" y="{y:.2}" width="{:.2}" height="{:.2}" rx="{radius}" fill="{fill}" stroke="{shape_stroke}" stroke-width="{:.2}"/>"###,
                shape.width,shape.height,shape.style.stroke_width).unwrap();
        }
        let text_color=shape.style.text;
        let fg=format!("#{:02x}{:02x}{:02x}",text_color[0],text_color[1],text_color[2]);
        if shape.kind==ShapeKind::Table {
            if let Some(table)=shape.table.as_ref() {
                let rows=table.cells.len();let cols=table.cells.first().map_or(0,Vec::len);
                if rows>0&&cols>0 {
                    for row in 0..rows {
                        for col in 0..cols {
                            if table.merge_at(row,col).is_some_and(|m|m.row!=row||m.column!=col){continue;}
                            let merge=table.merge_at(row,col);
                            let span_rows=merge.map_or(1,|m|m.rows);let span_cols=merge.map_or(1,|m|m.columns);
                            let cx0=x+shape.width*col as f32/cols as f32;
                            let cy0=y+table.row_offset(row);
                            let cw=shape.width*span_cols as f32/cols as f32;let ch=(row..row+span_rows).map(|i|table.row_height(i)).sum::<f32>();
                            let hidden_row=table.manually_hidden_rows.contains(&row)||table.row_is_filtered_out(row);
                            let cell_fill=if hidden_row{"#e8e8e8".to_owned()}else{table.cell_fills.as_ref().and_then(|fills|fills.get(row)).and_then(|r|r.get(col)).copied().flatten()
                                .map(|rgb|format!("#{:02x}{:02x}{:02x}",rgb[0],rgb[1],rgb[2]))
                                .unwrap_or_else(||if table.header_row&&row==0{"#dae5fc".to_owned()}else if table.banded_rows&&row%2==1{"#f1f4fa".to_owned()}else{fill.clone()})};
                            writeln!(&mut svg,r###"<rect x="{cx0:.2}" y="{cy0:.2}" width="{cw:.2}" height="{ch:.2}" fill="{cell_fill}"/>"###).unwrap();
                            if hidden_row{continue;}
                            let cx=cx0+cw/2.0;
                            let cy=cy0+ch/2.0;
                            let alignment=table.cell_alignments.as_ref().and_then(|a|a.get(row)).and_then(|a|a.get(col)).copied().unwrap_or(crate::canvas_core::TextHorizontal::Center);
                            let text_rgb=table.cell_text_colors.as_ref().and_then(|colors|colors.get(row)).and_then(|r|r.get(col)).copied().flatten().unwrap_or(text_color);
                            let cell_fg=format!("#{:02x}{:02x}{:02x}",text_rgb[0],text_rgb[1],text_rgb[2]);
                            let (tx,anchor)=match alignment {
                                crate::canvas_core::TextHorizontal::Left=>(cx0+4.0,"start"),
                                crate::canvas_core::TextHorizontal::Center=>(cx,"middle"),
                                crate::canvas_core::TextHorizontal::Right=>(cx0+cw-4.0,"end"),
                            };
                            let text=spreadsheet.display_value_at(shape.id,row,col)
                                .map(|value|value.display_text()).unwrap_or_else(||display_literal(&table.cells[row][col]));
                            let text_lines:Vec<_>=text.split('\n').collect();
                            for (line_index,line_text) in text_lines.iter().enumerate() {
                                let line_y=cy+(line_index as f32-(text_lines.len() as f32-1.0)/2.0)*shape.style.text_size*1.25;
                                writeln!(&mut svg,r###"<text x="{tx:.2}" y="{line_y:.2}" text-anchor="{anchor}" dominant-baseline="central" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="{:.2}" fill="{cell_fg}">{}</text>"###,
                                    shape.style.text_size,xml_escape(line_text)).unwrap();
                            }
                        }
                    }
                    let default_border=crate::canvas_core::TableBorder{color:shape.style.outline.unwrap_or([128,158,210]),width:shape.style.stroke_width.max(0.8),dash:crate::canvas_core::TableBorderDash::Solid};
                    for boundary in 0..=rows {for col in 0..cols {
                        if table.merged_cells.iter().any(|m|boundary>m.row&&boundary<m.row+m.rows&&col>=m.column&&col<m.column+m.columns){continue;}
                        let border=if let Some(grid)=&table.borders{grid.horizontal[boundary][col]}else{Some(default_border)};
                        if let Some(border)=border {let color=format!("#{:02x}{:02x}{:02x}",border.color[0],border.color[1],border.color[2]);let dash=match border.dash{crate::canvas_core::TableBorderDash::Solid=>"none",crate::canvas_core::TableBorderDash::Dashed=>"6,4",crate::canvas_core::TableBorderDash::Dotted=>"1,3"};let x0=x+shape.width*col as f32/cols as f32;let x1=x+shape.width*(col+1) as f32/cols as f32;let cy=y+table.row_offset(boundary);writeln!(&mut svg,r###"<line x1="{x0:.2}" y1="{cy:.2}" x2="{x1:.2}" y2="{cy:.2}" stroke="{color}" stroke-width="{:.2}" stroke-dasharray="{dash}"/>"###,border.width).unwrap();}
                    }}
                    for row in 0..rows {for boundary in 0..=cols {
                        if table.merged_cells.iter().any(|m|boundary>m.column&&boundary<m.column+m.columns&&row>=m.row&&row<m.row+m.rows){continue;}
                        let border=if let Some(grid)=&table.borders{grid.vertical[row][boundary]}else{Some(default_border)};
                        if let Some(border)=border {let color=format!("#{:02x}{:02x}{:02x}",border.color[0],border.color[1],border.color[2]);let dash=match border.dash{crate::canvas_core::TableBorderDash::Solid=>"none",crate::canvas_core::TableBorderDash::Dashed=>"6,4",crate::canvas_core::TableBorderDash::Dotted=>"1,3"};let cx=x+shape.width*boundary as f32/cols as f32;let y0=y+table.row_offset(row);let y1=y0+table.row_height(row);writeln!(&mut svg,r###"<line x1="{cx:.2}" y1="{y0:.2}" x2="{cx:.2}" y2="{y1:.2}" stroke="{color}" stroke-width="{:.2}" stroke-dasharray="{dash}"/>"###,border.width).unwrap();}
                    }}
                }
            }
            continue;
        }
        let line_count=shape.text.lines().count().max(1);
        if shape.kind==ShapeKind::StickyNote {
            for (i,row) in shape.text.lines().enumerate() {
                if (i as f32)*22.0>shape.height-20.0 {break;}
                writeln!(&mut svg,
                    r###"<text x="{:.2}" y="{:.2}" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="16" fill="{fg}">{}</text>"###,
                    x+12.0,y+30.0+(i as f32)*22.0,xml_escape(row)).unwrap();
            }
            continue;
        }
        let (text_x,anchor)=match shape.style.text_horizontal {
            crate::canvas_core::TextHorizontal::Left=>(x+12.0,"start"),
            crate::canvas_core::TextHorizontal::Center=>(x+shape.width/2.0,"middle"),
            crate::canvas_core::TextHorizontal::Right=>(x+shape.width-12.0,"end"),
        };
        let line_height=shape.style.text_size*1.4;
        let base_y=match shape.style.text_vertical {
            crate::canvas_core::TextVertical::Top=>y+12.0+shape.style.text_size*0.5,
            crate::canvas_core::TextVertical::Center=>y+shape.height/2.0-(line_count as f32-1.0)*line_height/2.0,
            crate::canvas_core::TextVertical::Bottom=>y+shape.height-12.0-(line_count as f32-1.0)*line_height,
        };
        for (i,row) in shape.text.lines().enumerate() {
            writeln!(&mut svg,
                r###"<text x="{text_x:.2}" y="{:.2}" text-anchor="{anchor}" dominant-baseline="central" font-family="Yu Gothic, Meiryo, Segoe UI, sans-serif" font-size="{:.2}" fill="{fg}">{}</text>"###,
                base_y+i as f32*line_height,shape.style.text_size,xml_escape(row)).unwrap();
        }
    }
    svg.push_str("</svg>\n");
    Ok(Artwork { svg, width, height })
}

/// Returns PNG bytes and *unpremultiplied* RGBA pixel data for Windows clipboard.
/// A white canvas makes every pixel opaque, so resvg's premultiplied RGBA is identical.
pub(super) fn render_png(doc: &Document) -> Result<(Vec<u8>, Vec<u8>, usize, usize), String> {
    let artwork = render_svg(doc)?;
    // Export at 2x when possible while capping the memory footprint.
    let scale = (8192.0_f32 / artwork.width.max(artwork.height)).min(2.0);
    let width = (artwork.width * scale).ceil().max(1.0) as u32;
    let height = (artwork.height * scale).ceil().max(1.0) as u32;
    if width == 0 || height == 0 || width.saturating_mul(height) > 40_000_000 {
        return Err("The image is too large".into());
    }
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&artwork.svg, &options)
        .map_err(|err| format!("Could not parse SVG drawing data: {err}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or("Could not allocate pixel memory for the PNG")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let bytes = pixmap.encode_png().map_err(|err| err.to_string())?;
    Ok((bytes, pixmap.data().to_vec(), width as usize, height as usize))
}

impl MindmapApp {
    pub(super) fn export_file(&mut self, png: bool) {
        self.finish_shape_edit(true);
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        let ext = if png { "png" } else { "svg" };
        let name = self.current_path.as_ref().and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "MindMap".into());
        let path = rfd::FileDialog::new()
            .add_filter(if png { "PNG画像" } else { "SVG画像" }, &[ext])
            .set_file_name(format!("{name}.{ext}"))
            .set_title(if png { "Save PNG Image" } else { "Save SVG Image" })
            .save_file();
        let Some(mut path) = path else { return };
        path.set_extension(ext);
        let result = if png {
            render_png(&self.doc).and_then(|(bytes, _, _, _)| write_json_atomic(&path, &bytes))
        } else {
            render_svg(&self.doc).and_then(|art| write_json_atomic(&path, art.svg.as_bytes()))
        };
        self.status = match result {
            Ok(()) => format!("Image saved: {}", path.display()),
            Err(e) => format!("Failed to save image: {e}"),
        };
    }
    pub(super) fn copy_image(&mut self) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        self.finish_shape_edit(true);
        let result = render_png(&self.doc).and_then(|(_, pixels, width, height)| {
            let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clip.set_image(arboard::ImageData {
                width, height, bytes: Cow::Owned(pixels),
            }).map_err(|e| e.to_string())
        });
        self.status = match result {
            Ok(()) => "Copied canvas image".into(),
            Err(err) => format!("Failed to copy image: {err}"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_svg_contains_nodes_textboxes_and_links() {
        let mut doc = Document::sample_design();
        let ids = doc.children(ROOT);
        assert!(doc.add_link(ids[0], ids[1]));
        let box_id = doc.add_text_box(900.0, 300.0);
        doc.text_box_mut(box_id).unwrap().text = "A < B & C".into();
        let rendered = render_svg(&doc).unwrap();
        assert!(rendered.svg.contains("stroke-dasharray"));
        assert!(rendered.svg.contains("Product Development Plan"));
        assert!(rendered.svg.contains("A &lt; B &amp; C"));
        assert!(rendered.width > 100.0);
        assert!(rendered.height > 100.0);
    }
    #[test]
    fn png_renders_and_is_decodable() {
        let doc = Document::sample_design();
        let (bytes, pixels, w, h) = render_png(&doc).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(pixels.len(), w*h*4);
    }
    #[test]
    fn v08_lines_stickies_frames_are_in_export() {
        let mut doc=Document::blank();
        let note=doc.add_shape(ShapeKind::StickyNote,Offset{x:500.0,y:200.0});
        doc.shape_mut(note).unwrap().text="Sticky Noteの本文".into();
        doc.add_frame_for(&HashSet::from([CanvasTarget::Shape(note)]),
            Offset{x:470.0,y:170.0},300.0,250.0).unwrap();
        let line=doc.add_line(Offset{x:500.0,y:500.0},Offset{x:800.0,y:500.0}).unwrap();
        doc.line_mut(line).unwrap().dashed=true;
        let svg=render_svg(&doc).unwrap().svg;
        assert!(svg.contains("Sticky Noteの本文"));
        assert!(svg.contains("stroke-dasharray"));
        assert!(svg.contains("fill-opacity"));
    }

}

#[cfg(test)]
mod diagram_v09_export_tests {
    use super::*;
    use crate::canvas_core::{CanvasTarget, ConnectorEnd, ConnectorRoute};
    #[test]
    fn svg_exports_new_shapes_routed_connectors_and_japanese_labels(){
        let mut doc=Document::sample_design();
        let a=doc.add_shape(ShapeKind::Diamond,Offset{x:300.0,y:-400.0});
        let b=doc.add_shape(ShapeKind::Document,Offset{x:600.0,y:-400.0});
        let c=doc.add_connector(
            ConnectorEnd{target:CanvasTarget::Shape(a),anchor:Anchor::Right},
            ConnectorEnd{target:CanvasTarget::Shape(b),anchor:Anchor::Left},true).unwrap();
        let conn=doc.connectors.iter_mut().find(|p|p.id==c).unwrap();
        conn.route=ConnectorRoute::Orthogonal;conn.label="条件を確認".into();
        let art=render_svg(&doc).unwrap();
        assert!(art.svg.contains("<polygon"));
        assert!(art.svg.contains("条件を確認"));
        assert!(art.svg.contains("<path"));
    }
}
