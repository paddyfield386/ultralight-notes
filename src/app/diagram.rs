//! Diagram presentation and connector-edit gestures. Pure routing lives in diagram_core.
use super::*;
use crate::canvas_core::{CanvasConnector, ConnectorDash, ConnectorMode, ConnectorRoute, TextHorizontal, TextVertical};
use crate::diagram_core::{nearest_segment, point_at_fraction, route_points, shape_vertices};

fn screen_point(p: Offset) -> Pos2 { Pos2::new(p.x,p.y) }
fn world_point(p: Pos2) -> Offset { Offset{x:p.x,y:p.y} }

impl MindmapApp {
    /// Polygonal primitives are drawn with the same control points as SVG export.
    pub(super) fn paint_shape_kind(painter:&egui::Painter,kind:ShapeKind,rect:Rect,fill:Color32,stroke:Stroke) {
        if let Some(points)=shape_vertices(kind,rect.left(),rect.top(),rect.width(),rect.height()) {
            painter.add(egui::Shape::convex_polygon(points.into_iter().map(screen_point).collect(),fill,stroke));
        } else if kind==ShapeKind::Cylinder {
            let rim=rect.height().min(65.0)*0.14;
            let mut perimeter=Vec::new();
            // Top half of ellipse, right wall, bottom half and left wall.
            for i in 0..=24 {
                let t=std::f32::consts::PI+(i as f32/24.0)*std::f32::consts::PI;
                perimeter.push(Pos2::new(rect.center().x+rect.width()*0.5*t.cos(),rect.top()+rim+rim*t.sin()));
            }
            for i in 0..=24 {
                let t=(i as f32/24.0)*std::f32::consts::PI;
                perimeter.push(Pos2::new(rect.center().x+rect.width()*0.5*t.cos(),rect.bottom()-rim+rim*t.sin()));
            }
            painter.add(egui::Shape::convex_polygon(perimeter,fill,stroke));
            let rim_points:Vec<_>=(0..=48).map(|i|{
                let t=i as f32*std::f32::consts::TAU/48.0;
                Pos2::new(rect.center().x+rect.width()*0.5*t.cos(),rect.top()+rim+rim*t.sin())
            }).collect();
            painter.add(egui::Shape::line(rim_points,stroke));
        } else if kind==ShapeKind::Ellipse {
            let pts=(0..=48).map(|j|{
                let t=j as f32*std::f32::consts::TAU/48.0;
                rect.center()+egui::vec2(rect.width()*0.5*t.cos(),rect.height()*0.5*t.sin())
            }).collect();
            painter.add(egui::Shape::convex_polygon(pts,fill,stroke));
        } else {
            let radius=if kind==ShapeKind::RoundedRectangle {14.0}else if kind==ShapeKind::StickyNote {4.0}else{2.0};
            painter.rect_filled(rect,radius,fill);
            if stroke.width>0.0 {painter.rect_stroke(rect,radius,stroke,StrokeKind::Inside);}
        }
    }

    pub(super) fn paint_shape_text(&self,painter:&egui::Painter,rect:Rect,shape:&crate::canvas_core::CanvasShape) {
        if shape.text.is_empty()||self.editing_shape==Some(shape.id){return;}
        let is_sticky=shape.kind==ShapeKind::StickyNote;
        let area=rect.shrink(12.0*self.zoom);
        if area.width()<=1.0 || area.height()<=1.0 {return;}
        let clipped=painter.with_clip_rect(area);
        let font=FontId::proportional((if is_sticky {16.0}else{shape.style.text_size})*self.zoom);
        let fg=color_from_rgb(shape.style.text);
        let galley=clipped.layout(shape.text.clone(),font,fg,area.width());
        let x=if is_sticky || shape.style.text_horizontal==TextHorizontal::Left {area.left()}
            else if shape.style.text_horizontal==TextHorizontal::Right {area.right()-galley.size().x}
            else {area.center().x-galley.size().x*0.5};
        let y=if is_sticky || shape.style.text_vertical==TextVertical::Top {area.top()}
            else if shape.style.text_vertical==TextVertical::Bottom {area.bottom()-galley.size().y}
            else {area.center().y-galley.size().y*0.5};
        let too_long=galley.size().y>area.height()+2.0;
        clipped.galley(Pos2::new(x,y),galley,fg);
        if too_long && self.canvas_selection.contains(&CanvasTarget::Shape(shape.id)) {
            painter.circle_filled(area.right_bottom()-egui::vec2(5.0,5.0),3.0,
                Color32::from_rgb(223,105,75));
        }
    }

