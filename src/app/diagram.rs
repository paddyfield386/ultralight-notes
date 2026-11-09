//! Diagram presentation and connector-edit gestures. Pure routing lives in diagram_core.
use super::*;
use crate::canvas_core::{CanvasConnector, ConnectorDash, ConnectorMode, ConnectorRoute, TextHorizontal, TextVertical};
use crate::diagram_core::{nearest_segment, point_at_fraction, route_points, route_points_avoiding, shape_vertices, RouteObstacle};

fn screen_point(p: Offset) -> Pos2 { Pos2::new(p.x,p.y) }
fn world_point(p: Pos2) -> Offset { Offset{x:p.x,y:p.y} }

impl MindmapApp {
    pub(super) fn node_alignment_guides(&self,id:NodeId,origin:Pos2)->(Vec2,Vec<(Pos2,Pos2)>) {
        let positions=self.positions();let Some(node)=self.doc.node(id)else{return (Vec2::ZERO,Vec::new())};
        let Some(&p)=positions.get(&id)else{return (Vec2::ZERO,Vec::new())};
        let moving=self.screen_rect(node,p,origin);
        let xs=[moving.left(),moving.center().x,moving.right()];let ys=[moving.top(),moving.center().y,moving.bottom()];
        let tolerance=9.0;let mut dx:Option<(f32,f32,Rect)>=None;let mut dy:Option<(f32,f32,Rect)>=None;
        let mut rects=Vec::new();
        for other in &self.doc.nodes {if other.id==id||self.doc.contains_descendant(id,other.id)||self.canvas_selection.contains(&CanvasTarget::Node(other.id)){continue;}if let Some(&op)=positions.get(&other.id){rects.push(self.screen_rect(other,op,origin));}}
        for shape in &self.doc.shapes {if shape.visible && !self.canvas_selection.contains(&CanvasTarget::Shape(shape.id)) {if let Some(r)=self.shape_screen_rect(shape.id,origin){rects.push(r);}}}
        for box_ in &self.doc.text_boxes {if !self.canvas_selection.contains(&CanvasTarget::TextBox(box_.id)){if let Some(r)=self.text_box_screen_rect(box_.id,origin){rects.push(r);}}}
        for r in &rects {
            for x in xs {for target in [r.left(),r.center().x,r.right()] {let d=target-x;if d.abs()<=tolerance&&dx.is_none_or(|(old,_,_)|d.abs()<old.abs()){dx=Some((d,target,*r));}}}
            for y in ys {for target in [r.top(),r.center().y,r.bottom()] {let d=target-y;if d.abs()<=tolerance&&dy.is_none_or(|(old,_,_)|d.abs()<old.abs()){dy=Some((d,target,*r));}}}
        }
        let mut guides=Vec::new();
        if let Some((_,x,r))=dx {guides.push((Pos2::new(x,moving.top().min(r.top())),Pos2::new(x,moving.bottom().max(r.bottom()))));}
        if let Some((_,y,r))=dy {guides.push((Pos2::new(moving.left().min(r.left()),y),Pos2::new(moving.right().max(r.right()),y)));}
        (Vec2::new(dx.map_or(0.0,|v|v.0),dy.map_or(0.0,|v|v.0)),guides)
    }
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
        let x=if shape.style.text_horizontal==TextHorizontal::Left {area.left()}
            else if shape.style.text_horizontal==TextHorizontal::Right {area.right()-galley.size().x}
            else {area.center().x-galley.size().x*0.5};
        let y=if shape.style.text_vertical==TextVertical::Top {area.top()}
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
        // Construct geometry directly from the document. A screen -> world
        // round trip changes coordinates by a few ulps as zoom/pan changes,
        // enough to flip an obstacle-boundary test or an equal-cost route.
        let world_rect = |target: CanvasTarget| -> Option<Rect> {
            match target {
                CanvasTarget::Node(id) => {
                    let node=self.doc.node(id)?; let pos=*positions.get(&id)?;
                    let depth=self.doc.depth(id);
                    Some(Rect::from_center_size(Pos2::new(pos.x,pos.y),
                        egui::vec2(self.card_width(node,depth),card_height(node,depth))))
                }
                CanvasTarget::Shape(id) => {
                    let s=self.doc.shape(id)?;
                    Some(Rect::from_min_size(Pos2::new(s.position.x,s.position.y),egui::vec2(s.width,s.height)))
                }
                CanvasTarget::TextBox(id) => {
                    let b=self.doc.text_box(id)?;
                    Some(Rect::from_min_size(Pos2::new(b.position.x,b.position.y),egui::vec2(b.width,b.height)))
                }
                CanvasTarget::Line(id) => {
                    let l=self.doc.line(id)?;
                    Some(Rect::from_two_pos(Pos2::new(l.start.x,l.start.y),Pos2::new(l.end.x,l.end.y)).expand(7.0))
                }
            }
        };
        let start=world_point(Self::anchor_position(world_rect(connector.from.target)?,connector.from.anchor));
        let end=world_point(Self::anchor_position(world_rect(connector.to.target)?,connector.to.anchor));
        let mut obstacle_rects=Vec::new();
        let targets=self.doc.shapes.iter().filter(|s|s.visible && s.kind!=ShapeKind::Frame).map(|s|CanvasTarget::Shape(s.id))
            .chain(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)))
            .chain(self.doc.nodes.iter().filter(|n|positions.contains_key(&n.id)).map(|n|CanvasTarget::Node(n.id)));
        for target in targets {
            let Some(r)=world_rect(target) else {continue;};
            obstacle_rects.push(r);
        }
        // Try the preferred clearance first, then relax it in small steps if
        // dense layouts leave no valid path. A connector must remain visible
        // even when the requested margin cannot be satisfied.
        let path=[24.0,16.0,8.0,0.0].into_iter().find_map(|margin|{
            let obstacles:Vec<_>=obstacle_rects.iter().map(|r|RouteObstacle{
                left:r.left()-margin,top:r.top()-margin,
                right:r.right()+margin,bottom:r.bottom()+margin,
            }).collect();
            let points=route_points_avoiding(connector,start,end,&obstacles);
            (!points.is_empty()).then_some(points)
        }).unwrap_or_else(||{
            // Last-resort orthogonal geometry preserves perpendicular anchor
            // stubs, even if a very crowded map prevents obstacle avoidance.
            let mut fallback=connector.clone();
            fallback.route=ConnectorRoute::Orthogonal;
            route_points(&fallback,start,end)
        });
        Some(path.into_iter()
            .map(|p|Pos2::new(origin.x+p.x*self.zoom,origin.y+p.y*self.zoom)).collect())
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
    #[cfg(test)]
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
        if ui.button(self.language.text("✎ Edit Label", "✎ ラベルを編集")).clicked(){self.begin_connector_edit(id);ui.close();}
        let label_visible=self.doc.connectors.iter().find(|c|c.id==id).is_some_and(|c|c.label_visible);
        if ui.button(if label_visible {self.language.text("Hide Label", "ラベルを非表示")}else{self.language.text("Show Label", "ラベルを表示")}).clicked(){
            self.update_connector_style(id,|c|c.label_visible=!c.label_visible);ui.close();
        }
        if false { ui.menu_button(self.language.text("Connector Route", "コネクター経路"),|ui|{
            for (label,route) in [("直Lines",ConnectorRoute::Straight),
                ("直角",ConnectorRoute::Orthogonal),("折れLines",ConnectorRoute::Polyline)]{
                if ui.button(label).clicked(){self.set_connector_route(id,route);ui.close();}
            }
            ui.separator();
            if ui.button(self.language.text("Restore Automatic Routing", "自動経路に戻す")).clicked(){
                self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.mode=ConnectorMode::Auto;c.waypoints.clear();}ui.close();
            }
            if ui.button(self.language.text("Switch Current Route to Manual", "現在の経路を手動にする")).clicked(){
                if let Some(points)=self.connector_world_path(id,positions,origin) {
                    self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){
                        c.waypoints=points[1..points.len().saturating_sub(1)].to_vec();c.mode=ConnectorMode::Manual;
                    }
                }
                ui.close();
            }
            if ui.button(self.language.text("Add Waypoint Here", "ここに経由点を追加")).clicked(){
                self.add_connector_waypoint(id,self.context_connector_point,positions,origin);
                ui.close();
            }
            if ui.button(self.language.text("Remove Nearest Waypoint", "最寄りの経由点を削除")).clicked(){
                if let Some(c)=self.doc.connectors.iter().find(|c|c.id==id){
                    let i=c.waypoints.iter().enumerate().min_by(|(_,a),(_,b)|{
                        (a.x-self.context_connector_point.x).hypot(a.y-self.context_connector_point.y)
                            .total_cmp(&(b.x-self.context_connector_point.x).hypot(b.y-self.context_connector_point.y))
                    }).map(|(i,_)|i);
                    if let Some(i)=i {self.checkpoint();if let Some(c)=self.doc.connectors.iter_mut().find(|c|c.id==id){c.waypoints.remove(i);}}
                }
                ui.close();
            }
        }); }
        ui.menu_button(self.language.text("Line Style", "線のスタイル"),|ui|{
            for (label,dash) in [("実Lines",ConnectorDash::Solid),("破Lines",ConnectorDash::Dashed),("点Lines",ConnectorDash::Dotted)]{
                if ui.button(label).clicked(){
                    self.update_connector_style(id,|c|c.dash=dash);
                    ui.close();
                }
            }
        });
        ui.menu_button(self.language.text("Line Width", "線の太さ"),|ui|{
            for width in [1.0_f32,2.0,3.0,5.0,8.0]{
                if ui.button(format!("{width:.0} px")).clicked(){
                    self.update_connector_style(id,|c|c.width=width);
                    ui.close();
                }
            }
        });
        ui.menu_button(self.language.text("Arrowheads", "矢印"),|ui|{
            for (label,start,end) in [("なし",false,false),("終点",false,true),("始点",true,false),("両端",true,true)] {
                if ui.button(label).clicked(){
                    self.update_connector_style(id,|c|{c.arrow_start=start;c.arrow=end;});ui.close();
                }
            }
        });
        ui.menu_button(self.language.text("Line Color", "線の色"),|ui|{
            for (label,color) in [("Blue",[80,126,222]),("Green",[65,164,110]),("赤",[217,91,106]),
                                  ("Purple",[145,100,203]),("Gray",[109,128,162]),("Black",[32,42,61])] {
                if ui.button(label).clicked(){self.update_connector_style(id,|c|c.color=color);ui.close();
                }
            }
        });
        ui.menu_button("ラベル",|ui|{
            ui.label(self.language.text("Position on Route", "経路上の位置"));
            for (label,fraction) in [(self.language.text("Near Start", "始点寄り"),0.25),(self.language.text("Center", "中央"),0.5),(self.language.text("Near End", "終点寄り"),0.75)] {
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
            ui.menu_button(self.language.text("Background Color", "背景色"),|ui|{
                for (label,color) in [(self.language.text("Transparent", "透明"),None),(self.language.text("White", "白"),Some([255,255,255])),(self.language.text("Yellow", "黄"),Some([255,245,196])),(self.language.text("Blue", "青"),Some([227,239,255]))]{
                    if ui.button(label).clicked(){self.update_connector_style(id,|c|c.label_background=color);ui.close();}
                }
            });
        });
        ui.separator();
        if ui.button(self.language.text("⌫ Delete Connector", "⌫ コネクターを削除")).clicked(){
            self.checkpoint();self.doc.connectors.retain(|c|c.id!=id);
            self.selected_connector=None;ui.close();
        }
    }
}

