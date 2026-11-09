//! Responsive application ribbon and icon presentation.
use super::*;

/// Responsive ribbon rules are pure and can be unit-tested without a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RibbonPlan {
    compact: bool,
    inline_palette: bool,
    secondary: bool,
    narrow: bool,
}

impl RibbonPlan {
    fn for_width(width: f32) -> Self {
        let width = if width.is_finite() { width.max(0.0) } else { 0.0 };
        Self {
            compact: width < 1450.0,
            inline_palette: width >= 930.0,
            secondary: width >= 1110.0,
            narrow: width < 865.0,
        }
    }
}

impl MindmapApp {
    fn ribbon_tool_active(&self, icon: &'static str) -> bool {
        match icon {
            "sticky" => self.insert_shape_mode == Some(ShapeKind::StickyNote),
            "frame" => self.insert_shape_mode == Some(ShapeKind::Frame),
            "rect" => self.insert_shape_mode == Some(ShapeKind::Rectangle),
            "rounded" => self.insert_shape_mode == Some(ShapeKind::RoundedRectangle),
            "ellipse" => self.insert_shape_mode == Some(ShapeKind::Ellipse),
            "diamond" => self.insert_shape_mode == Some(ShapeKind::Diamond),
            "insert_text" => self.insert_text_box_mode,
            "line" => self.insert_line_mode,
            "connect" => self.insert_connector_mode && self.insert_connector_route == ConnectorRoute::Straight,
            "orthogonal" => self.insert_connector_mode && self.insert_connector_route == ConnectorRoute::Orthogonal,
            _ => false,
        }
    }