    /// Converts current route to screen points, preserving manual world-space waypoints.
    pub(super) fn connector_screen_path(&self,id:u64,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<Vec<Pos2>> {
        let connector=self.doc.connectors.iter().find(|c|c.id==id)?;
        let (a,b)=self.connector_screen_points(id,positions,origin)?;
        let mut scaled=connector.clone();
        for p in &mut scaled.waypoints {p.x=origin.x+p.x*self.zoom;p.y=origin.y+p.y*self.zoom;}
        Some(route_points(&scaled,world_point(a),world_point(b)).into_iter().map(screen_point).collect())
    }
    pub(super) fn connector_world_path(&self,id:u64,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<Vec<Offset>> {
        self.connector_screen_path(id,positions,origin).map(|p|p.into_iter().map(|q|{
            let s=(q-origin)/self.zoom;Offset{x:s.x,y:s.y}
        }).collect())
    }
    /// A single control point is available for every selected connector,
    /// including auto-routed connectors that have not been edited yet.
    pub(super) fn connector_control_point(&self,id:u64,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<Pos2> {
        let c=self.doc.connectors.iter().find(|c|c.id==id)?;
        if c.mode==ConnectorMode::Manual {
            if let Some(w)=c.waypoints.first() {
                return Some(origin+egui::vec2(w.x,w.y)*self.zoom);
            }
        }
        let path=self.connector_screen_path(id,positions,origin)?;
        let p=point_at_fraction(&path.iter().map(|p|world_point(*p)).collect::<Vec<_>>(),0.5);
        Some(screen_point(p))
    }
    pub(super) fn connector_waypoint_hit(&self,p:Pos2,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<(u64,usize)> {
        let id=self.selected_connector?;
        self.connector_control_point(id,positions,origin)
            .filter(|q|p.distance_sq(*q)<140.0).map(|_|(id,0))
    }
    pub(super) fn begin_connector_waypoint_drag(&mut self,id:u64,positions:&HashMap<NodeId,Position>,origin:Pos2) {
        let point=self.connector_control_point(id,positions,origin);
        let Some(point)=point else{return};
        self.checkpoint();
        if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id) {
            if c.mode==ConnectorMode::Auto || c.waypoints.is_empty() {
                c.waypoints=vec![Offset{x:(point.x-origin.x)/self.zoom,y:(point.y-origin.y)/self.zoom}];
                c.mode=ConnectorMode::Manual;
            }
        }
    }
    pub(super) fn connector_label_point(&self,id:u64,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<Pos2> {
        let c=self.doc.connectors.iter().find(|c|c.id==id)?;
        let points=self.connector_screen_path(id,positions,origin)?;
        let p=point_at_fraction(&points.iter().map(|p|world_point(*p)).collect::<Vec<_>>(),c.label_fraction);
        Some(screen_point(p)+egui::vec2(c.label_offset.x,c.label_offset.y)*self.zoom)
    }
    pub(super) fn connector_label_hit(&self,p:Pos2,positions:&HashMap<NodeId,Position>,origin:Pos2)->Option<u64> {
        self.doc.connectors.iter().filter(|c|c.label_visible&&!c.label.is_empty())
            .find(|c|self.connector_label_point(c.id,positions,origin).is_some_and(|center|
                Rect::from_center_size(center,egui::vec2(190.0,35.0)).contains(p))).map(|c|c.id)
    }
    pub(super) fn paint_connector(&self,painter:&egui::Painter,c:&CanvasConnector,positions:&HashMap<NodeId,Position>,origin:Pos2) {
        let Some(path)=self.connector_screen_path(c.id,positions,origin) else{return};
        if path.len()<2 {return;}
        let mut rect=Rect::NOTHING;
        for p in &path {rect.extend_with(*p);}
        if !rect.expand(24.0).intersects(painter.clip_rect()){return;}
        let color=color_from_rgb(c.color);
        let stroke=Stroke::new((c.width*self.zoom).max(0.75),color);
        for segment in path.windows(2){
            let a=segment[0];let b=segment[1];
            if c.dash==ConnectorDash::Solid {painter.line_segment([a,b],stroke);}
            else {
                let delta=b-a;let length=delta.length();
                if length>0.01 {
                    let u=delta/length;
                    let (draw,period)=if c.dash==ConnectorDash::Dotted {(2.0,7.0)}else{(9.0,15.0)};
                    let mut t=0.0;
                    while t<length {painter.line_segment([a+u*t,a+u*(t+draw).min(length)],stroke);t+=period;}
                }
            }
        }
        let arrow=|tip:Pos2,back:Pos2| {
            let d=tip-back;let len=d.length();
            if len<0.1 {return;}
            let u=d/len;let perp=egui::vec2(-u.y,u.x);
            for sign in [-1.0_f32,1.0] {
                painter.line_segment([tip,tip-u*(12.0*self.zoom).max(8.0)+perp*sign*(5.0*self.zoom).max(4.0)],stroke);
            }
        };
        if c.arrow {arrow(*path.last().unwrap(),path[path.len()-2]);}
        if c.arrow_start {arrow(path[0],path[1]);}
        if c.label_visible && !c.label.is_empty() && self.editing_connector!=Some(c.id) {
            if let Some(p)=self.connector_label_point(c.id,positions,origin){
                let font=FontId::proportional(c.label_size*self.zoom);
                let fg=color_from_rgb(c.label_color);
                let galley=painter.layout(c.label.clone(),font,fg,260.0*self.zoom);
                let rect=Rect::from_center_size(p,galley.size()+egui::vec2(12.0,6.0));
                if let Some(bg)=c.label_background {painter.rect_filled(rect,3.0,color_from_rgb(bg));}
                painter.galley(p-galley.size()*0.5,galley,fg);
            }
        }
        if self.selected_connector==Some(c.id)||self.selected_connectors.contains(&c.id){
            painter.circle_filled(path[0],5.5,ACCENT);
            painter.circle_filled(*path.last().unwrap(),5.5,ACCENT);
            if let Some(p)=self.connector_control_point(c.id,positions,origin) {
                let handle=Rect::from_center_size(p,egui::vec2(11.0,11.0));
                painter.rect_filled(handle,2.0,Color32::WHITE);
                painter.rect_stroke(handle,2.0,Stroke::new(1.6,ACCENT),StrokeKind::Inside);
            }
        }
    }
    pub(super) fn set_connector_route(&mut self,id:u64,route:ConnectorRoute) {
        if self.doc.connectors.iter().any(|c|c.id==id && c.route!=route){
            self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.route=route;}
        }
    }
    pub(super) fn add_connector_waypoint(&mut self,id:u64,point:Offset,positions:&HashMap<NodeId,Position>,origin:Pos2) {
        let Some(path)=self.connector_world_path(id,positions,origin) else{return};
        let Some(index)=nearest_segment(&path,point) else{return};
        let Some(before)=self.doc.connectors.iter().find(|c|c.id==id).cloned() else{return};
        self.checkpoint();
        if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id) {
            if c.mode==ConnectorMode::Auto {
                c.waypoints=path[1..path.len().saturating_sub(1)].to_vec();
            }
            // The visible route can contain auto-generated elbows that aren't stored.
            c.waypoints.insert(index.min(c.waypoints.len()),point);
            c.mode=ConnectorMode::Manual;
            if before.route==ConnectorRoute::Straight {c.route=ConnectorRoute::Polyline;}
        }
    }
    pub(super) fn begin_connector_edit(&mut self,id:u64){
        if let Some(c)=self.doc.connectors.iter().find(|c|c.id==id){
            self.editing_connector=Some(id);
            self.connector_edit_text=c.label.clone();
            self.connector_edit_focus_pending=true;
            self.selected_connector=Some(id);
            self.selected_connectors.clear();self.selected_connectors.insert(id);
        }
    }
    pub(super) fn finish_connector_edit(&mut self){
        if let Some(id)=self.editing_connector.take(){
            if self.doc.connectors.iter().any(|c|c.id==id && c.label!=self.connector_edit_text){
                self.checkpoint();
                if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.label=self.connector_edit_text.clone();}
            }
        }
        self.connector_edit_focus_pending=false;
    }
    pub(super) fn inline_connector_editor(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,positions:&HashMap<NodeId,Position>,origin:Pos2) {
        let Some(id)=self.editing_connector else{return};
        let Some(center)=self.connector_label_point(id,positions,origin) else{self.editing_connector=None;return};
        let rect=Rect::from_center_size(center,egui::vec2(240.0*self.zoom,34.0*self.zoom));
        let editor_frame=egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(ui.visuals().widgets.inactive.bg_stroke)
            .corner_radius(4);
        let response=ui.place(rect,egui::TextEdit::singleline(&mut self.connector_edit_text)
            .id_salt(("connector-inline-label",id))
            .font(FontId::proportional(15.0*self.zoom))
            .desired_width(rect.width())
            .frame(editor_frame));
        if self.connector_edit_focus_pending {response.request_focus();self.connector_edit_focus_pending=false;}
        if (ctx.input(|i|i.key_pressed(egui::Key::Enter)) || response.lost_focus()) && !self.ime_composing {
            self.finish_connector_edit();
        }
    }
    /// Approximate nearest-facing anchors; user-dragged anchors remain explicit.
    pub(super) fn auto_anchor_pair(a:Rect,b:Rect)->(Anchor,Anchor){
        let delta=b.center()-a.center();
        if delta.x.abs()>=delta.y.abs() {
            if delta.x>=0.0 {(Anchor::Right,Anchor::Left)}else{(Anchor::Left,Anchor::Right)}
        }else if delta.y>=0.0 {(Anchor::Bottom,Anchor::Top)}else{(Anchor::Top,Anchor::Bottom)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_port_is_the_nearest_visible_circle_to_destination(){
        let r=Rect::from_min_size(Pos2::new(100.0,100.0),egui::vec2(120.0,60.0));
        assert_eq!(MindmapApp::facing_source_anchor(r,Pos2::new(330.0,135.0)),Anchor::Right);
        assert_eq!(MindmapApp::facing_source_anchor(r,Pos2::new(160.0,260.0)),Anchor::Bottom);
        assert_eq!(MindmapApp::visual_anchor_position(r,Anchor::Top),r.center_top());
    }
    #[test]
    fn pair_anchors_choose_nearest_facing_sides(){
        let a=Rect::from_min_size(Pos2::ZERO,egui::vec2(100.0,100.0));
        let b=Rect::from_min_size(Pos2::new(250.0,10.0),egui::vec2(100.0,100.0));
        assert_eq!(MindmapApp::auto_anchor_pair(a,b),(Anchor::Right,Anchor::Left));
    }
}

impl MindmapApp {
    pub(super) fn update_connector_style(&mut self,id:u64,apply:impl Fn(&mut CanvasConnector)){
        let mut selected=self.selected_connectors.clone();selected.insert(id);
        if selected.is_empty(){return;}
        self.checkpoint();
        for c in &mut self.doc.connectors {if selected.contains(&c.id){apply(c);}}
    }
    /// Compact context editing for connector routes, endpoint arrows and labels.
    pub(super) fn connector_context_menu(&mut self,ui:&mut egui::Ui,id:u64,positions:&HashMap<NodeId,Position>,origin:Pos2){
        if ui.button("✎ Edit Label").clicked(){self.begin_connector_edit(id);ui.close();}
        let label_visible=self.doc.connectors.iter().find(|c|c.id==id).is_some_and(|c|c.label_visible);
        if ui.button(if label_visible {"Hide Label"}else{"Show Label"}).clicked(){
            self.update_connector_style(id,|c|c.label_visible=!c.label_visible);ui.close();
        }
        ui.menu_button("Connector Route",|ui|{
            for (label,route) in [("直Lines",ConnectorRoute::Straight),
                ("直角",ConnectorRoute::Orthogonal),("折れLines",ConnectorRoute::Polyline)]{
                if ui.button(label).clicked(){self.set_connector_route(id,route);ui.close();}
            }
            ui.separator();
            if ui.button("Restore Automatic Routing").clicked(){
                self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.mode=ConnectorMode::Auto;c.waypoints.clear();}ui.close();
            }
            if ui.button("Switch Current Route to Manual").clicked(){
                if let Some(points)=self.connector_world_path(id,positions,origin) {
                    self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){
                        c.waypoints=points[1..points.len().saturating_sub(1)].to_vec();c.mode=ConnectorMode::Manual;
                    }
                }
                ui.close();
            }
            if ui.button("Add Waypoint Here").clicked(){
                self.add_connector_waypoint(id,self.context_connector_point,positions,origin);
                ui.close();
            }
            if ui.button("Remove Nearest Waypoint").clicked(){
                if let Some(c)=self.doc.connectors.iter().find(|c|c.id==id){
                    let i=c.waypoints.iter().enumerate().min_by(|(_,a),(_,b)|{
                        (a.x-self.context_connector_point.x).hypot(a.y-self.context_connector_point.y)
                            .total_cmp(&(b.x-self.context_connector_point.x).hypot(b.y-self.context_connector_point.y))
                    }).map(|(i,_)|i);
                    if let Some(i)=i {self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.waypoints.remove(i);}}
                }
                ui.close();
            }
        });
        ui.menu_button("Line Style",|ui|{
            for (label,dash) in [("実Lines",ConnectorDash::Solid),("破Lines",ConnectorDash::Dashed),("点Lines",ConnectorDash::Dotted)]{
                if ui.button(label).clicked(){
                    self.update_connector_style(id,|c|c.dash=dash);
                    ui.close();
                }
            }
        });
        ui.menu_button("Line Width",|ui|{
            for width in [1.0_f32,2.0,3.0,5.0,8.0]{
                if ui.button(format!("{width:.0} px")).clicked(){
                    self.update_connector_style(id,|c|c.width=width);
                    ui.close();
                }
            }
        });
        ui.menu_button("Arrowheads",|ui|{
            for (label,start,end) in [("なし",false,false),("終点",false,true),("始点",true,false),("両端",true,true)] {
                if ui.button(label).clicked(){
                    self.update_connector_style(id,|c|{c.arrow_start=start;c.arrow=end;});ui.close();
                }
            }
        });
        ui.menu_button("Line Color",|ui|{
            for (label,color) in [("Blue",[80,126,222]),("Green",[65,164,110]),("赤",[217,91,106]),
                                  ("Purple",[145,100,203]),("Gray",[109,128,162]),("Black",[32,42,61])] {
                if ui.button(label).clicked(){self.update_connector_style(id,|c|c.color=color);ui.close();
                }
            }
        });
        ui.menu_button("ラベル",|ui|{
            ui.label("Position on Route");
            for (label,fraction) in [("Near Start",0.25),("Center",0.5),("Near End",0.75)] {
                if ui.button(label).clicked(){self.update_connector_style(id,|c|c.label_fraction=fraction);ui.close();}
            }
            ui.label("サイズ");
            for size in [12.0_f32,15.0,18.0,24.0] {
                if ui.button(format!("{size:.0} pt")).clicked(){self.update_connector_style(id,|c|c.label_size=size);ui.close();}
            }
            ui.menu_button("文字色",|ui|{
                for (label,color) in [("紺",[48,59,82]),("Black",[0,0,0]),("Blue",[80,126,222]),("赤",[217,91,106])]{
                    if ui.button(label).clicked(){self.update_connector_style(id,|c|c.label_color=color);ui.close();}
                }
            });
            ui.menu_button("Background Color",|ui|{
                for (label,color) in [("Transparent",None),("White",Some([255,255,255])),("薄Yellow色",Some([255,245,196])),("薄Blue",Some([227,239,255]))]{
                    if ui.button(label).clicked(){self.update_connector_style(id,|c|c.label_background=color);ui.close();}
                }
            });
        });
        ui.separator();
        if ui.button("⌫ Delete Connector").clicked(){
            self.checkpoint();self.doc.connectors.retain(|c|c.id!=id);
            self.selected_connector=None;ui.close();
        }
    }
}

