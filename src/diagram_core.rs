//! Diagram geometry: no GUI dependencies. All route/waypoint coordinates are world-space.
use crate::canvas_core::{Anchor, ConnectorRoute, ConnectorMode, CanvasConnector, ShapeKind};
use crate::mindmap_core::Offset;

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
