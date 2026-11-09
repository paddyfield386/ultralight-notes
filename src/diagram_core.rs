//! Diagram geometry: no GUI dependencies. All route/waypoint coordinates are world-space.
use crate::canvas_core::{Anchor, ConnectorRoute, ConnectorMode, CanvasConnector, ShapeKind};
use crate::mindmap_core::Offset;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteObstacle { pub left: f32, pub top: f32, pub right: f32, pub bottom: f32 }

fn blocked(p: Offset, obstacles: &[RouteObstacle]) -> bool {
    obstacles.iter().any(|o| p.x > o.left && p.x < o.right && p.y > o.top && p.y < o.bottom)
}

/// Automatic orthogonal route which keeps the anchors fixed and detours around
/// expanded obstacle rectangles. Candidate bends are generated from obstacle
/// edges, so the route remains stable while adding as few corners as possible.
pub fn route_points_avoiding(connector: &CanvasConnector, start: Offset, end: Offset, obstacles: &[RouteObstacle]) -> Vec<Offset> {
    if connector.mode != ConnectorMode::Auto || obstacles.is_empty() {
        return route_points(connector, start, end);
    }
    // A straight/polyline connector that intersects an object is promoted to
    // an orthogonal detour for this automatic render pass. The stored route
    // style remains unchanged, so editing/history behavior is preserved.
    let mut routed=connector.clone();
    routed.route=ConnectorRoute::Orthogonal;
    // Keep a short normal segment at each anchor so the connector always
    // leaves/enters the object's surface perpendicularly.
    let clearance = 24.0;
    let normal = |p: Offset, anchor: Anchor, distance: f32| match anchor {
        Anchor::Left => Offset{x:p.x-distance,y:p.y}, Anchor::Right => Offset{x:p.x+distance,y:p.y},
        Anchor::Top => Offset{x:p.x,y:p.y-distance}, Anchor::Bottom => Offset{x:p.x,y:p.y+distance},
    };
    // Shorten the stubs when another expanded obstacle is close to the
    // anchor. Otherwise a perfectly routable narrow gap falls back to a
    // direct line through the intervening node.
    let stub_distance = |p: Offset, anchor: Anchor| {
        let mut distance: f32=clearance;
        for o in obstacles {
            let gap=match anchor {
                Anchor::Left if p.y>o.top && p.y<o.bottom && o.right<=p.x => Some(p.x-o.right),
                Anchor::Right if p.y>o.top && p.y<o.bottom && o.left>=p.x => Some(o.left-p.x),
                Anchor::Top if p.x>o.left && p.x<o.right && o.bottom<=p.y => Some(p.y-o.bottom),
                Anchor::Bottom if p.x>o.left && p.x<o.right && o.top>=p.y => Some(o.top-p.y),
                _ => None,
            };
            if let Some(gap)=gap { distance=distance.min(gap * 0.5); }
        }
        distance
    };
    let start_stub=normal(start,routed.from.anchor,stub_distance(start,routed.from.anchor));
    let end_stub=normal(end,routed.to.anchor,stub_distance(end,routed.to.anchor));
    let mut xs = vec![start_stub.x, end_stub.x];
    let mut ys = vec![start_stub.y, end_stub.y];
    for o in obstacles {
        xs.extend([o.left, o.right]); ys.extend([o.top, o.bottom]);
    }
    xs.sort_by(|a,b| a.total_cmp(b)); xs.dedup_by(|a,b| (*a-*b).abs()<0.01);
    ys.sort_by(|a,b| a.total_cmp(b)); ys.dedup_by(|a,b| (*a-*b).abs()<0.01);
    let mut points = Vec::new();
    for &y in &ys { for &x in &xs { let p=Offset{x,y}; if !blocked(p,obstacles) { points.push(p); } } }
    let nearest = |p: Offset| points.iter().position(|q| (q.x-p.x).abs()<0.01 && (q.y-p.y).abs()<0.01);
    let (Some(si),Some(ei))=(nearest(start_stub),nearest(end_stub)) else { return Vec::new(); };
    let mut dist=vec![f32::INFINITY;points.len()]; let mut prev:Vec<Option<usize>>=vec![None;points.len()]; let mut used=vec![false;points.len()]; dist[si]=0.0;
    for _ in 0..points.len() {
        let Some((u,_))=dist.iter().enumerate().filter(|(i,d)|!used[*i]&&d.is_finite()).min_by(|a,b|a.1.total_cmp(b.1)) else {break};
        used[u]=true; if u==ei {break;}
        for v in 0..points.len() { if used[v] {continue;} let a=points[u];let b=points[v];
            if (a.x-b.x).abs()>0.01 && (a.y-b.y).abs()>0.01 {continue;}
            // Do not immediately reverse back along an endpoint stub. Such
            // U-turns draw a spurious overhanging segment beyond the junction.
            let dot = |x: Offset, y: Offset| x.x*y.x+x.y*y.y;
            if u==si && dot(Offset{x:b.x-a.x,y:b.y-a.y},Offset{x:start_stub.x-start.x,y:start_stub.y-start.y}) < -0.001 {continue;}
            if v==ei && dot(Offset{x:b.x-a.x,y:b.y-a.y},Offset{x:end.x-end_stub.x,y:end.y-end_stub.y}) < -0.001 {continue;}
            let blocked_seg=obstacles.iter().any(|o| {
                if (a.x-b.x).abs()<0.01 { a.x>o.left&&a.x<o.right&&a.y.min(b.y)<o.bottom&&a.y.max(b.y)>o.top }
                else { a.y>o.top&&a.y<o.bottom&&a.x.min(b.x)<o.right&&a.x.max(b.x)>o.left }
            });
            if blocked_seg {continue;}
            let bend=if prev[u].is_some_and(|p| { let q=points[p]; ((q.x-a.x).abs()>0.01) != ((a.x-b.x).abs()>0.01) }) { 8.0 } else {0.0};
            let nd=dist[u]+(a.x-b.x).abs()+(a.y-b.y).abs()+bend;
            if nd<dist[v] {dist[v]=nd;prev[v]=Some(u);}
        }
    }
    if !dist[ei].is_finite() { return Vec::new(); }
    let mut out=Vec::new(); let mut i=ei; out.push(points[i]); while let Some(p)=prev[i] {i=p;out.push(points[i]);} out.reverse();
    let mut compact:Vec<Offset>=Vec::new(); for p in out { if compact.len()<2 || {let a=compact[compact.len()-2];let b=*compact.last().unwrap(); (a.x-b.x).abs()>0.01 || (b.x-p.x).abs()>0.01 || (a.y-b.y).abs()>0.01 || (b.y-p.y).abs()>0.01} {compact.push(p);} }
    let mut result=Vec::with_capacity(compact.len()+2);
    result.push(start); result.extend(compact); result.push(end);
    result
}