impl MindmapApp {
    /// World-coordinate snap comparisons, with a screen-pixel tolerance.
    /// Temporary Alt suppression is handled by the canvas input dispatcher.
    pub(super) fn alignment_guides_for(&self,id:u64,origin:Pos2)->(Vec2,Vec<(Pos2,Pos2)>) {
        if !self.snap_to_grid {return (Vec2::ZERO,Vec::new());}
        let Some(moving)=self.doc.shape(id) else{return (Vec2::ZERO,Vec::new())};
        let (left,top,right,bottom)=(moving.position.x,moving.position.y,
            moving.position.x+moving.width,moving.position.y+moving.height);
        let x_pos=[left,(left+right)*0.5,right];
        let y_pos=[top,(top+bottom)*0.5,bottom];
        let tolerance=9.0/self.zoom.max(0.1);
        let mut dx:Option<(f32,Pos2,Pos2)>=None;
        let mut dy:Option<(f32,Pos2,Pos2)>=None;
        for other in &self.doc.shapes {
            if !other.visible||other.id==id||self.canvas_selection.contains(&CanvasTarget::Shape(other.id)) {continue;}
            let (l,t,r,b)=(other.position.x,other.position.y,
                other.position.x+other.width,other.position.y+other.height);
            for &x in &x_pos {for &target in &[l,(l+r)*0.5,r] {
                let diff=target-x;
                if diff.abs()<=tolerance && dx.is_none_or(|(old,_,_)|diff.abs()<old.abs()) {
                    let anchor=origin.x+target*self.zoom;
                    dx=Some((diff,Pos2::new(anchor,origin.y+top.min(t)*self.zoom),
                        Pos2::new(anchor,origin.y+bottom.max(b)*self.zoom)));
                }
            }}
            for &y in &y_pos {for &target in &[t,(t+b)*0.5,b] {
                let diff=target-y;
                if diff.abs()<=tolerance && dy.is_none_or(|(old,_,_)|diff.abs()<old.abs()) {
                    let anchor=origin.y+target*self.zoom;
                    dy=Some((diff,Pos2::new(origin.x+left.min(l)*self.zoom,anchor),
                        Pos2::new(origin.x+right.max(r)*self.zoom,anchor)));
                }
            }}
        }
        let guides=dx.iter().chain(dy.iter()).map(|(_,a,b)|(*a,*b)).collect();
        (egui::vec2(dx.map_or(0.0,|d|d.0),dy.map_or(0.0,|d|d.0)),guides)
    }
    pub(super) fn apply_alignment_snap(&mut self,id:u64,origin:Pos2){
        let (delta,_)=self.alignment_guides_for(id,origin);
        if delta.length_sq()>0.0001 { self.move_canvas_selection(delta*self.zoom); }
    }
}

