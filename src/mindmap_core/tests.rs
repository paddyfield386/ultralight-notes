//! JSON compatibility, layout and editing regressions.
use super::*;

#[cfg(test)]
mod v04_tests {
    use super::*;

    #[test]
    fn sibling_row_spacing_is_consistent() {
        let mut doc = Document::blank();
        let ids: Vec<_> = (0..5).map(|i| doc.add_child(ROOT, format!("Node{i}")).unwrap()).collect();
        let positions = layout(&doc);
        for pair in ids.windows(2) {
            let dy = positions[&pair[1]].y - positions[&pair[0]].y;
            assert!((dy - LEAF_SPACING).abs() < 0.01, "inconsistent sibling row step: {dy}");
        }
        let saved = serde_json::to_string_pretty(&doc).unwrap();
        let reloaded: Document = serde_json::from_str(&saved).unwrap();
        assert_eq!(layout(&reloaded), positions);
    }
    #[test]
    fn multiline_siblings_have_room_for_their_actual_card_heights() {
        let mut doc = Document::blank();
        let a = doc.add_child(ROOT, "1行目\n2行目").unwrap();
        let b = doc.add_child(ROOT, "単一行").unwrap();
        let positions = layout(&doc);
        let delta = positions[&b].y - positions[&a].y;
        let card_heights = (node_height_text("1行目\n2行目", 1)
            + node_height_text("単一行", 1)) * 0.5;
        assert!(delta >= card_heights + 26.0,
            "multiline sibling cards are too close: center distance {delta}, half-heights {card_heights}");
    }

    #[test]
    fn text_box_color_roundtrip_and_old_json_compatibility() {
        let mut doc = Document::blank();
        let id = doc.add_text_box(14.0, 17.0);
        doc.text_box_mut(id).unwrap().fill_color = Some([255, 245, 210]);
        doc.text_box_mut(id).unwrap().border_color = Some([240, 175, 15]);
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let restored: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.text_box(id).unwrap().fill_color, Some([255, 245, 210]));
        assert_eq!(restored.text_box(id).unwrap().border_color, Some([240, 175, 15]));
        assert!(restored.text_box(id).unwrap().border_none);
        // A genuine legacy fixture omits the newer appearance fields.
        let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
        old["text_boxes"][0].as_object_mut().unwrap().remove("fill_color");
        old["text_boxes"][0].as_object_mut().unwrap().remove("border_color");
        old["text_boxes"][0].as_object_mut().unwrap().remove("border_none");
        old["text_boxes"][0].as_object_mut().unwrap().remove("fill_transparent");
        let roundtrip: Document = serde_json::from_value(old).unwrap();
        assert_eq!(roundtrip.text_box(id).unwrap().fill_color, None);
        assert_eq!(roundtrip.text_box(id).unwrap().border_color, None);
        assert!(!roundtrip.text_box(id).unwrap().fill_transparent);
        assert!(!roundtrip.text_box(id).unwrap().border_none);
        doc.text_box_mut(id).unwrap().fill_color = None;
        doc.text_box_mut(id).unwrap().fill_transparent = true;
        let transparent_json = serde_json::to_string(&doc).unwrap();
        assert!(transparent_json.contains("\"fill_transparent\":true"));
        let restored: Document = serde_json::from_str(&transparent_json).unwrap();
        assert!(restored.text_box(id).unwrap().fill_transparent);
    }
    #[test]
    fn new_text_box_is_transparent_and_has_no_outline() {
        let mut doc = Document::blank();
        let id = doc.add_text_box(10.0, 20.0);
        let b = doc.text_box(id).unwrap();
        assert!(b.fill_transparent);
        assert!(b.border_none);
        let reloaded: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert!(reloaded.text_box(id).unwrap().fill_transparent);
        assert!(reloaded.text_box(id).unwrap().border_none);
    }
    #[test]
    fn annotations_roundtrip_and_are_outside_tree() {
        let mut doc = Document::sample_design();
        let previous_nodes = doc.nodes.len();
        let id = doc.add_text_box(120.0, -75.0);
        doc.text_box_mut(id).unwrap().text = "注記\n自由入力".into();
        doc.text_box_mut(id).unwrap().width = 420.0;
        doc.validate().unwrap();
        assert_eq!(doc.nodes.len(), previous_nodes);
        let loaded: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.text_box(id).unwrap().text, "注記\n自由入力");
        let mut roundtrip = loaded;
        assert!(roundtrip.delete_text_box(id));
        assert!(roundtrip.text_box(id).is_none());
    }
    #[test]
    fn pure_json_keeps_only_titles_and_children() {
        let mut doc = Document::sample_design();
        let id = doc.add_text_box(12.0, 44.0);
        doc.text_box_mut(id).unwrap().text = "floating".into();
        doc.node_mut(ROOT).unwrap().offset = Offset { x: 300.0, y: 100.0 };
        let json = doc.pure_tree_json(None).unwrap();
        let data: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(data["title"], "Product Development Plan");
        assert!(data["children"].is_array());
        for field in ["offset", "id", "collapsed", "color", "side", "groups", "text_boxes", "parent"] {
            assert!(!json.contains(&format!("\"{field}\"")));
        }
    }
    #[test]
    fn legacy_json_fixture_remains_compatible() {
        let doc: Document = serde_json::from_str(include_str!("../../samples/compatibility-example.json")).unwrap();
        doc.validate().unwrap();
        assert!(doc.text_boxes.is_empty());
        assert_eq!(doc.nodes.len(), 14);
    }
}

