//! Regression tests for the desktop shell.
use super::*;

#[cfg(test)]
mod v04_app_tests {
    use super::*;

    #[test]
    fn deleting_initial_map_preserves_other_maps_and_can_create_on_empty_board(){
        let mut app=MindmapApp::new(Instant::now(),None,false);
        let other=app.doc.add_map("independent");
        app.select(ROOT);app.delete_selected();
        assert!(app.doc.node(ROOT).is_none());assert_eq!(app.doc.node(other).unwrap().title,"independent");
        app.select(other);app.delete_selected();assert!(app.doc.nodes.is_empty());
        assert_eq!(app.doc.validate(),Ok(()));
        let new_map=app.doc.add_map("new");assert_eq!(app.doc.node(new_map).unwrap().offset.x,0.0);
    }

    #[test]
    fn editor_keeps_positions_and_card_width_static_until_commit() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        let id = app.doc.children(ROOT)[0];
        let before = app.positions();
        let width = app.card_width(app.doc.node(id).unwrap(), app.doc.depth(id));
        app.start_edit(id, false);
        app.edit_text = "あいうえおかな漢字変換中の長い日本語入力テスト".repeat(8);
        app.ime_preedit = "未確定変換候補".to_owned();
        assert_eq!(before, app.positions(), "layout must not move during IME input");
        assert_eq!(width, app.card_width(app.doc.node(id).unwrap(), app.doc.depth(id)));
        app.finish_edit(true);
        assert_ne!(before, app.positions(), "commit may reflow the tree exactly once");
    }

    #[test]
    fn double_click_editor_selects_existing_title_only() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        let id = app.doc.children(ROOT)[0];
        app.start_edit(id, false);
        assert!(app.edit_select_all_pending);
        assert_eq!(app.edit_text, app.doc.node(id).unwrap().title);
        app.finish_edit(false);
        assert!(!app.edit_select_all_pending);
        app.add_child();
        assert!(app.edit_select_all_pending, "new placeholder should be selected for replacement");
        assert_eq!(app.edit_text, "New Topic");
    }

    #[test]
    fn text_box_background_and_outline_undo() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        let id = app.doc.add_text_box(10.0, 20.0);
        app.select_text_box(id);
        app.apply_text_box_color(false, Some([255, 239, 196]));
        app.apply_text_box_color(true, Some([220, 157, 44]));
        assert_eq!(app.doc.text_box(id).unwrap().fill_color, Some([255, 239, 196]));
        assert_eq!(app.doc.text_box(id).unwrap().border_color, Some([220, 157, 44]));
        app.undo();
        assert_eq!(app.doc.text_box(id).unwrap().border_color, None);
        app.undo();
        assert_eq!(app.doc.text_box(id).unwrap().fill_color, None);
        assert!(app.doc.text_box(id).unwrap().fill_transparent, "new annotations default to transparent");
        app.apply_text_box_color(false, Some([255, 239, 196]));
        assert!(!app.doc.text_box(id).unwrap().fill_transparent);
        app.apply_text_box_appearance(false, None, true);
        assert!(app.doc.text_box(id).unwrap().fill_transparent);
        app.undo();
        assert!(!app.doc.text_box(id).unwrap().fill_transparent);
        app.apply_text_box_color(true, Some([220, 157, 44]));
        assert!(!app.doc.text_box(id).unwrap().border_none);
        app.remove_text_box_border();
        assert!(app.doc.text_box(id).unwrap().border_none);
        assert_eq!(app.doc.text_box(id).unwrap().border_color, None);
        app.undo();
        assert!(!app.doc.text_box(id).unwrap().border_none);
    }

    #[test]
    fn free_text_box_edit_survives_json_and_undo() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        app.checkpoint();
        let id = app.doc.add_text_box(25.0, -90.0);
        app.start_text_box_edit(id, true);
        app.text_box_editor = "確認メモ\n日本語IME".to_owned();
        app.finish_text_box_edit(true);
        let saved = serde_json::to_string_pretty(&app.doc).unwrap();
        let loaded: Document = serde_json::from_str(&saved).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.text_box(id).unwrap().text, "確認メモ\n日本語IME");
        app.undo();
        assert!(app.doc.text_box(id).is_none());
    }
    #[test]
    fn shift_tab_navigates_to_parent_without_adding_a_node() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        let parent = app.doc.children(ROOT)[0];
        let child = app.doc.children(parent)[0];
        let count = app.doc.nodes.len();
        app.select(child);
        app.select_parent();
        assert_eq!(app.selected, parent);
        assert_eq!(app.doc.nodes.len(), count);
        app.select_parent();
        assert_eq!(app.selected, ROOT);
        app.select_parent();
        assert_eq!(app.selected, ROOT, "root has no parent");
    }

    #[test]
    fn shift_tab_commits_inline_edit_and_selects_parent() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        let parent = app.doc.children(ROOT)[0];
        let child = app.doc.children(parent)[0];
        app.start_edit(child, false);
        app.edit_text = "Edit中のタイトル".to_owned();
        app.select_parent();
        assert_eq!(app.selected, parent);
        assert!(app.editing.is_none());
        assert_eq!(app.doc.node(child).unwrap().title, "Edit中のタイトル");
    }

    #[test]
    fn test_app_uses_default_preferences_independent_of_user_profile() {
        let app = MindmapApp::new(Instant::now(), None, false);
        assert!(!app.reflow_on_add);
        assert!(!app.snap_to_grid);
        assert_eq!(app.grid, GridSettings::default());
    }

    #[test]
    fn add_child_with_reflow_off_keeps_all_existing_visible_positions() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        assert!(!app.reflow_on_add);
        app.select(ROOT);
        let old = app.positions();
        app.add_child();
        let current = app.positions();
        assert_eq!(app.doc.nodes.len(), old.len() + 1);
        for (id, old_pos) in old {
            let new_pos = current.get(&id).unwrap();
            assert!((new_pos.x - old_pos.x).abs() < 0.01, "x moved for {id}");
            assert!((new_pos.y - old_pos.y).abs() < 0.01, "y moved for {id}");
        }
    }

    #[test]
    fn renaming_new_topic_with_reflow_off_does_not_move_existing_nodes() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        app.select(ROOT);
        app.add_child();
        let new_id = app.selected;
        let before = app.positions();
        app.edit_text = "日本語のタイトルを十分に長く入力して確認する".repeat(5);
        app.finish_edit(true);
        let after = app.positions();
        for (id, old_pos) in before {
            let new_pos = after.get(&id).unwrap();
            assert!((new_pos.x - old_pos.x).abs() < 0.01 && (new_pos.y - old_pos.y).abs() < 0.01,
                "position moved for {id}");
        }
        assert!(app.doc.node(new_id).unwrap().title.starts_with("日本語"));
    }

    #[test]
    fn add_child_with_reflow_on_recalculates_existing_positions() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        app.reflow_on_add = true;
        app.select(ROOT);
        let old = app.positions();
        app.add_child();
        let current = app.positions();
        assert!(old.iter().any(|(id, old_pos)|
            current.get(id).is_some_and(|p| (p.y - old_pos.y).abs() > 0.01)));
    }
}