    fn shape_menu_item(&self, ui: &mut egui::Ui, label: &str, kind: ShapeKind) -> bool {
        let mut clicked = false;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 24.0), Sense::hover());
            Self::draw_shape_outline(ui.painter(), rect.shrink(3.0), kind, Stroke::new(1.25, ACCENT));
            if ui.button(self.language.ribbon_text(label)).clicked() { clicked = true; }
        });
        clicked
    }

    fn set_selected_text_alignment(&mut self, horizontal: Option<TextHorizontal>, vertical: Option<TextVertical>) {
        let targets: Vec<_> = self.canvas_selection.iter().copied().filter(|target|
            matches!(target, CanvasTarget::Shape(_) | CanvasTarget::TextBox(_))).collect();
        if targets.is_empty() { return; }
        self.checkpoint();
        for target in targets {
            match target {
                CanvasTarget::Shape(id) => if let Some(shape) = self.doc.shape_mut(id) {
                    if let Some(value) = horizontal { shape.style.text_horizontal = value; }
                    if let Some(value) = vertical { shape.style.text_vertical = value; }
                },
                CanvasTarget::TextBox(id) => if let Some(text_box) = self.doc.text_box_mut(id) {
                    if let Some(value) = horizontal { text_box.text_horizontal = value; }
                    if let Some(value) = vertical { text_box.text_vertical = value; }
                },
                _ => {}
            }
        }
    }

    /// Word-style three-line alignment icon. Horizontal alignment shifts the
    /// line endings; vertical alignment shifts the three-line block itself.
    fn text_alignment_icon_button(
        &self,
        ui: &mut egui::Ui,
        horizontal: Option<TextHorizontal>,
        vertical: Option<TextVertical>,
        tooltip: &str,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(23.0, 19.0), Sense::click());
        let hovered = response.hovered();
        let painter = ui.painter();
        if hovered {
            painter.rect_filled(rect.shrink(1.0), 3.0, if self.dark {
                Color32::from_rgb(65, 71, 91)
            } else {
                Color32::from_rgb(232, 235, 254)
            });
        }
        let color = if self.dark { Color32::from_rgb(236, 241, 255) } else { Color32::from_rgb(79, 90, 134) };
        let stroke = Stroke::new(1.6, color);
        let widths = [8.0, 14.0, 10.5];
        let y_start = match vertical {
            Some(TextVertical::Top) => rect.top() + 3.5,
            Some(TextVertical::Center) => rect.center().y - 3.0,
            Some(TextVertical::Bottom) => rect.bottom() - 9.5,
            None => rect.center().y - 3.0,
        };
        for (index, width) in widths.into_iter().enumerate() {
            let x = match horizontal {
                Some(TextHorizontal::Left) => rect.left() + 4.0,
                Some(TextHorizontal::Center) | None => rect.center().x - width * 0.5,
                Some(TextHorizontal::Right) => rect.right() - 4.0 - width,
            };
            let y = y_start + index as f32 * 3.0;
            painter.line_segment([Pos2::new(x, y), Pos2::new(x + width, y)], stroke);
        }
        let clicked = response.clicked();
        response.on_hover_text(tooltip);
        clicked
    }

    /// Compact controls shared by the Home ribbon and a shape's context menu.
    pub(super) fn text_alignment_menu(&mut self, ui: &mut egui::Ui) {
        ui.label(self.language.text("Horizontal", "横位置"));
        ui.horizontal(|ui| {
            for (hint, value) in [("Left", TextHorizontal::Left), ("Center", TextHorizontal::Center), ("Right", TextHorizontal::Right)] {
                if self.text_alignment_icon_button(ui, Some(value), None, self.language.ribbon_text(hint)) {
                    self.set_selected_text_alignment(Some(value), None); ui.close();
                }
            }
        });
        ui.label(self.language.text("Vertical", "縦位置"));
        ui.horizontal(|ui| {
            for (hint, value) in [("Top", TextVertical::Top), ("Middle", TextVertical::Center), ("Bottom", TextVertical::Bottom)] {
                if self.text_alignment_icon_button(ui, None, Some(value), self.language.ribbon_text(hint)) {
                    self.set_selected_text_alignment(None, Some(value)); ui.close();
                }
            }
        });
    }

    fn ribbon_text_alignment(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(1.0, 1.0);
            ui.horizontal(|ui| for (hint, value) in [("Left", TextHorizontal::Left), ("Center", TextHorizontal::Center), ("Right", TextHorizontal::Right)] {
                if self.text_alignment_icon_button(ui, Some(value), None, self.language.ribbon_text(hint)) { self.set_selected_text_alignment(Some(value), None); }
            });
            ui.horizontal(|ui| for (hint, value) in [("Top", TextVertical::Top), ("Middle", TextVertical::Center), ("Bottom", TextVertical::Bottom)] {
                if self.text_alignment_icon_button(ui, None, Some(value), self.language.ribbon_text(hint)) { self.set_selected_text_alignment(None, Some(value)); }
            });
        });
    }

    /// Dark-theme swatches preview the dark canvas rather than the pastel
    /// colors used by the light theme. The palette index stored in the document
    /// stays unchanged when switching themes.
    fn ribbon_palette_colors(&self, index: usize) -> (Color32, Color32) {
        if !self.dark {
            return (TINTS[index].fill, TINTS[index].border);
        }
        const DARK_SWATCHES: [Color32; 8] = [
            Color32::from_rgb(39, 68, 112), // blue
            Color32::from_rgb(34, 86, 65),  // green
            Color32::from_rgb(75, 51, 119), // purple
            Color32::from_rgb(109, 81, 36), // yellow
            Color32::from_rgb(110, 53, 76), // pink
            Color32::from_rgb(82, 59, 115), // lavender
            Color32::from_rgb(29, 86, 89),  // cyan
            Color32::from_rgb(64, 75, 94),  // gray
        ];
        (DARK_SWATCHES[index], TINTS[index].line)
    }
    pub(super) fn icon(&mut self, ui: &egui::Ui, name: &'static str) -> egui::TextureId {
        let bytes: &'static [u8] = match name {
            "undo" => include_bytes!("../../assets/tabler/undo.rgba"),
            "redo" => include_bytes!("../../assets/tabler/redo.rgba"),
            "child" => include_bytes!("../../assets/tabler/child.rgba"),
            "sibling" => include_bytes!("../../assets/tabler/sibling.rgba"),
            "delete" => include_bytes!("../../assets/tabler/delete.rgba"),
            "fold" => include_bytes!("../../assets/tabler/fold.rgba"),
            "unfold" => include_bytes!("../../assets/tabler/unfold.rgba"),
            "align_branch" => include_bytes!("../../assets/tabler/align_branch.rgba"),
            "align_all" => include_bytes!("../../assets/tabler/align_all.rgba"),
            "fit" => include_bytes!("../../assets/tabler/fit.rgba"),
            "focus" => include_bytes!("../../assets/tabler/focus.rgba"),
            "actual" => include_bytes!("../../assets/tabler/actual.rgba"),
            "new_map" => include_bytes!("../../assets/tabler/new_map.rgba"),
            "group" => include_bytes!("../../assets/tabler/group.rgba"),
            "link" => include_bytes!("../../assets/tabler/link.rgba"),
            "ungroup" => include_bytes!("../../assets/tabler/ungroup.rgba"),
            "palette" => include_bytes!("../../assets/tabler/palette.rgba"),
            "new_file" => include_bytes!("../../assets/tabler/new_file.rgba"),
            "open_file" => include_bytes!("../../assets/tabler/open_file.rgba"),
            "save_file" => include_bytes!("../../assets/tabler/save_file.rgba"),
            "insert_text" => include_bytes!("../../assets/tabler/insert_text.rgba"),
            "pure_json" | "pure_json_selection" => include_bytes!("../../assets/tabler/pure_json.rgba"),
            "rect" => include_bytes!("../../assets/tabler/rect.rgba"),
            "rounded" => include_bytes!("../../assets/tabler/rounded.rgba"),
            "ellipse" => include_bytes!("../../assets/tabler/ellipse.rgba"),
            "connect" => include_bytes!("../../assets/tabler/connect.rgba"),
            "sticky" => include_bytes!("../../assets/tabler/sticky.rgba"),
            "line" => include_bytes!("../../assets/tabler/line.rgba"),
            "frame" => include_bytes!("../../assets/tabler/frame.rgba"),
            "diamond" => include_bytes!("../../assets/tabler/diamond.rgba"),
            "orthogonal" => include_bytes!("../../assets/tabler/orthogonal.rgba"),
            _ => unreachable!("unknown ribbon icon"),
        };
        self.icons.entry(name).or_insert_with(|| {
            // Tabler source icons are black alpha masks. Change RGB to white so
            // painter.image's tint works in both light and dark theme.
            let mut rgba=bytes.to_vec();
            for pixel in rgba.chunks_exact_mut(4) {pixel[0]=255;pixel[1]=255;pixel[2]=255;}
            let image = egui::ColorImage::from_rgba_unmultiplied([48, 48], &rgba);
            ui.ctx().load_texture(format!("ribbon-{name}"), image, egui::TextureOptions::LINEAR)
        }).id()
    }
    /// Office-like compact tile. The ribbon becomes icon-only on narrow windows.
    pub(super) fn ribbon_button(&mut self, ui: &mut egui::Ui, icon: &'static str, label: &str, enabled: bool) -> bool {
        let label = self.language.ribbon_text(label);
        let texture = self.icon(ui, icon);
        let active = self.ribbon_tool_active(icon);
        let w = if self.ribbon_compact { 38.0 } else { 62.0 };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(w, 65.0), Sense::click());
        let painter = ui.painter();
        if (response.hovered() || active) && enabled {
            painter.rect_filled(rect.shrink(2.0), 6.0,
                if active {
                    if self.dark { Color32::from_rgb(82, 74, 135) } else { Color32::from_rgb(213, 207, 255) }
                } else if self.dark { Color32::from_rgb(65, 71, 91) }
                else { Color32::from_rgb(232, 235, 254) });
        }
        let icon_center = Pos2::new(rect.center().x, if self.ribbon_compact { rect.center().y } else { rect.top() + 23.0 });
        painter.image(texture,
            Rect::from_center_size(icon_center, egui::vec2(24.0, 24.0)),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            if !enabled {Color32::from_gray(130)}
            else if active { ACCENT }
            else if self.dark {Color32::from_rgb(236,241,255)}
            else {Color32::from_rgb(79,90,134)});
        if !self.ribbon_compact {
            // Labels in Japanese can be substantially wider than the English
            // originals. Lay them out inside their tile and clip the result so
            // neighboring ribbon labels can never overlap.
            let color = if !enabled { Color32::from_gray(170) }
                else if self.dark { Color32::WHITE } else { INK };
            let font_size = if label.chars().count() > 9 { 8.0 } else { 9.5 };
            let label_painter = painter.with_clip_rect(rect.shrink(2.0));
            let galley = label_painter.layout(label.to_owned(), FontId::proportional(font_size), color, w - 6.0);
            let pos = Pos2::new(rect.center().x - galley.size().x * 0.5, rect.bottom() - galley.size().y - 3.0);
            label_painter.galley(pos, galley, color);
        }
        response.on_hover_text(label).clicked() && enabled
    }
    pub(super) fn ribbon_group(ui: &mut egui::Ui, width: f32, label: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
        ui.allocate_ui_with_layout(egui::vec2(width, 82.0),
            egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.set_min_width(width);
                ui.set_max_width(width);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    add_contents(ui);
                });
                ui.add_space(1.0);
                ui.label(egui::RichText::new(label).size(10.0).color(Color32::from_gray(136)));
            });
    }
    pub(super) fn ribbon_view_settings(&mut self, ui: &mut egui::Ui) {
        if ui.checkbox(&mut self.grid.visible, self.language.ribbon_text("Show Dot Grid")).changed() { self.save_preferences(); }
        if ui.button(self.language.ribbon_text("Grid Settings…")).clicked() { self.grid_settings_open = true; ui.close(); }
        ui.separator();
        if ui.checkbox(&mut self.reflow_on_add, self.language.ribbon_text("Automatically rearrange when adding")).changed() { self.save_preferences(); }
        ui.label(egui::RichText::new(self.language.text("Off by default. Existing node positions are preserved.", "既定ではオフです。既存ノードの位置は維持されます。")).small().weak());
        ui.separator();
        if ui.checkbox(&mut self.snap_to_grid, self.language.ribbon_text("Snap to Grid")).changed() {self.save_preferences();}
        ui.separator();
        if ui.button(if self.dark { self.language.ribbon_text("Light Theme") } else { self.language.ribbon_text("Dark Theme") }).clicked() {
            self.dark = !self.dark;
            ui.ctx().set_theme(if self.dark { egui::Theme::Dark } else { egui::Theme::Light });
            self.save_preferences();
            ui.close();
        }
    }
    pub(super) fn ribbon_color_menu(&mut self, ui: &mut egui::Ui) {
        if self.selected_shape.is_some(){
            for (i,label) in COLOR_LABELS.iter().enumerate(){
                let (swatch, border) = self.ribbon_palette_colors(i);
                if Self::color_tile(ui,swatch,border,label){
                    self.draw_selected_sticky_color(rgb_of(TINTS[i].fill));ui.close();
                }
            }
        } else if self.selected_text_box.is_some() {
            ui.menu_button("Background Color", |ui| self.text_box_palette(ui, false));
            ui.menu_button("Border Color", |ui| self.text_box_palette(ui, true));
        } else {
            if ui.button("Automatic (Branch Color)").clicked() { self.apply_color(None); ui.close(); }
            ui.separator();
            for row in 0..2 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for i in row*4..(row+1)*4 {
                        let (swatch, border) = self.ribbon_palette_colors(i);
                        if Self::color_tile(ui, swatch, border, COLOR_LABELS[i]) {
                            self.apply_color(Some(i as u8)); ui.close();
                        }
                    }
                });
            }
        }
    }
    /// A two-row, directly clickable color palette. Pop-up menus are used only
    /// when the available window width cannot accommodate this palette.
    fn ribbon_inline_palette(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
            for row in 0..2 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for index in (row * 4)..(row * 4 + 4) {
                        let color = TINTS[index].fill;
                        let (swatch, border) = self.ribbon_palette_colors(index);
                        if Self::color_tile(ui, swatch, border, COLOR_LABELS[index]) {
                            if self.selected_shape.is_some() || self.selected_text_box.is_some() {
                                self.draw_selected_sticky_color(rgb_of(color));
                            } else if self.selected_text_box.is_some() {
                                self.apply_text_box_color(false, Some(rgb_of(color)));
                            } else {
                                self.apply_color(Some(index as u8));
                            }
                        }
                    }
                });
            }
        });
    }
    /// A responsive two-tab ribbon whose content never requests a scroll area.
    /// All operations fit at the current width by reducing label density and
    /// moving less frequent actions into the More menu when necessary.
    pub(super) fn ribbon(&mut self, ui: &mut egui::Ui) {
        let tabs = [
            self.language.text("Home", "ホーム"),
            self.language.text("Insert", "挿入"),
        ];
        let full_width = ui.available_width();
        let plan = RibbonPlan::for_width(full_width);
        self.ribbon_compact = plan.compact;
        let tile = if self.ribbon_compact { 38.0 } else { 62.0 };
        let group_width = |count: f32| count * (tile + 1.0) + 4.0;
        let has_selection = !self.selection.is_empty() && self.selected_text_box.is_none();
        let can_delete = self.selected_text_box.is_some() || self.selected_link.is_some() || !self.selection.is_empty();
        let clip = ui.clip_rect();
        let ribbon_top = ui.cursor().top();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(egui::RichText::new("MindMap").strong().size(16.0));
            ui.separator();
            for (index, name) in tabs.iter().enumerate() {
                let active = self.ribbon_tab == index;
                let color = if active { ACCENT } else if self.dark { Color32::LIGHT_GRAY } else { INK };
                let tab = ui.add(egui::Button::new(egui::RichText::new(*name).size(13.0).color(color).strong())
                    .selected(active).frame(false));
                if active {
                    let y = tab.rect.bottom() + 2.0;
                    ui.painter().line_segment([
                        Pos2::new(tab.rect.left() + 6.0, y),
                        Pos2::new(tab.rect.right() - 6.0, y),
                    ], Stroke::new(2.4, ACCENT));
                }
                if tab.clicked() { self.ribbon_tab = index; }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = format!("{}{}", self.filename(), if self.dirty { " *" } else { "" });
                ui.label(egui::RichText::new(name).size(11.0).color(if self.dark {
                    Color32::LIGHT_GRAY
                } else { Color32::from_gray(100) }));
            });
        });
        ui.add_space(3.0);
        let dark = self.dark;
        egui::Frame::new()
            .fill(if dark { Color32::from_rgb(34, 40, 55) } else { Color32::WHITE })
            .corner_radius(9)
            .stroke(Stroke::new(1.0, if dark { Color32::from_gray(72) } else { Color32::from_rgb(227, 231, 243) }))
            .inner_margin(egui::Margin::symmetric(6, 5))
            .show(ui, |ui| {
                // Stretch the bar to its parent width; the actions themselves
                // stay compact rather than stretching individual groups.
                ui.set_min_width((full_width - 12.0).max(0.0));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    if self.ribbon_tab == 0 {
                        Self::ribbon_group(ui, group_width(3.0) + 28.0, self.language.ribbon_text("File"), |ui| {
                            if self.ribbon_button(ui, "new_file", "New", true) { self.request_new(); }
                            if self.ribbon_button(ui, "open_file", "Open", true) { self.open_file_dialog(FileDialog::Open); }
                            if self.ribbon_button(ui, "save_file", "Save", true) { self.save(); }
                            ui.menu_button("▾", |ui| {
                                if ui.button("Save As…").clicked() {
                                    self.open_file_dialog(FileDialog::SaveAs);
                                    ui.close();
                                }
                            });
                        });
                        ui.separator();
                        Self::ribbon_group(ui, group_width(2.0), self.language.ribbon_text("Edit"), |ui| {
                            if self.ribbon_button(ui, "undo", "Undo", !self.undo.is_empty()) { self.undo(); }
                            if self.ribbon_button(ui, "redo", "Redo", !self.redo.is_empty()) { self.redo(); }
                        });
                        ui.separator();
                        Self::ribbon_group(ui, group_width(3.0), self.language.ribbon_text("Node"), |ui| {
                            if self.ribbon_button(ui, "child", "Add Child", has_selection) { self.add_child(); }
                            if self.ribbon_button(ui, "sibling", "Add Sibling", has_selection) { self.add_sibling(); }
                            if self.ribbon_button(ui, "delete", "Delete", can_delete) { self.delete_selected(); }
                        });
                        if !plan.narrow {
                            ui.separator();
                            Self::ribbon_group(ui, group_width(2.0), self.language.ribbon_text("Arrange"), |ui| {
                                if self.ribbon_button(ui, "align_branch", "Arrange Branch", has_selection) { self.align_selection(); }
                                if self.ribbon_button(ui, "align_all", "Arrange All", true) { self.reset_all_layout(); }
                            });
                        }
                        ui.separator();
                        Self::ribbon_group(ui, 82.0, self.language.text("Text alignment", "文字揃え"), |ui| self.ribbon_text_alignment(ui));
                        if !plan.narrow {
                            ui.separator();
                            Self::ribbon_group(ui, group_width(3.0), self.language.ribbon_text("View"), |ui| {
                                if self.ribbon_button(ui, "fit", "Fit All", true) { self.fit_pending = true; }
                                if self.ribbon_button(ui, "focus", "Center on Selection", has_selection || self.selected_text_box.is_some()) { self.center_pending = true; }
                                if self.ribbon_button(ui, "actual", "100%", true) { self.zoom = 1.0; }
                            });
                        }
                        if plan.secondary {
                            ui.separator();
                            Self::ribbon_group(ui, group_width(2.0), self.language.ribbon_text("Branch"), |ui| {
                                if self.ribbon_button(ui, "fold", "Collapse", has_selection) { self.toggle_collapse(true); }
                                if self.ribbon_button(ui, "unfold", "Expand", has_selection) { self.toggle_collapse(false); }
                            });
                            ui.separator();
                            Self::ribbon_group(ui, group_width(2.0), self.language.ribbon_text("For AI"), |ui| {
                                if self.ribbon_button(ui, "pure_json", "Structured JSON", true) {
                                    self.pure_json_to_clipboard(ui.ctx(), false);
                                }
                                let has_canvas_selection=!self.canvas_selection.is_empty();
                                if self.ribbon_button(ui, "pure_json_selection", "Selection JSON", has_canvas_selection) {
                                    self.pure_json_to_clipboard(ui.ctx(), true);
                                }
                            });
                        }
                        ui.separator();
                        if plan.inline_palette {
                            Self::ribbon_group(ui, 114.0, self.language.ribbon_text("Color"), |ui| self.ribbon_inline_palette(ui));
                        } else {
                            Self::ribbon_group(ui, 77.0, self.language.ribbon_text("Color"), |ui| {
                                ui.menu_button(self.language.ribbon_text("Color ▾"), |ui| self.ribbon_color_menu(ui));
                            });
                        }
                        ui.separator();
                        Self::ribbon_group(ui, 77.0, self.language.ribbon_text("View"), |ui| {
                            ui.menu_button(self.language.text("More ▾", "その他 ▾"), |ui| {
                                if plan.narrow {
                                    if ui.button(self.language.ribbon_text("Arrange Branch")).clicked() { self.align_selection(); ui.close(); }
                                    if ui.button(self.language.ribbon_text("Arrange All")).clicked() { self.reset_all_layout(); ui.close(); }
                                    ui.separator();
                                    if ui.button(self.language.ribbon_text("Fit All")).clicked() { self.fit_pending = true; ui.close(); }
                                    if ui.button(self.language.ribbon_text("Center on Selection")).clicked() { self.center_pending = true; ui.close(); }
                                    if ui.button("100%").clicked() { self.zoom = 1.0; ui.close(); }
                                    ui.separator();
                                }
                                if !plan.secondary {
                                    if ui.button(self.language.ribbon_text("Collapse")).clicked() { self.toggle_collapse(true); ui.close(); }
                                    if ui.button(self.language.ribbon_text("Expand")).clicked() { self.toggle_collapse(false); ui.close(); }
                                    if ui.button(self.language.ribbon_text("Copy Structured JSON")).clicked() {
                                        self.pure_json_to_clipboard(ui.ctx(), false);
                                        ui.close();
                                    }
                                    if ui.button(self.language.ribbon_text("Copy Selection JSON")).clicked() {
                                        self.pure_json_to_clipboard(ui.ctx(), true);
                                        ui.close();
                                    }
                                    ui.separator();
                                }
                                ui.separator();
                                if ui.button(self.language.ribbon_text("Find Node…  Ctrl+F")).clicked() {
                                    self.search_open = true;
                                    self.search_focus_pending = true;
                                    ui.close();
                                }
                                ui.menu_button(self.language.ribbon_text("Export Image"), |ui| {
                                    if ui.button(self.language.ribbon_text("Save as PNG…")).clicked() { self.export_file(true); ui.close(); }
                                    if ui.button(self.language.ribbon_text("Save as SVG…")).clicked() { self.export_file(false); ui.close(); }
                                    if ui.button(self.language.ribbon_text("Copy Image")).clicked() { self.copy_image(); ui.close(); }
                                });
                                self.ribbon_view_settings(ui);
                                ui.separator();
                                ui.menu_button(self.language.ribbon_text("Language"), |ui| {
                                    for language in [Language::English, Language::Japanese] {
                                        if ui.selectable_label(self.language == language, language.label()).clicked() {
                                            self.language = language;
                                            self.save_preferences();
                                            ui.close();
                                        }
                                    }
                                });
                                ui.separator();
                                if self.selected_shape.is_some() || self.selected_text_box.is_some() {
                                    ui.menu_button(self.language.text("Text alignment", "文字揃え"), |ui| self.text_alignment_menu(ui));
                                    ui.separator();
                                }
                                ui.menu_button(self.language.ribbon_text("Align"),|ui|self.arrangement_menu(ui));
                                ui.menu_button(self.language.ribbon_text("Distribute Evenly"),|ui|{
                                    if ui.button(self.language.ribbon_text("Distribute Horizontally")).clicked(){self.arrange_selected(brainstorm::Arrangement::EvenX);ui.close();}
                                    if ui.button(self.language.ribbon_text("Distribute Vertically")).clicked(){self.arrange_selected(brainstorm::Arrangement::EvenY);ui.close();}
                                });
                                if ui.button(self.language.ribbon_text("Create Frame from Selection")).clicked(){self.create_frame_for_selection();ui.close();}
                                if ui.button(self.language.ribbon_text("Remove from Frame")).clicked(){self.detach_selected_from_frames();ui.close();}
                                ui.separator();
                                if ui.button(self.language.ribbon_text("Duplicate Ctrl+D")).clicked(){self.duplicate_canvas_selection();ui.close();}
                                ui.menu_button(self.language.ribbon_text("Arrange Order"), |ui| {
                                    for (label,d) in [("Bring to Front",2),("Bring Forward",1),("Send Backward",-1),("Send to Back",-2)] {
                                        if ui.button(self.language.ribbon_text(label)).clicked() {self.change_selected_shape_z(d);ui.close();}
                                    }
                                });
                            });
                        });
                    } else {
                        Self::ribbon_group(ui,group_width(1.0),self.language.ribbon_text("Sticky Note"),|ui| {
                            if self.ribbon_button(ui,"sticky","Sticky Note",true){
                                self.insert_text_box_mode=false;self.insert_line_mode=false;
                                self.insert_shape_mode=Some(ShapeKind::StickyNote);
                            }
                        });
                        ui.separator();
                        Self::ribbon_group(ui,group_width(1.0),self.language.ribbon_text("Frame"),|ui|{
                            if self.ribbon_button(ui,"frame","Frame",true){
                                self.insert_shape_mode=Some(ShapeKind::Frame);self.insert_line_mode=false;
                            }
                        });
                        ui.separator();
                        Self::ribbon_group(ui, group_width(2.0), self.language.ribbon_text("Canvas"), |ui| {
                            if self.ribbon_button(ui, "insert_text", "Text Box", true) {
                                self.insert_shape_mode=None;
                                self.insert_line_mode=false;
                                self.insert_text_box_mode = true;
                                self.finish_edit(true);
                                self.finish_text_box_edit(true);
                            }
                            if self.ribbon_button(ui, "new_map", "Add Map", true) { self.add_map(); }
                        });
                        ui.separator();
                        Self::ribbon_group(ui, group_width(1.0), self.language.ribbon_text("Link"), |ui| {
                            if self.ribbon_button(ui, "link", "Link", has_selection) {
                                if self.selection.len() == 2 { self.link_two_selected(); }
                                else { self.begin_link(self.selected); }
                            }
                        });
                        ui.separator();
                        if !plan.compact {
                            Self::ribbon_group(ui,group_width(5.0),self.language.ribbon_text("Shapes"),|ui|{
                                for (icon,label,kind) in [
                                    ("rect","Rectangle",ShapeKind::Rectangle),
                                    ("rounded","Rounded",ShapeKind::RoundedRectangle),
                                    ("ellipse","Ellipse",ShapeKind::Ellipse),
                                    ("diamond","Decision",ShapeKind::Diamond)] {
                                    if self.ribbon_button(ui,icon,label,true) {
                                        self.insert_shape_mode=Some(kind);
                                        self.insert_text_box_mode=false;self.insert_line_mode=false;
                                        self.insert_connector_mode=false;
                                    }
                                }
                                 if self.ribbon_button(ui,"line","Line",true){
                                     self.insert_shape_mode=None;self.insert_text_box_mode=false;
                                     self.insert_line_mode=true;self.insert_connector_mode=false;
                                 }
                             });
                            ui.menu_button(self.language.ribbon_text("More Shapes ▾"),|ui|{
                                for (label,kind) in [
                                    ("Parallelogram (Input/Output)",ShapeKind::Parallelogram),
                                    ("Cylinder (Database)",ShapeKind::Cylinder),
                                    ("Hexagon (Preparation)",ShapeKind::Hexagon),
                                    ("Document",ShapeKind::Document)] {
                                    if self.shape_menu_item(ui, label, kind) {
                                        self.insert_shape_mode=Some(kind);
                                        self.insert_text_box_mode=false;self.insert_line_mode=false;
                                        self.insert_connector_mode=false;ui.close();
                                    }
                                }
                            });
                        } else {
                            ui.menu_button(self.language.ribbon_text("Shapes ▾"),|ui|{
                                for (label,kind) in [
                                    ("Rectangle",ShapeKind::Rectangle),("RoundedRectangle",ShapeKind::RoundedRectangle),
                                    ("Ellipse",ShapeKind::Ellipse),("Diamond (Decision)",ShapeKind::Diamond),
                                    ("Parallelogram (Input/Output)",ShapeKind::Parallelogram),
                                    ("Cylinder (Database)",ShapeKind::Cylinder),
                                    ("Hexagon (Preparation)",ShapeKind::Hexagon),
                                    ("Document",ShapeKind::Document)] {
                                    if self.shape_menu_item(ui, label, kind) {
                                        self.insert_shape_mode=Some(kind);self.insert_line_mode=false;
                                        self.insert_text_box_mode=false;self.insert_connector_mode=false;ui.close();
                                    }
                                }
                                 if ui.button(self.language.ribbon_text("Line (Freeform)")).clicked(){
                                     self.insert_shape_mode=None;self.insert_line_mode=true;
                                     self.insert_connector_mode=false;ui.close();
                                 }
                            });
                        }
                        ui.separator();
                        if !plan.compact {
                            Self::ribbon_group(ui,group_width(2.0),self.language.ribbon_text("Connector"),|ui|{
                                if self.ribbon_button(ui,"connect","Connector",true){
                                    self.insert_shape_mode=None;self.insert_line_mode=false;
                                    self.insert_connector_route=ConnectorRoute::Straight;self.insert_connector_arrow=false;
                                    self.insert_connector_mode=true;
                                }
                                if self.ribbon_button(ui,"orthogonal","Connector (Arrow)",true){
                                    self.insert_shape_mode=None;self.insert_line_mode=false;
                                    self.insert_connector_route=ConnectorRoute::Orthogonal;self.insert_connector_arrow=true;
                                    self.insert_connector_mode=true;
                                }
                            });
                        } else {
                            ui.menu_button(self.language.ribbon_text("Connector ▾"),|ui|{
                                for (label,route,arrow) in [("Connector",ConnectorRoute::Straight,false),
                                                      ("Connector (Arrow)",ConnectorRoute::Orthogonal,true)] {
                                    if ui.button(self.language.ribbon_text(label)).clicked(){
                                        self.insert_shape_mode=None;self.insert_line_mode=false;
                                        self.insert_connector_route=route;self.insert_connector_arrow=arrow;
                                        self.insert_connector_mode=true;
                                        ui.close();
                                    }
                                }
                            });
                        }
                        ui.separator();
                        // Tool cancellation is discoverable from the cursor and help, not a ribbon label.
                        // Group/Ungroup remain in the context menu/shortcuts.
                    }
                });
            });
        // Wheel anywhere over the ribbon switches Home/Insert; no wheel action
        // is dispatched to the canvas when the pointer is above the ribbon.
        let ribbon_rect = Rect::from_min_max(Pos2::new(clip.left(), ribbon_top),
            Pos2::new(clip.right(), ui.min_rect().bottom()));
        let over_ribbon = ui.ctx().input(|i| i.pointer.hover_pos().is_some_and(|p| ribbon_rect.contains(p)));
        if over_ribbon {
            let delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if delta.abs() >= 8.0 {
                self.ribbon_tab = if delta < 0.0 { (self.ribbon_tab + 1).min(tabs.len() - 1) }
                    else { self.ribbon_tab.saturating_sub(1) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_windows_show_direct_palette_and_full_ribbon() {
        let layout = RibbonPlan::for_width(1510.0);
        assert!(!layout.compact);
        assert!(layout.inline_palette);
        assert!(layout.secondary);
        assert!(!layout.narrow);
    }

    #[test]
    fn reduced_width_degrades_to_color_dropdown_without_horizontal_scrolling() {
        let layout = RibbonPlan::for_width(760.0);
        assert!(layout.compact);
        assert!(!layout.inline_palette);
        assert!(!layout.secondary);
        assert!(layout.narrow);
    }

    #[test]
    fn middle_width_keeps_direct_palette_but_collapses_extra_actions() {
        let layout = RibbonPlan::for_width(1000.0);
        assert!(layout.inline_palette);
        assert!(!layout.secondary);
        assert!(!layout.narrow);
    }
}
