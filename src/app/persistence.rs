//! JSON file lifecycle, native dialogs and application settings.
use super::*;

impl MindmapApp {
    pub(super) fn save_preferences(&self) {
        if let Some(path) = preferences_path() {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() { return; }
            }
            let preferences = Preferences {
                recent_files: self.recent_files.clone(),
                grid: self.grid,
                dark: self.dark,
                reflow_on_add: self.reflow_on_add,
                snap_to_grid: self.snap_to_grid,
                language: self.language,
            };
            if let Ok(bytes) = serde_json::to_vec_pretty(&preferences) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }
    pub(super) fn remember(&mut self, path: PathBuf) {
        self.recent_files.retain(|old| old != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(12);
        self.save_preferences();
    }
    /// Returns true only when a complete JSON document has reached disk.
    /// A cancelled Save As or an I/O error must not allow a pending exit.
    pub(super) fn save_to(&mut self, path: PathBuf) -> bool {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        let path = ensure_json_extension(path);
        let result = self.doc.validate()
            .and_then(|_| serde_json::to_vec_pretty(&self.doc).map_err(|e| e.to_string()))
            .and_then(|data| write_json_atomic(&path, &data));
        match result {
            Ok(()) => {
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                self.current_path = Some(path.clone());
                self.remember(path);
                self.dirty = false;
                self.status.clear();
                true
            }
            Err(err) => {
                self.status = format!("Failed to save: {err}");
                false
            }
        }
    }
    pub(super) fn save(&mut self) -> bool {
        if let Some(path) = self.current_path.clone() { self.save_to(path) }
        else { self.open_file_dialog(FileDialog::SaveAs) }
    }
    pub(super) fn load_from(&mut self, path: PathBuf) {
        let result = std::fs::read_to_string(&path).map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Document>(&text).map_err(|e| e.to_string()))
            .and_then(|mut doc| {
                doc.validate()?;
                // Loading v1 never rewrites the file. A subsequent explicit save uses v2.
                doc.version = 2;
                Ok(doc)
            });
        match result {
            Ok(doc) => {
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                self.doc = doc; self.selected = self.doc.roots()[0];
                self.reset_canvas_ephemeral();
                self.selection = HashSet::from([self.selected]); self.current_path = Some(path.clone());
                self.remember(path); self.undo.clear(); self.redo.clear();
                self.dragging = None; self.drag_moving.clear(); self.middle_pan = false; self.drop_intent = None; self.editing = None;
                self.pan = Vec2::ZERO; self.zoom = 0.92; self.dirty = false;
                self.selected_text_box = None; self.editing_text_box = None;
                self.insert_text_box_mode = false;
                self.link_origin = None; self.selected_link = None; self.context_link = None;
                self.search_open = false; self.search_query.clear();
                self.status.clear();
            }
            Err(err) => { self.status = format!("Failed to load: {err}"); }
        }
    }
    pub(super) fn new_document(&mut self) {
        self.doc = Document::blank(); self.selected = ROOT;
        self.reset_canvas_ephemeral(); self.selection = HashSet::from([ROOT]);
        self.current_path = None; self.undo.clear(); self.redo.clear();
        self.dragging = None; self.drag_moving.clear(); self.middle_pan = false; self.drop_intent = None;
        self.editing = None; self.editing_text_box = None;
        self.selected_text_box = None; self.insert_text_box_mode = false;
        self.link_origin = None; self.selected_link = None; self.context_link = None;
        self.search_open = false; self.search_query.clear();
        self.pan = Vec2::ZERO; self.zoom = 0.92;
        self.dirty = false; self.start_edit(ROOT, false);
    }
    pub(super) fn request_new(&mut self) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        if self.dirty {
            self.pending_action = Some(PendingAction::New);
            self.ask_discard = true;
        } else { self.new_document(); }
    }
    pub(super) fn request_open(&mut self, path: PathBuf) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        if self.dirty {
            self.pending_action = Some(PendingAction::Open(path));
            self.ask_discard = true;
        } else { self.load_from(path); }
    }
    pub(super) fn request_exit(&mut self, ctx: &egui::Context) {
        self.finish_edit(true);
        self.finish_text_box_edit(true);
        if self.dirty {
            self.pending_action = Some(PendingAction::Exit);
            self.ask_discard = true;
        } else {
            self.approve_exit(ctx);
        }
    }
    pub(super) fn approve_exit(&mut self, ctx: &egui::Context) {
        // The native close-request event may arrive again on the next frame.
        // Without this guard, "Discard Changes" loops back into confirmation forever.
        self.exit_approved = true;
        self.save_preferences();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub(super) fn complete_pending_action(&mut self, ctx: &egui::Context) {
        let action = self.pending_action.take();
        self.ask_discard = false;
        match action {
            Some(PendingAction::Exit) => self.approve_exit(ctx),
            Some(PendingAction::Open(path)) => self.load_from(path),
            Some(PendingAction::New) => self.new_document(),
            None => {},
        }
    }
    pub(super) fn open_file_dialog(&mut self, mode: FileDialog) -> bool {
        // On Windows rfd uses the native Windows common file picker/save dialog.
        // Cancel returns None, leaving both the document and dirty flag unchanged.
        let mut dialog = rfd::FileDialog::new()
            .add_filter("MindMap JSON", &["json"]);
        if let Some(directory) = self.current_path.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(directory);
        }
        match mode {
            FileDialog::Open => {
                if let Some(path) = dialog.set_title("Open MindMap JSON").pick_file() {
                    self.request_open(path);
                }
                false
            }
            FileDialog::SaveAs => {
                let filename = self.current_path.as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| "MindMap.json".to_owned());
                match dialog.set_title("Save MindMap JSON")
                    .set_file_name(filename).save_file() {
                    Some(path) => self.save_to(path),
                    None => false, // Cancel: keep document, pending action and dirty flag.
                }
            }
        }
    }
}
