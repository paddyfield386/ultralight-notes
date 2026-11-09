//! Transient dialogs and grid preferences.
use super::*;

impl MindmapApp {
    pub(super) fn grid_settings_window(&mut self, ctx: &egui::Context) {
        if !self.grid_settings_open { return; }
        let original = self.grid;
        let mut open = true;
        egui::Window::new(self.language.text("Grid Settings", "グリッド設定"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(370.0)
            .show(ctx, |ui| {
                ui.label(self.language.text("Adjust the canvas dot grid.", "キャンバスのドットグリッドを調整します。"));
                ui.add_space(10.0);
                ui.checkbox(&mut self.grid.visible, self.language.text("Show Grid", "グリッドを表示"));
                ui.add_enabled_ui(self.grid.visible, |ui| {
                    ui.add(egui::Slider::new(&mut self.grid.spacing, 16.0..=96.0)
                        .text(self.language.text("Dot Spacing", "ドット間隔")).suffix(" px"));
                    ui.add(egui::Slider::new(&mut self.grid.dot_radius, 0.6..=3.0)
                        .text(self.language.text("Dot Size", "ドットサイズ")).suffix(" px"));
                    ui.add(egui::Slider::new(&mut self.grid.contrast, 0..=100)
                        .text(self.language.text("Dot Contrast", "ドットのコントラスト")).suffix(" %"));
                    ui.checkbox(&mut self.grid.major_dots,
                        self.language.text("Show a major dot every five dots", "5個ごとに大きなドットを表示"));
                });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(self.language.text("Restore Defaults", "既定値に戻す")).clicked() {
                        self.grid = GridSettings::default();
                    }
                    ui.label(egui::RichText::new(self.language.text(
                        "Changes are saved automatically", "変更は自動的に保存されます")).small().weak());
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
            egui::Window::new(self.language.text("Unsaved Changes", "未保存の変更"))
                .collapsible(false).resizable(false).default_width(340.0)
                .show(ctx, |ui| {
                    let leaving = matches!(self.pending_action, Some(PendingAction::Exit));
                    ui.label(if leaving {
                        self.language.text("You have unsaved changes. Save before quitting?", "未保存の変更があります。終了前に保存しますか？")
                    } else {
                        self.language.text("You have unsaved changes. Save before continuing?", "未保存の変更があります。続行前に保存しますか？")
                    });
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        let save_label = if leaving {
                            self.language.text("Save and Quit", "保存して終了")
                        } else {
                            self.language.text("Save and Continue", "保存して続行")
                        };
                        if ui.add(egui::Button::new(save_label)
                            .fill(Color32::from_rgb(37, 99, 235))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(29, 78, 216)))).clicked() {
                            if self.save() { self.complete_pending_action(ctx); }
                        }
                        if ui.button(self.language.text("Discard Changes", "変更を破棄")).clicked() {
                            self.complete_pending_action(ctx);
                        }
                        if ui.button(self.language.text("Cancel", "キャンセル")).clicked() {
                            self.pending_action = None;
                            self.ask_discard = false;
                        }
                    });
                });
        }
        if !self.status.is_empty() {
            egui::Window::new(self.language.text("File Operations", "ファイル操作"))
                .collapsible(false).resizable(false).anchor(Align2::CENTER_TOP, [0.0, 80.0])
                .show(ctx, |ui| {
                    ui.label(&self.status);
                    if ui.button(self.language.text("Close", "閉じる")).clicked() { self.status.clear(); }
                });
        }
    }
}