impl MindmapApp {
    /// World-coordinate snap comparisons, with a screen-pixel tolerance.
    /// Temporary Alt suppression is handled by the canvas input dispatcher.
    pub(super) fn alignment_guides_for(&self,id:u64,origin:Pos2)->(Vec2,Vec<(Pos2,Pos2)>) {
        self.object_alignment_guides(CanvasTarget::Shape(id),origin)
    }
    pub(super) fn object_alignment_guides(&self,moving:CanvasTarget,origin:Pos2)->(Vec2,Vec<(Pos2,Pos2)>) {
        let positions=self.positions();
        let Some(rect)=self.object_screen_rect(moving,&positions,origin) else{return (Vec2::ZERO,Vec::new())};
        let (left,top,right,bottom)=((rect.left()-origin.x)/self.zoom,(rect.top()-origin.y)/self.zoom,
            (rect.right()-origin.x)/self.zoom,(rect.bottom()-origin.y)/self.zoom);
        let x_pos=[left,(left+right)*0.5,right];
        let y_pos=[top,(top+bottom)*0.5,bottom];
        let tolerance=9.0/self.zoom.max(0.1);
        let mut dx:Option<(f32,Pos2,Pos2)>=None;
        let mut dy:Option<(f32,Pos2,Pos2)>=None;
        let targets=self.doc.shapes.iter().filter(|s|s.visible).map(|s|CanvasTarget::Shape(s.id))
            .chain(self.doc.text_boxes.iter().map(|b|CanvasTarget::TextBox(b.id)))
            .chain(self.doc.nodes.iter().filter(|n|positions.contains_key(&n.id)).map(|n|CanvasTarget::Node(n.id)));
        for other in targets {
            if other==moving||self.canvas_selection.contains(&other) {continue;}
            let Some(rect)=self.object_screen_rect(other,&positions,origin) else {continue;};
            let (l,t,r,b)=((rect.left()-origin.x)/self.zoom,(rect.top()-origin.y)/self.zoom,
                (rect.right()-origin.x)/self.zoom,(rect.bottom()-origin.y)/self.zoom);
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
    pub(super) fn apply_alignment_snap(&mut self,id:u64,origin:Pos2)->Vec2{
        let (delta,_)=self.alignment_guides_for(id,origin);
        if delta.length_sq()>0.0001 { self.move_canvas_selection(delta*self.zoom); }
        delta*self.zoom
    }
}

#[cfg(test)]
mod diagram_ui_regressions {
    use super::*;
    #[test]
    fn text_box_guides_align_to_shape_when_grid_snap_is_disabled(){
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.doc.nodes.clear();app.zoom=1.0;app.snap_to_grid=false;
        app.doc.add_shape(ShapeKind::Rectangle,Offset{x:100.0,y:100.0});
        let box_id=app.doc.add_text_box(103.0,350.0);
        app.canvas_selection.clear();app.canvas_selection.insert(CanvasTarget::TextBox(box_id));
        let (delta,guides)=app.object_alignment_guides(CanvasTarget::TextBox(box_id),Pos2::ZERO);
        assert!((delta.x+3.0).abs()<0.01);assert!(!guides.is_empty());
    }
    #[test]
    fn node_guides_ignore_objects_moving_in_the_same_selection(){
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.doc.nodes.retain(|n|n.id==ROOT);app.zoom=1.0;
        let positions=app.positions();let rect=app.screen_rect(app.doc.node(ROOT).unwrap(),positions[&ROOT],Pos2::ZERO);
        let shape=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:rect.left(),y:rect.bottom()+30.0});
        app.canvas_selection=HashSet::from([CanvasTarget::Node(ROOT),CanvasTarget::Shape(shape)]);
        let (delta,guides)=app.node_alignment_guides(ROOT,Pos2::ZERO);
        assert_eq!(delta,Vec2::ZERO);assert!(guides.is_empty());
    }
    #[test]
    fn shape_guides_include_text_boxes_without_grid_snap(){
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        app.doc.nodes.clear();app.zoom=1.0;app.snap_to_grid=false;
        let shape=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:100.0,y:100.0});
        let box_id=app.doc.add_text_box(103.0,350.0);
        app.canvas_selection.clear();app.canvas_selection.insert(CanvasTarget::Shape(shape));
        let (delta,guides)=app.alignment_guides_for(shape,Pos2::ZERO);
        assert!(app.doc.text_box(box_id).is_some());
        assert!((delta.x-3.0).abs()<0.01);
        assert!(!guides.is_empty());
    }
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
    fn connector_route_does_not_change_when_zoom_and_pan_change() {
        let mut app=MindmapApp::new(std::time::Instant::now(),None,false);
        let a=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:1500.25,y:1000.75});
        let b=app.doc.add_shape(ShapeKind::Rectangle,Offset{x:1500.25,y:1550.75});
        app.doc.add_shape(ShapeKind::Rectangle,Offset{x:1500.25,y:1275.75});
        let id=app.doc.add_connector(
            ConnectorEnd{target:CanvasTarget::Shape(a),anchor:Anchor::Bottom},
            ConnectorEnd{target:CanvasTarget::Shape(b),anchor:Anchor::Top},false).unwrap();
        let positions=app.positions();
        app.zoom=1.0;
        let reference=app.connector_world_path(id,&positions,Pos2::ZERO).unwrap();
        assert!(reference.len()>2,"expected an obstacle detour");
        for (zoom,origin) in [(0.3,Pos2::new(387.3,-152.7)),(0.73,Pos2::new(-279.4,502.6)),
            (1.37,Pos2::new(811.7,337.2)),(2.4,Pos2::new(-33.1,-734.9))] {
            app.zoom=zoom;
            let actual=app.connector_world_path(id,&positions,origin).unwrap();
            assert_eq!(actual.len(),reference.len(),"route topology changed at zoom {zoom}");
            for (a,b) in actual.iter().zip(&reference) {
                assert!((a.x-b.x).abs()<0.01 && (a.y-b.y).abs()<0.01,
                    "route moved at zoom {zoom}: {a:?} vs {b:?}");
            }
        }
    }
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