#[cfg(test)]
mod diagram_ui_regressions {
    use super::*;
    #[test]
    fn multiple_connectors_can_change_line_style_in_one_undo_step(){
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let a=app.doc.add_shape(ShapeKind::Rectangle,Offset::default());
        let b=app.doc.add_shape(ShapeKind::Diamond,Offset{x:350.0,y:0.0});
        let c=app.doc.add_shape(ShapeKind::Ellipse,Offset{x:700.0,y:0.0});
        let ab=app.doc.add_connector(
            ConnectorEnd{target:CanvasTarget::Shape(a),anchor:Anchor::Right},
            ConnectorEnd{target:CanvasTarget::Shape(b),anchor:Anchor::Left},false).unwrap();
        let bc=app.doc.add_connector(
            ConnectorEnd{target:CanvasTarget::Shape(b),anchor:Anchor::Right},
            ConnectorEnd{target:CanvasTarget::Shape(c),anchor:Anchor::Left},false).unwrap();
        app.selected_connectors=HashSet::from([ab,bc]);
        app.update_connector_style(ab,|c|{c.dash=ConnectorDash::Dashed;c.width=3.0;});
        assert!(app.doc.connectors.iter().all(|c|c.dash==ConnectorDash::Dashed&&c.width==3.0));
        app.undo();
        assert!(app.doc.connectors.iter().all(|c|c.dash==ConnectorDash::Solid&&c.width==2.0));
    }
    #[test]
    fn guide_snap_matches_nearby_shape_center(){
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.zoom=1.0;app.snap_to_grid=true;
        let first=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:100.0,y:100.0});
        let other=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:303.0,y:105.0});
        app.canvas_selection.clear();app.canvas_selection.insert(CanvasTarget::Shape(first));
        let (delta,guides)=app.alignment_guides_for(first,Pos2::ZERO);
        assert_eq!(app.doc.shape(other).unwrap().position.x,303.0);
        assert!(delta.x.is_finite()&&delta.y.is_finite());
        assert!(!guides.is_empty());
    }
}