// helper on Document is unnecessary in production; testing should use a mutable clone.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_and_rightward_layout() {
        for doc in [Document::sample_design(), Document::sample_100()] {
            doc.validate().unwrap();
            let pos = layout(&doc);
            assert_eq!(pos.len(), doc.nodes.len());
            for node in doc.nodes.iter().filter(|n| n.id != ROOT) {
                assert!(pos[&node.id].x > pos[&node.parent.unwrap()].x);
            }
        }
        assert_eq!(Document::sample_100().nodes.len(), 100);
    }
    #[test]
    fn inherited_manual_offset_moves_subtree() {
        let mut doc = Document::sample_design();
        let parent = doc.children(ROOT)[0];
        let child = doc.children(parent)[0];
        let before = layout(&doc);
        doc.node_mut(parent).unwrap().offset = Offset { x: 70.0, y: -20.0 };
        let after = layout(&doc);
        for id in [parent, child] {
            assert!((after[&id].x - before[&id].x - 70.0).abs() < 1e-3);
            assert!((after[&id].y - before[&id].y + 20.0).abs() < 1e-3);
        }
    }
    #[test]
    fn reparent_cycle_protection_and_json_roundtrip() {
        let mut doc = Document::sample_design();
        let branches = doc.children(ROOT);
        let child = doc.children(branches[0])[0];
        assert!(!doc.reparent(ROOT, child));
        assert!(!doc.reparent(branches[0], child));
        assert!(doc.reparent(child, branches[1]));
        assert_eq!(doc.parent(child), Some(branches[1]));
        doc.node_mut(child).unwrap().color = Some(3);
        let text = serde_json::to_string_pretty(&doc).unwrap();
        let restored: Document = serde_json::from_str(&text).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, doc);
    }
    #[test]
    fn sibling_comes_immediately_after_and_deletion() {
        let mut doc = Document::blank();
        let a = doc.add_child(ROOT, "A").unwrap();
        let b = doc.add_child(ROOT, "B").unwrap();
        let c = doc.add_sibling(a, "C").unwrap();
        assert_eq!(doc.children(ROOT), vec![a, c, b]);
        assert!(doc.delete_subtree(c));
        doc.validate().unwrap();
    }
    #[test]
    fn sibling_drop_reorders_without_overlaps_and_survives_json() {
        let mut doc = Document::blank();
        let a = doc.add_child(ROOT, "A").unwrap();
        let b = doc.add_child(ROOT, "B").unwrap();
        let c = doc.add_child(ROOT, "C").unwrap();
        let d = doc.add_child(b, "D").unwrap();
        assert!(doc.move_next_to(c, a, true));
        assert_eq!(doc.children(ROOT), vec![a, c, b]);
        assert!(doc.move_next_to(d, a, true));
        assert_eq!(doc.parent(d), Some(ROOT));
        assert_eq!(doc.children(ROOT), vec![a, d, c, b]);
        assert!(!doc.move_next_to(a, d, false)); // adjacent: no change
        let positions = layout(&doc);
        for ids in [doc.children(ROOT), doc.children(b)] {
            for pair in ids.windows(2) {
                assert!(positions[&pair[0]].y < positions[&pair[1]].y);
            }
        }
        doc.validate().unwrap();
        let restored: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(doc, restored);
    }
    #[test]
    fn titles_expand_automatic_columns() {
        let mut doc = Document::blank();
        let a = doc.add_child(ROOT, "非常に長い日本語のタイトルを入力している最中のNode").unwrap();
        let b = doc.add_child(a, "子").unwrap();
        let base = layout(&doc);
        assert!(base[&b].x - base[&a].x > node_width_text(&doc.node(a).unwrap().title, 1)*0.5);
        let expanded = layout_with_widths(&doc, |id, depth| if id == a { 1000.0 } else { node_width_text(&doc.node(id).unwrap().title,depth) });
        assert!(expanded[&b].x > base[&b].x);
    }
    #[test]
    fn copy_node_only_excludes_children_and_preserves_node_properties() {
        let mut doc = Document::blank();
        let parent = doc.add_child(ROOT, "市場調査").unwrap();
        let child = doc.add_child(parent, "顧客インタビュー").unwrap();
        doc.add_child(child, "質問票").unwrap();
        doc.node_mut(parent).unwrap().color = Some(3);
        doc.node_mut(parent).unwrap().offset = Offset { x: 30.0, y: -10.0 };

        let single = doc.copy_node_only(parent).unwrap();
        let full = doc.copy_subtree(parent).unwrap();
        assert_eq!(single.plain_text(), "市場調査");
        assert_eq!(single.node_count(), 1);
        assert!(single.children.is_empty());
        assert_eq!(single.color, Some(3));
        assert_eq!(single.offset, Offset { x: 30.0, y: -10.0 });
        assert_eq!(full.node_count(), 3);

        let new_node = doc.paste_subtree_after(parent, &single).unwrap();
        assert!(doc.children(new_node).is_empty());
        assert_eq!(doc.node(new_node).unwrap().title, "市場調査");
        assert_eq!(doc.node(new_node).unwrap().color, Some(3));
        assert_eq!(doc.node(new_node).unwrap().offset, Offset::default());
        assert_eq!(doc.children(parent), vec![child]);
        doc.validate().unwrap();
    }

    #[test]
    fn copying_a_branch_pastes_a_deep_independent_copy() {
        let mut doc = Document::blank();
        let first = doc.add_child(ROOT, "市場調査").unwrap();
        let child = doc.add_child(first, "顧客インタビュー").unwrap();
        let grandchild = doc.add_child(child, "ヒアリング項目").unwrap();
        doc.node_mut(first).unwrap().color = Some(3);
        doc.node_mut(first).unwrap().offset = Offset { x: 81.0, y: 22.0 };
        doc.node_mut(child).unwrap().offset = Offset { x: 10.0, y: 3.0 };
        let snap = doc.copy_subtree(first).unwrap();
        assert_eq!(snap.node_count(), 3);
        assert_eq!(snap.plain_text(), "市場調査\n  顧客インタビュー\n    ヒアリング項目");
        let new_root = doc.paste_subtree_after(first, &snap).unwrap();
        let children = doc.children(ROOT);
        assert_eq!(children, vec![first, new_root]);
        let new_child = doc.children(new_root)[0];
        let new_grandchild = doc.children(new_child)[0];
        assert_ne!(first, new_root);
        assert_ne!(child, new_child);
        assert_ne!(grandchild, new_grandchild);
        assert_eq!(doc.node(new_root).unwrap().color, Some(3));
        assert_eq!(doc.node(new_root).unwrap().offset, Offset::default());
        assert_eq!(doc.node(new_child).unwrap().offset, Offset { x: 10.0, y: 3.0 });
        doc.node_mut(child).unwrap().title = "元の子をEdit".into();
        assert_eq!(doc.node(new_child).unwrap().title, "顧客インタビュー");
        doc.validate().unwrap();
        let reloaded: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(doc, reloaded);
    }

    #[test]
    fn paste_to_root_becomes_child_and_repeated_paste_creates_new_ids() {
        let mut doc = Document::blank();
        let a = doc.add_child(ROOT, "A").unwrap();
        let snap = doc.copy_subtree(a).unwrap();
        let b = doc.paste_subtree_after(ROOT, &snap).unwrap();
        let c = doc.paste_subtree_after(b, &snap).unwrap();
        assert_eq!(doc.children(ROOT), vec![a, b, c]);
        assert_eq!(doc.node(c).unwrap().title, "A");
        assert_ne!(b, c);
        doc.validate().unwrap();
    }

    #[test]
    fn paste_limits_document_size() {
        let mut doc = Document::sample_100();
        let snap = doc.copy_subtree(ROOT).unwrap();
        while doc.nodes.len() + snap.node_count() <= 10_000 {
            doc.paste_subtree_after(ROOT, &snap).unwrap();
        }
        let original_len = doc.nodes.len();
        assert!(doc.paste_subtree_after(ROOT, &snap).is_none());
        assert_eq!(doc.nodes.len(), original_len);
    }

    #[test]
    fn reads_original_v1_json_without_color() {
        let mut doc = Document::sample_design();
        let mut v = serde_json::to_value(&doc).unwrap();
        // The original prototype stored side=Left/Right. It is accepted but
        // ignored by the new right-only layout and omitted on re-serialization.
        v["nodes"][1]["side"] = serde_json::json!("Left");
        let restored: Document = serde_json::from_value(v).unwrap();
        assert_eq!(restored.nodes[1].side, Some(Side::Left));
        let encoded = serde_json::to_string(&restored).unwrap();
        assert!(!encoded.contains("\"side\""));
        doc.nodes[0].color = Some(9);
        assert!(doc.validate().is_err());
    }
    #[test]
    fn independent_maps_roundtrip_and_keep_board_positions() {
        let mut doc = Document::sample_design();
        let first = doc.roots()[0];
        let second = doc.add_map("第2マップ");
        let child = doc.add_child(second, "Right側のBranch").unwrap();
        let before = layout(&doc);
        assert!(before[&second].x > before[&first].x + 1000.0);
        assert!(before[&child].x > before[&second].x);
        doc.reset_all_offsets();
        let after = layout(&doc);
        assert_eq!(before[&second], after[&second]);
        let parsed: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed.roots(), vec![first, second]);
        let mut d2 = parsed.clone();
        assert!(d2.delete_subtree(second));
        assert_eq!(d2.roots(), vec![ROOT]);
        d2.validate().unwrap();
    }
    #[test]
    fn grouping_and_collapsing_are_preserved() {
        let mut doc = Document::sample_design();
        let a = doc.children(ROOT)[0];
        let b = doc.children(ROOT)[1];
        let c = doc.children(a)[0];
        let gid = doc.group_nodes(&HashSet::from([a,b])).unwrap();
        assert_eq!(doc.group_for(a), Some(gid));
        assert_eq!(doc.members_of(b).len(), 2);
        doc.node_mut(a).unwrap().collapsed = true;
        assert!(!layout(&doc).contains_key(&c));
        let restored: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        restored.validate().unwrap();
        assert!(restored.node(a).unwrap().collapsed);
        let mut restored = restored;
        assert!(restored.ungroup(a));
        assert!(restored.group_for(a).is_none());
    }
    #[test]
    fn color_and_multiple_selection_copy_are_independent() {
        let mut doc = Document::sample_design();
        let ids = doc.children(ROOT);
        doc.node_mut(ids[0]).unwrap().color = Some(2);
        doc.node_mut(ids[1]).unwrap().color = Some(2);
        let copies = ids.iter().take(2).map(|id| doc.copy_node_only(*id).unwrap()).collect::<Vec<_>>();
        let pasted = doc.paste_forest_after(ids[2], &copies);
        assert_eq!(pasted.len(), 2);
        assert!(pasted.iter().all(|id| doc.node(*id).unwrap().color == Some(2)));
        doc.validate().unwrap();
    }

}