pub fn shape_vertices(kind: ShapeKind, x: f32, y: f32, width: f32, height: f32) -> Option<Vec<Offset>> {
    let normalized: &[(f32, f32)] = match kind {
        ShapeKind::Diamond => &[(0.5,0.0),(1.0,0.5),(0.5,1.0),(0.0,0.5)],
        ShapeKind::Parallelogram => &[(0.22,0.0),(1.0,0.0),(0.78,1.0),(0.0,1.0)],
        ShapeKind::Hexagon => &[(0.23,0.0),(0.77,0.0),(1.0,0.5),(0.77,1.0),(0.23,1.0),(0.0,0.5)],
        ShapeKind::Document => &[(0.0,0.0),(1.0,0.0),(1.0,0.83),(0.8,0.89),(0.55,0.83),(0.3,0.91),(0.0,0.83)],
        _ => return None,
    };
    Some(normalized.iter().map(|&(xx,yy)| Offset{x:x+width*xx,y:y+height*yy}).collect())
}

fn push_unique(out: &mut Vec<Offset>, point: Offset) {
    if out.last().is_none_or(|last| (last.x-point.x).abs() + (last.y-point.y).abs()>0.01) {
        out.push(point);
    }
}

/// The first and last points always coincide with the current anchors.
/// Manual middle points are preserved (orthogonal segments are repaired after moving anchors).
pub fn route_points(connector: &CanvasConnector, start: Offset, end: Offset) -> Vec<Offset> {
    if connector.route == ConnectorRoute::Straight && connector.mode==ConnectorMode::Auto {
        return vec![start,end];
    }
    let mut points = vec![start];
    if connector.mode==ConnectorMode::Manual && !connector.waypoints.is_empty() {
        for waypoint in &connector.waypoints {
            if connector.route==ConnectorRoute::Orthogonal {
                let current=*points.last().unwrap_or(&start);
                if (current.x-waypoint.x).abs()>0.001 && (current.y-waypoint.y).abs()>0.001 {
                    // Preserve the endpoint's preferred axis without changing a stored waypoint.
                    if matches!(connector.from.anchor,Anchor::Left|Anchor::Right) {
                        push_unique(&mut points,Offset{x:waypoint.x,y:current.y});
                    } else { push_unique(&mut points,Offset{x:current.x,y:waypoint.y}); }
                }
            }
            push_unique(&mut points,*waypoint);
        }
    } else if connector.route == ConnectorRoute::Orthogonal {
        match (connector.from.anchor,connector.to.anchor) {
            (Anchor::Right|Anchor::Left, Anchor::Right|Anchor::Left) => {
                let middle=(start.x+end.x)*0.5;
                push_unique(&mut points,Offset{x:middle,y:start.y});
                push_unique(&mut points,Offset{x:middle,y:end.y});
            }
            (Anchor::Top|Anchor::Bottom, Anchor::Top|Anchor::Bottom) => {
                let middle=(start.y+end.y)*0.5;
                push_unique(&mut points,Offset{x:start.x,y:middle});
                push_unique(&mut points,Offset{x:end.x,y:middle});
            }
            (Anchor::Right|Anchor::Left,_) => push_unique(&mut points,Offset{x:end.x,y:start.y}),
            _ => push_unique(&mut points,Offset{x:start.x,y:end.y}),
        }
    } else if connector.route == ConnectorRoute::Polyline {
        // A simple editable two-piece route; manual points override this suggestion.
        let middle=Offset{x:(start.x+end.x)*0.5,y:(start.y+end.y)*0.5};
        push_unique(&mut points,middle);
    }
    if connector.route == ConnectorRoute::Orthogonal {
        let current=*points.last().unwrap_or(&start);
        if (current.x-end.x).abs()>0.001 && (current.y-end.y).abs()>0.001 {
            if matches!(connector.to.anchor,Anchor::Left|Anchor::Right) {
                push_unique(&mut points,Offset{x:current.x,y:end.y});
            } else { push_unique(&mut points,Offset{x:end.x,y:current.y}); }
        }
    }
    push_unique(&mut points,end);
    if points.len()==1 {points.push(end);}
    points
}