#[cfg(test)]
mod connector_handle_regressions {
    use super::*;
    #[test]
    fn connection_circles_are_centered_on_shape_outline(){
        let rect=Rect::from_min_size(Pos2::new(100.0,120.0),egui::vec2(180.0,100.0));
        assert_eq!(MindmapApp::visual_anchor_position(rect,Anchor::Left),rect.left_center());
        assert_eq!(MindmapApp::visual_anchor_position(rect,Anchor::Right),rect.right_center());
        assert_eq!(MindmapApp::visual_anchor_position(rect,Anchor::Top),rect.center_top());
        assert_eq!(MindmapApp::visual_anchor_position(rect,Anchor::Bottom),rect.center_bottom());
    }
    #[test]
    fn hovered_unselected_shape_can_start_an_anchor_drag(){
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let id=app.doc.add_shape(ShapeKind::Diamond,Offset{x:300.0,y:220.0});
        let origin=Pos2::ZERO;
        let r=app.shape_screen_rect(id,origin).unwrap();
        let p=MindmapApp::visual_anchor_position(r,Anchor::Right);
        assert_eq!(app.find_anchor(p,&app.positions(),origin),
            Some(ConnectorEnd{target:CanvasTarget::Shape(id),anchor:Anchor::Right}));
    }
}