#[cfg(test)]
mod link_regressions {
    use super::*;

    #[test]
    fn links_roundtrip_without_changing_tree_and_delete_with_node() {
        let mut doc = Document::sample_design();
        let a = doc.children(ROOT)[0];
        let b = doc.children(ROOT)[1];
        let before_parents = doc.nodes.iter().map(|n| (n.id, n.parent)).collect::<Vec<_>>();
        assert!(doc.add_link(a, b));
        assert!(!doc.add_link(b, a));
        assert!(!doc.add_link(a, a));
        assert_eq!(before_parents, doc.nodes.iter().map(|n| (n.id, n.parent)).collect::<Vec<_>>());
        let json = serde_json::to_string_pretty(&doc).unwrap();
        assert!(json.contains("\"links\""));
        let restored: Document = serde_json::from_str(&json).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.links.len(), 1);
        assert!(doc.delete_subtree(a));
        assert!(doc.links.is_empty());
        doc.validate().unwrap();
    }
    #[test]
    fn old_v1_without_links_and_invalid_refs() {
        let mut doc = Document::sample_design();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("\"links\""));
        let old: Document = serde_json::from_str(&json).unwrap();
        assert!(old.links.is_empty());
        let a = doc.children(ROOT)[0];
        doc.links.push(NodeLink {from: a, to: 99999});
        assert!(doc.validate().is_err());
    }
}