pub fn distance_sq(point: Offset, a: Offset, b: Offset) -> f32 {
    let dx=b.x-a.x;let dy=b.y-a.y;
    let t=if dx*dx+dy*dy>0.0001 {((point.x-a.x)*dx+(point.y-a.y)*dy)/(dx*dx+dy*dy)} else {0.0};
    let t=t.clamp(0.0,1.0);
    (point.x-a.x-dx*t).powi(2)+(point.y-a.y-dy*t).powi(2)
}

pub fn point_at_fraction(path: &[Offset], fraction: f32) -> Offset {
    let first=path.first().copied().unwrap_or_default();
    if path.len()<2 {return first;}
    let total:f32=path.windows(2).map(|ab|(ab[1].x-ab[0].x).hypot(ab[1].y-ab[0].y)).sum();
    let mut remain=total*fraction.clamp(0.0,1.0);
    for window in path.windows(2) {
        let (a,b)=(window[0],window[1]);
        let d=(b.x-a.x).hypot(b.y-a.y);
        if d>=remain {let t=if d>0.0001 {remain/d}else{0.0};return Offset{x:a.x+(b.x-a.x)*t,y:a.y+(b.y-a.y)*t};}
        remain-=d;
    }
    *path.last().unwrap_or(&first)
}

pub fn nearest_segment(path:&[Offset], p:Offset) -> Option<usize> {
    path.windows(2).enumerate().min_by(|(_,a),(_,b)|
        distance_sq(p,a[0],a[1]).total_cmp(&distance_sq(p,b[0],b[1])))
        .map(|(index,_)|index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_core::{CanvasTarget,ConnectorEnd};
    fn connector(route:ConnectorRoute)->CanvasConnector {
        let mut c=CanvasConnector::new(5,
            ConnectorEnd{target:CanvasTarget::Shape(1),anchor:Anchor::Right},
            ConnectorEnd{target:CanvasTarget::Shape(2),anchor:Anchor::Left},true);
        c.route=route;c
    }
    #[test]
    fn all_new_polygon_vertices_follow_shape_bounds() {
        for kind in [ShapeKind::Diamond,ShapeKind::Parallelogram,ShapeKind::Hexagon,ShapeKind::Document] {
            let v=shape_vertices(kind,20.0,30.0,150.0,95.0).unwrap();
            assert!(v.len()>=4 && v.iter().all(|p|p.x>=20.0&&p.x<=170.0&&p.y>=30.0&&p.y<=125.0));
        }
    }
    #[test]
    fn orthogonal_route_uses_only_axis_aligned_segments() {
        let c=connector(ConnectorRoute::Orthogonal);
        let a=Offset{x:100.0,y:50.0};let b=Offset{x:350.0,y:210.0};
        let points=route_points(&c,a,b);
        assert_eq!(points.first(),Some(&a));assert_eq!(points.last(),Some(&b));
        assert!(points.windows(2).all(|x|(x[0].x-x[1].x).abs()<0.001||(x[0].y-x[1].y).abs()<0.001));
    }
    #[test]
    fn automatic_route_detours_around_obstacle() {
        let c=connector(ConnectorRoute::Orthogonal);
        let a=Offset{x:20.0,y:60.0}; let b=Offset{x:380.0,y:60.0};
        let obstacle=RouteObstacle{left:150.0,top:20.0,right:250.0,bottom:100.0};
        let path=route_points_avoiding(&c,a,b,&[obstacle]);
        assert!(path.len()>2);
        assert!(path.windows(2).all(|w| {
            let (p,q)=(w[0],w[1]);
            !((p.x-q.x).abs()<0.01 && p.x>obstacle.left && p.x<obstacle.right && p.y.min(q.y)<obstacle.bottom && p.y.max(q.y)>obstacle.top)
        }));
    }
    #[test]
    fn offset_right_ports_do_not_overshoot_and_reverse() {
        let mut c=connector(ConnectorRoute::Orthogonal);
        c.from.anchor=Anchor::Right; c.to.anchor=Anchor::Right;
        let start=Offset{x:100.0,y:25.0}; let end=Offset{x:116.0,y:95.0};
        let obstacles=[RouteObstacle{left:0.0,top:0.0,right:100.0,bottom:50.0},
            RouteObstacle{left:16.0,top:70.0,right:116.0,bottom:120.0}];
        let path=route_points_avoiding(&c,start,end,&obstacles);
        assert!(path.len()>=4, "missing route: {path:?}");
        let before=path[path.len()-3]; let stub=path[path.len()-2];
        assert!((stub.x-before.x)*(end.x-stub.x)+(stub.y-before.y)*(end.y-stub.y)>=-0.001,
            "route reverses over its end stub: {path:?}");
        let stub=path[1]; let next=path[2];
        assert!((next.x-stub.x)*(stub.x-start.x)+(next.y-stub.y)*(stub.y-start.y)>=-0.001,
            "route reverses over its start stub: {path:?}");
    }
    #[test]
    fn tight_vertical_gap_does_not_fall_back_through_middle_node() {
        let mut c=connector(ConnectorRoute::Orthogonal);
        c.from.anchor=Anchor::Bottom; c.to.anchor=Anchor::Top;
        let start=Offset{x:50.0,y:50.0}; let end=Offset{x:50.0,y:110.0};
        let obstacles=[
            RouteObstacle{left:0.0,top:0.0,right:100.0,bottom:50.0},
            RouteObstacle{left:10.0,top:58.0,right:90.0,bottom:102.0},
            RouteObstacle{left:0.0,top:110.0,right:100.0,bottom:160.0},
        ];
        let path=route_points_avoiding(&c,start,end,&obstacles);
        assert!(path.len()>2, "expected an actual detour: {path:?}");
        assert_eq!(path.first(),Some(&start)); assert_eq!(path.last(),Some(&end));
        for segment in path.windows(2) {
            let (a,b)=(segment[0],segment[1]);
            assert!(obstacles.iter().all(|o| {
                if (a.x-b.x).abs()<0.001 {
                    !(a.x>o.left && a.x<o.right && a.y.min(b.y)<o.bottom && a.y.max(b.y)>o.top)
                } else {
                    (a.y-b.y).abs()<0.001 && !(a.y>o.top && a.y<o.bottom && a.x.min(b.x)<o.right && a.x.max(b.x)>o.left)
                }
            }), "segment {a:?} -> {b:?} crosses a node");
        }
    }
    #[test]
    fn waypoint_and_label_fraction_are_stable() {
        let mut c=connector(ConnectorRoute::Polyline);
        c.mode=ConnectorMode::Manual;c.waypoints=vec![Offset{x:180.0,y:85.0}];
        let path=route_points(&c,Offset{x:100.0,y:80.0},Offset{x:250.0,y:200.0});
        assert_eq!(path[1],c.waypoints[0]);
        let mid=point_at_fraction(&path,0.5);
        assert!(mid.x.is_finite()&&mid.y.is_finite());
        assert_eq!(nearest_segment(&path,Offset{x:180.0,y:85.0}),Some(0));
    }
}

#[cfg(test)]
mod document_roundtrip_tests {
    use super::*;
    use crate::canvas_core::{CanvasTarget, ConnectorEnd, ConnectorDash};
    use crate::mindmap_core::Document;

    #[test]
    fn v09_connectors_and_new_shapes_survive_json_version_two() {
        let mut doc=Document::sample_design();
        let a=doc.add_shape(ShapeKind::Diamond,Offset{x:100.0,y:100.0});
        let b=doc.add_shape(ShapeKind::Cylinder,Offset{x:400.0,y:200.0});
        let conn=doc.add_connector(
            ConnectorEnd{target:CanvasTarget::Shape(a),anchor:Anchor::Right},
            ConnectorEnd{target:CanvasTarget::Shape(b),anchor:Anchor::Left},true).unwrap();
        let c=doc.connectors.iter_mut().find(|c|c.id==conn).unwrap();
        c.route=ConnectorRoute::Orthogonal;c.mode=ConnectorMode::Manual;
        c.waypoints=vec![Offset{x:250.0,y:180.0}];
        c.arrow_start=true;c.dash=ConnectorDash::Dashed;
        c.label="確認済み → 本番".into();c.label_size=18.0;
        c.label_offset=Offset{x:0.0,y:-14.0};c.label_background=Some([255,255,255]);
        let saved=serde_json::to_string(&doc).unwrap();
        let restored:Document=serde_json::from_str(&saved).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.version,2);
        assert_eq!(restored.shape(a).unwrap().kind,ShapeKind::Diamond);
        assert_eq!(restored.shape(b).unwrap().kind,ShapeKind::Cylinder);
        assert_eq!(restored.connectors.iter().find(|c|c.id==conn),
            doc.connectors.iter().find(|c|c.id==conn));
    }
    #[test]
    fn invalid_waypoints_are_rejected_without_crashing(){
        let mut doc=Document::sample_design();
        let a=doc.add_shape(ShapeKind::Rectangle,Offset::default());
        let b=doc.add_shape(ShapeKind::Hexagon,Offset{x:500.0,y:200.0});
        let conn=doc.add_connector(
            ConnectorEnd{target:CanvasTarget::Shape(a),anchor:Anchor::Right},
            ConnectorEnd{target:CanvasTarget::Shape(b),anchor:Anchor::Left},false).unwrap();
        doc.connectors.iter_mut().find(|c|c.id==conn).unwrap()
            .waypoints.push(Offset{x:f32::NAN,y:100.0});
        assert!(doc.validate().is_err());
    }
    #[test]
    fn old_v08_connectors_deserialize_with_safe_defaults(){
        let old=r#"{"id":4,"from":{"target":{"kind":"shape","id":1},"anchor":"right"},
             "to":{"target":{"kind":"shape","id":2},"anchor":"left"},
             "color":[1,2,3],"width":2.0,"arrow":true}"#;
        let c:CanvasConnector=serde_json::from_str(old).unwrap();
        assert_eq!(c.route,ConnectorRoute::Straight);
        assert_eq!(c.mode,ConnectorMode::Auto);
        assert_eq!(c.label,"");
        assert!(c.label_visible);
        assert!(!c.arrow_start);
        assert!(c.waypoints.is_empty());
    }
}
