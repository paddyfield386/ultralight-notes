use super::*;
use crate::canvas_core::{CanvasTarget,ShapeKind,Anchor,ConnectorEnd};

#[test]
fn v2_round_trip_and_connector_endpoint_integrity() {
    let mut doc=Document::sample_design();
    assert_eq!(doc.version,2);
    let r=doc.add_shape(ShapeKind::RoundedRectangle, Offset{x:400.0,y:120.0});
    let e=doc.add_shape(ShapeKind::Ellipse, Offset{x:620.0,y:120.0});
    let connector=doc.add_connector(
        ConnectorEnd{target:CanvasTarget::Shape(r),anchor:Anchor::Right},
        ConnectorEnd{target:CanvasTarget::Shape(e),anchor:Anchor::Left},true).unwrap();
    let json=serde_json::to_string_pretty(&doc).unwrap();
    let restored:Document=serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.connectors[0].id,connector);
    assert_eq!(restored.shapes.len(),2);
    assert_eq!(restored.connectors[0].from.target,CanvasTarget::Shape(r));
}

#[test]
fn deleting_object_cleans_up_references_and_is_undo_friendly() {
    let mut doc=Document::blank();
    let s=doc.add_shape(ShapeKind::Rectangle,Offset{x:5.0,y:10.0});
    doc.add_connector(ConnectorEnd{target:CanvasTarget::Node(ROOT),anchor:Anchor::Right},
        ConnectorEnd{target:CanvasTarget::Shape(s),anchor:Anchor::Left},false).unwrap();
    let before=doc.clone();
    assert!(doc.delete_shape(s));
    assert!(doc.connectors.is_empty());
    assert_eq!(before.connectors.len(),1);
    doc.validate().unwrap();
}

#[test]
fn grouped_items_preserve_relative_z_order() {
    let mut doc=Document::blank();
    let ids:Vec<_>=(0..4).map(|n|doc.add_shape(ShapeKind::Rectangle,Offset{x:n as f32*100.0,y:0.0})).collect();
    let chosen=HashSet::from([ids[1],ids[2]]);
    assert!(doc.change_z_for_selection(&chosen,2));
    assert_eq!(doc.shapes.iter().map(|s|s.id).collect::<Vec<_>>(),vec![ids[0],ids[3],ids[1],ids[2]]);
    assert!(doc.change_z_for_selection(&chosen,-2));
    assert_eq!(doc.shapes.iter().map(|s|s.id).collect::<Vec<_>>(),vec![ids[1],ids[2],ids[0],ids[3]]);
    doc.validate().unwrap();
}

#[test]
fn z_order_single_step_is_persistent_and_preserves_selection_order() {
    let mut doc = Document::blank();
    let ids: Vec<_> = (0..5)
        .map(|n| doc.add_shape(ShapeKind::Rectangle, Offset { x: n as f32 * 100.0, y: 0.0 }))
        .collect();
    let chosen = HashSet::from([ids[1], ids[3]]);
    assert!(doc.change_z_for_selection(&chosen, 1));
    let moved = vec![ids[0], ids[2], ids[1], ids[4], ids[3]];
    assert_eq!(doc.shapes.iter().map(|s| s.id).collect::<Vec<_>>(), moved);
    assert_eq!(doc.shapes.iter().map(|s| s.z).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);

    // The saved z values, not just the in-memory Vec order, must survive reload.
    let json = serde_json::to_string(&doc).unwrap();
    let mut restored: Document = serde_json::from_str(&json).unwrap();
    restored.normalize_z();
    assert_eq!(restored.shapes.iter().map(|s| s.id).collect::<Vec<_>>(), moved);
    assert!(restored.change_z_for_selection(&chosen, -1));
    assert_eq!(restored.shapes.iter().map(|s| s.id).collect::<Vec<_>>(), ids);
    restored.validate().unwrap();
}

#[test]
fn canvas_group_does_not_change_tree_parents() {
    let mut doc=Document::sample_design();
    let node=doc.children(ROOT)[0];
    let shape=doc.add_shape(ShapeKind::Ellipse, Offset{x:50.0,y:50.0});
    let before=doc.parent(node);
    let selected=HashSet::from([CanvasTarget::Node(node),CanvasTarget::Shape(shape)]);
    doc.group_canvas(&selected).unwrap();
    assert_eq!(doc.parent(node),before);
    assert_eq!(doc.canvas_group_members(CanvasTarget::Shape(shape)).len(),2);
    doc.validate().unwrap();
    assert!(doc.ungroup_canvas(&selected));
}

#[test]
fn multiline_card_height_expands_after_commit() {
    assert!(node_height_text("1行\n2行",2)>node_height_text("1行",2));
    assert_eq!(node_width_text("一行",2),node_width_text("一行\n一行",2));
}

#[test]
fn legacy_v1_document_is_readable_and_not_auto_modified() {
    let old=r#"{
        "version":1,"next_id":2,
        "nodes":[{"id":1,"parent":null,"title":"旧形式","offset":{"x":0.0,"y":0.0}}]
    }"#;
    let doc:Document=serde_json::from_str(old).unwrap();
    doc.validate().unwrap();
    assert_eq!(doc.version,1);
    assert!(doc.shapes.is_empty());
    assert!(doc.connectors.is_empty());
}