#[cfg(test)]
mod preference_tests {
    use super::*;

    #[test]
    fn old_recent_files_settings_migrate_with_default_grid() {
        let old = r#"{"recent_files":[]}"#;
        let prefs: Preferences = serde_json::from_str(old).unwrap();
        assert_eq!(prefs.grid, GridSettings::default());
        assert!(!prefs.dark);
        assert!(!prefs.reflow_on_add, "automatic reflow must default to OFF");
    }

    #[test]
    fn default_grid_matches_product_spec() {
        let grid = GridSettings::default();
        assert!(grid.visible);
        assert_eq!(grid.spacing, 20.0);
        assert_eq!(grid.dot_radius, 1.0);
        assert_eq!(grid.contrast, 30);
        assert!(grid.major_dots);
    }

    #[test]
    fn grid_settings_round_trip_and_validate() {
        let prefs = Preferences {
            recent_files: Vec::new(),
            grid: GridSettings { visible: true, spacing: 42.0, dot_radius: 2.1, contrast: 80, major_dots: false },
            dark: true,
            reflow_on_add: true,
            snap_to_grid: false,
            language: Language::English,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let decoded: Preferences = serde_json::from_str(&json).unwrap();
        assert_eq!(prefs.grid, decoded.grid);
        assert!(decoded.dark);
        assert!(decoded.reflow_on_add);
        let unsafe_settings = GridSettings { spacing: f32::INFINITY, dot_radius: f32::NAN, contrast: 255, ..prefs.grid };
        let safe = unsafe_settings.sanitized();
        assert!(safe.spacing.is_finite() && safe.dot_radius.is_finite());
        assert!(safe.contrast <= 100);
    }

    #[test]
    fn atomic_json_write_does_not_clobber_on_failure() {
        let root = std::env::temp_dir().join(format!("mindmap-v36-{}-{}", std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("board.json");
        write_json_atomic(&path, b"{\"version\":1}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"version\":1}");
        let invalid = root.join("missing").join("board.json");
        assert!(write_json_atomic(&invalid, b"bad").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"version\":1}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn all_save_paths_use_json_extension() {
        assert_eq!(ensure_json_extension(PathBuf::from("plan")), PathBuf::from("plan.json"));
        assert_eq!(ensure_json_extension(PathBuf::from("plan.txt")), PathBuf::from("plan.json"));
        assert_eq!(ensure_json_extension(PathBuf::from("plan.JSON")), PathBuf::from("plan.JSON"));
    }
}

#[cfg(test)]
mod search_and_association_regressions {
    use super::*;

    #[test]
    fn search_finds_nodes_and_reveals_collapsed_parent() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        app.search_query = "Requirements".into();
        let matches = app.search_matches();
        assert!(!matches.is_empty());
        let id = matches[0];
        let parent = app.doc.parent(id).unwrap();
        app.doc.node_mut(parent).unwrap().collapsed = true;
        assert!(!app.positions().contains_key(&id));
        app.jump_to_match(id);
        assert_eq!(app.selected, id);
        assert!(!app.doc.node(parent).unwrap().collapsed);
        assert!(app.positions().contains_key(&id));
        assert!(app.center_pending);
    }

    #[test]
    fn link_has_undo_and_redo_without_reparenting() {
        let mut app = MindmapApp::new(Instant::now(), None, false);
        let ids = app.doc.children(ROOT);
        let (a, b) = (ids[0], ids[1]);
        let parents = (app.doc.parent(a), app.doc.parent(b));
        app.create_link(a, b);
        assert_eq!(app.doc.links.len(), 1);
        assert_eq!((app.doc.parent(a), app.doc.parent(b)), parents);
        app.undo();
        assert!(app.doc.links.is_empty());
        app.redo();
        assert_eq!(app.doc.links.len(), 1);
        app.delete_selected_link();
        assert!(app.doc.links.is_empty());
    }
}
