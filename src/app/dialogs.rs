//! Transient dialogs and grid preferences.
use super::*;

impl MindmapApp {
    pub(super) fn grid_settings_window(&mut self, ctx: &egui::Context) {
        if !self.grid_settings_open { return; }
        let original = self.grid;
        let mut open = true;
        egui::Window::new("Grid Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(370.0)
            .show(ctx, |ui| {
                ui.label("Adjust the canvas dot grid.");
                ui.add_space(10.0);
                ui.checkbox(&mut self.grid.visible, "Show Grid");
                ui.add_enabled_ui(self.grid.visible, |ui| {
                    ui.add(egui::Slider::new(&mut self.grid.spacing, 16.0..=96.0)
                        .text("Dot Spacing").suffix(" px"));
                    ui.add(egui::Slider::new(&mut self.grid.dot_radius, 0.6..=3.0)
                        .text("Dot Size").suffix(" px"));
                    ui.add(egui::Slider::new(&mut self.grid.contrast, 0..=100)
                        .text("Dot Contrast").suffix(" %"));
                    ui.checkbox(&mut self.grid.major_dots, "Show a major dot every five dots");
                });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Restore Defaults").clicked() {
                        self.grid = GridSettings::default();
                    }
                    ui.label(egui::RichText::new("Changes are saved automatically").small().weak());
                });
            });
        self.grid = self.grid.sanitized();
        if self.grid != original {
            self.save_preferences();
            ctx.request_repaint();
        }
        self.grid_settings_open = open;
    }
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.grid_settings_window(ctx);
        if self.ask_discard {
            egui::Window::new("未Saveの変更")
                .collapsible(false).resizable(false).default_width(340.0)
                .show(ctx, |ui| {
                    let leaving = matches!(self.pending_action, Some(PendingAction::Exit));
                    ui.label(if leaving {
                        "You have unsaved changes. Save before quitting?"
                    } else {
                        "You have unsaved changes. Save before continuing?"
                    });
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui.button(if leaving { "Save and Quit" } else { "Save and Continue" }).clicked() {
                            // Only a successful write may trigger close/open/new.
                            if self.save() { self.complete_pending_action(ctx); }
                        }
                        if ui.button("Discard Changes").clicked() {
                            self.complete_pending_action(ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_action = None;
                            self.ask_discard = false;
                        }
                    });
                });
        }
        if !self.status.is_empty() {
            // File errors and success notifications are transient overlays, not a permanent status panel.
            egui::Window::new("File Operations")
                .collapsible(false).resizable(false).anchor(Align2::CENTER_TOP, [0.0, 80.0])
                .show(ctx, |ui| {
                    ui.label(&self.status);
                    if ui.button("Close").clicked() { self.status.clear(); }
                });
        }
    }
}
