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
            "table" => self.insert_shape_mode == Some(ShapeKind::Table),
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
        ui.label(self.language.text("Horizontal", "水平方向"));
        ui.horizontal(|ui| {
            for (hint, value) in [("Left", TextHorizontal::Left), ("Center", TextHorizontal::Center), ("Right", TextHorizontal::Right)] {
                if self.text_alignment_icon_button(ui, Some(value), None, self.language.ribbon_text(hint)) {
                    self.set_selected_text_alignment(Some(value), None); ui.close();
                }
            }
        });
        ui.label(self.language.text("Vertical", "垂直方向"));
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
            "calculator" => include_bytes!("../../assets/tabler/calculator.rgba"),
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
            "table" => include_bytes!("../../assets/tabler/new_map.rgba"),
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
    fn table_insert_picker(&mut self, ui: &mut egui::Ui) {
        let width = if self.ribbon_compact { 38.0 } else { 62.0 };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 65.0), Sense::click());
        let color = if self.dark { Color32::from_rgb(236, 241, 255) } else { Color32::from_rgb(79, 90, 134) };
        if response.hovered() {
            ui.painter().rect_filled(rect.shrink(2.0), 6.0, if self.dark { Color32::from_rgb(65, 71, 91) } else { Color32::from_rgb(232, 235, 254) });
        }
        let center = Pos2::new(rect.center().x, if self.ribbon_compact { rect.center().y } else { rect.top() + 22.0 });
        let icon = Rect::from_center_size(center, egui::vec2(22.0, 22.0));
        ui.painter().rect_stroke(icon, 2.0, Stroke::new(1.7, color), StrokeKind::Inside);
        for i in 1..3 {
            let x = icon.left() + icon.width() * i as f32 / 3.0;
            let y = icon.top() + icon.height() * i as f32 / 3.0;
            ui.painter().line_segment([Pos2::new(x, icon.top()), Pos2::new(x, icon.bottom())], Stroke::new(1.25, color));
            ui.painter().line_segment([Pos2::new(icon.left(), y), Pos2::new(icon.right(), y)], Stroke::new(1.25, color));
        }
        if !self.ribbon_compact {
            let label = self.language.text("Table", "\u{8868}");
            let label_color = if self.dark { Color32::WHITE } else { INK };
            let galley = ui.painter().layout(label.to_owned(), FontId::proportional(9.5), label_color, width - 6.0);
            ui.painter().with_clip_rect(rect.shrink(2.0)).galley(Pos2::new(rect.center().x - galley.size().x * 0.5, rect.bottom() - galley.size().y - 4.0), galley, label_color);
            ui.painter().text(Pos2::new(rect.right() - 8.0, rect.bottom() - 8.0), Align2::CENTER_CENTER, "v", FontId::proportional(8.0), color);
        }
        let response = response.on_hover_text(self.language.text("Insert Table", "\u{8868}\u{3092}\u{633f}\u{5165}"));
        egui::Popup::menu(&response).width(245.0).show(|ui| {
            ui.set_min_width(245.0);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(self.language.text("Insert Table", "\u{8868}\u{3092}\u{633f}\u{5165}")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("表計算").small().color(if self.dark { Color32::from_gray(165) } else { Color32::from_gray(120) }));
                });
            });
            ui.add_space(3.0);
            ui.label(egui::RichText::new(self.language.text("Select table size", "\u{8868}\u{306E}\u{5927}\u{304D}\u{3055}\u{3092}\u{9078}\u{629E}")).small().color(if self.dark { Color32::from_gray(175) } else { Color32::from_gray(105) }));
            ui.add_space(8.0);
            let mut hover_size = None;
            let mut chosen_size = None;
            let mut cells = Vec::with_capacity(80);
            egui::Grid::new("table-size-picker").min_col_width(20.0).max_col_width(20.0)
                .spacing(egui::vec2(3.0, 3.0)).show(ui, |ui| {
                for row in 1..=8 {
                    for col in 1..=10 {
                        let (cell, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::click());
                        if response.hovered() { hover_size = Some((row, col)); }
                        if response.clicked() { chosen_size = Some((row, col)); }
                        cells.push((row, col, cell, response.hovered()));
                    }
                    ui.end_row();
                }
            });
            let (preview_rows, preview_cols) = hover_size.unwrap_or((1, 1));
            for (row, col, cell, cell_hovered) in cells {
                let active = row <= preview_rows && col <= preview_cols;
                let fill = if active { if self.dark { Color32::from_rgb(77, 91, 145) } else { Color32::from_rgb(210, 220, 255) } }
                    else if cell_hovered { if self.dark { Color32::from_rgb(58, 65, 84) } else { Color32::from_rgb(240, 243, 251) } }
                    else if self.dark { Color32::from_rgb(39, 45, 59) } else { Color32::WHITE };
                let border = if active { ACCENT } else if self.dark { Color32::from_gray(100) } else { Color32::from_rgb(190, 198, 214) };
                ui.painter().rect_filled(cell, 2.0, fill);
                ui.painter().rect_stroke(cell, 2.0, Stroke::new(if active { 1.4 } else { 0.8 }, border), StrokeKind::Inside);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (rows, cols) = hover_size.unwrap_or((1, 1));
                ui.label(egui::RichText::new(format!("{rows} x {cols}")).strong().color(ACCENT));
                ui.label(egui::RichText::new(self.language.text("cells", "\u{30BB}\u{30EB}")).small());
            });
            if let Some((rows, cols)) = chosen_size {
                self.pending_table_create = Some((rows, cols));
                ui.close();
            }
        });
    }
    fn table_ribbon_button(&self,ui:&mut egui::Ui,glyph:&str,label:&str,enabled:bool)->bool {
        self.table_ribbon_button_state(ui,glyph,label,enabled,false)
    }
    fn table_ribbon_button_state(&self,ui:&mut egui::Ui,glyph:&str,label:&str,enabled:bool,selected:bool)->bool {
        let w=if self.ribbon_compact{38.0}else{62.0};
        let (rect,response)=ui.allocate_exact_size(egui::vec2(w,65.0),Sense::click());
        if (response.hovered()||selected)&&enabled {ui.painter().rect_filled(rect.shrink(2.0),6.0,if selected{if self.dark{Color32::from_rgb(82,74,135)}else{Color32::from_rgb(213,207,255)}}else if self.dark{Color32::from_rgb(65,71,91)}else{Color32::from_rgb(232,235,254)});}
        let icon_color=if enabled {if self.dark{Color32::from_rgb(236,241,255)}else{Color32::from_rgb(79,90,134)}}else{Color32::from_gray(130)};
        let icon_pos=Pos2::new(rect.center().x,if self.ribbon_compact{rect.center().y}else{rect.top()+23.0});
        if glyph.starts_with("align-") {
            for (i,len) in [14.0,9.0,12.0].into_iter().enumerate() {
                let x=match glyph {"align-left"=>icon_pos.x-10.0,"align-right"=>icon_pos.x+10.0-len,_=>icon_pos.x-len/2.0};
                let y=icon_pos.y-6.0+i as f32*6.0;
                ui.painter().line_segment([Pos2::new(x,y),Pos2::new(x+len,y)],Stroke::new(2.0,icon_color));
            }
        } else {ui.painter().text(icon_pos,Align2::CENTER_CENTER,glyph,FontId::proportional(24.0),icon_color);}
        if !self.ribbon_compact||self.ribbon_tab==2 {let font_size=if self.ribbon_compact{6.5}else{9.0};let label_color=if enabled{if self.dark{Color32::WHITE}else{INK}}else{Color32::GRAY};let galley=ui.painter().layout(label.to_owned(),FontId::proportional(font_size),label_color,w-2.0);ui.painter().with_clip_rect(rect.shrink(1.0)).galley(Pos2::new(rect.center().x-galley.size().x/2.0,rect.bottom()-galley.size().y-2.0),galley,label_color);}
        response.on_hover_text(label).clicked()&&enabled
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
        if ui.button(self.language.ribbon_text("Grid Settings\u{2026}")).clicked() { self.grid_settings_open = true; ui.close(); }
        ui.separator();
        if ui.checkbox(&mut self.reflow_on_add, self.language.ribbon_text("Automatically rearrange when adding")).changed() { self.save_preferences(); }
        ui.label(egui::RichText::new(self.language.text("Off by default. Existing node positions are preserved.", "\u{9ED9}\u{8A8D}\u{3067}\u{306F}\u{7121}\u{52B9}\u{3067}\u{3059}\u{3002}\u{65E2}\u{5B58}\u{306E}\u{914D}\u{7F6E}\u{306F}\u{4FDD}\u{6301}\u{3055}\u{308C}\u{307E}\u{3059}\u{3002}")).small().weak());
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
    fn show_table_filter_popup(&mut self,ctx:&egui::Context) {
        let Some((table_id,column,position,mut included,mut condition))=self.table_filter_popup.take() else{return};
        let Some(table)=self.doc.shape(table_id).and_then(|shape|shape.table.as_ref()) else{return};
        let mut values:Vec<String>=table.cells.iter().map(|row|row.get(column).cloned().unwrap_or_default()).collect::<HashSet<_>>().into_iter().collect();
        values.sort_by_key(|value|value.to_lowercase());
        let mut apply=false;
        let mut cancel=false;
        egui::Area::new(egui::Id::unique(("table-filter-values",table_id,column)))
            .order(egui::Order::Foreground).fixed_pos(position).show(ctx,|ui|egui::Frame::popup(ui.style()).show(ui,|ui|{
                ui.set_min_width(220.0);ui.set_max_width(300.0);
                ui.strong(format!("列 {} の絞り込み",column+1));
                ui.horizontal(|ui|{
                    if ui.selectable_label(condition.is_none(),"値リスト").clicked(){condition=None;}
                    if ui.selectable_label(condition.is_some(),"条件").clicked(){condition.get_or_insert((TableFilterOperator::Equal,String::new()));}
                });
                if let Some((operator,value))=condition.as_mut(){
                    egui::ComboBox::from_id_salt(("table-filter-operator",table_id,column)).selected_text(match operator{
                        TableFilterOperator::Equal=>"等しい",TableFilterOperator::NotEqual=>"等しくない",
                        TableFilterOperator::GreaterThan=>"より大きい",TableFilterOperator::GreaterThanOrEqual=>"以上",
                        TableFilterOperator::LessThan=>"より小さい",TableFilterOperator::LessThanOrEqual=>"以下",
                        TableFilterOperator::Contains=>"含む",TableFilterOperator::StartsWith=>"先頭が一致",TableFilterOperator::EndsWith=>"末尾が一致",
                    }).show_ui(ui,|ui|{
                        for (choice,label) in [(TableFilterOperator::Equal,"等しい"),(TableFilterOperator::NotEqual,"等しくない"),
                            (TableFilterOperator::GreaterThan,"より大きい"),(TableFilterOperator::GreaterThanOrEqual,"以上"),
                            (TableFilterOperator::LessThan,"より小さい"),(TableFilterOperator::LessThanOrEqual,"以下"),
                            (TableFilterOperator::Contains,"含む"),(TableFilterOperator::StartsWith,"先頭が一致"),(TableFilterOperator::EndsWith,"末尾が一致")] {
                            ui.selectable_value(operator,choice,label);
                        }
                    });
                    ui.text_edit_singleline(value);
                }else{
                    ui.horizontal(|ui|{
                        if ui.small_button("すべて選択").clicked(){included=values.iter().cloned().collect();}
                        if ui.small_button("すべて解除").clicked(){included.clear();}
                    });
                    egui::ScrollArea::vertical().max_height(220.0).show(ui,|ui|{
                        for value in &values {
                            let label=if value.is_empty(){"(空白)".to_owned()}else{value.clone()};
                            let mut selected=included.contains(value);
                            if ui.checkbox(&mut selected,label).changed(){
                                if selected{included.insert(value.clone());}else{included.remove(value);}
                            }
                        }
                    });
                }
                ui.horizontal(|ui|{
                    if ui.button("適用").clicked(){apply=true;}
                    if ui.button("キャンセル").clicked(){cancel=true;}
                });
            }));
        if apply {
            if let Some((operator,value))=condition{self.apply_table_filter_condition(table_id,column,operator,value);}
            else{self.apply_table_filter_values(table_id,column,included);}
        } else if !cancel {self.table_filter_popup=Some((table_id,column,position,included,condition));}
    }
    /// A responsive two-tab ribbon whose content never requests a scroll area.
    /// All operations fit at the current width by reducing label density and
    /// moving less frequent actions into the More menu when necessary.
    pub(super) fn ribbon(&mut self, ui: &mut egui::Ui) {
        let has_selected_table=self.selected_shape.is_some_and(|id|self.doc.shape(id).is_some_and(|s|s.kind==ShapeKind::Table));
        let mut tabs = vec![
            self.language.text("Home", "\u{30DB}\u{30FC}\u{30E0}"),
            self.language.text("Insert", "\u{633F}\u{5165}"),
        ];
        if has_selected_table {tabs.push(self.language.text("Table", "\u{8868}"));}
        if self.ribbon_tab>=tabs.len(){self.ribbon_tab=0;}
        let full_width = ui.available_width();
        let plan = RibbonPlan::for_width(full_width);
        self.ribbon_compact = if has_selected_table&&self.ribbon_tab==2 {full_width<1100.0}else{plan.compact};
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
                // Keep the content panel's vertical footprint identical on
                // Home, Insert, and Table, regardless of its controls.
                ui.set_min_height(82.0);
                ui.set_max_height(82.0);
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
                            ui.menu_button("\u{25BE}", |ui| {
                                if ui.button("Save As\u{2026}").clicked() {
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
                        Self::ribbon_group(ui, group_width(1.0), "ツール", |ui| {
                            if self.ribbon_button(ui, "calculator", "電卓", true) { self.calculator.open = !self.calculator.open; self.calculator.keyboard_focus=self.calculator.open; }
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
                        Self::ribbon_group(ui, 82.0, self.language.text("Text alignment", "\u{6587}\u{5B57}\u{63C3}\u{3048}"), |ui| self.ribbon_text_alignment(ui));
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
                                ui.menu_button(self.language.ribbon_text("Color \u{25BE}"), |ui| self.ribbon_color_menu(ui));
                            });
                        }
                        ui.separator();
                        Self::ribbon_group(ui, 77.0, self.language.ribbon_text("View"), |ui| {
                            ui.menu_button(self.language.text("More", "その他"), |ui| {
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
                                if ui.button(self.language.ribbon_text("Find Node\u{2026}  Ctrl+F")).clicked() {
                                    self.search_open = true;
                                    self.search_focus_pending = true;
                                    ui.close();
                                }
                                ui.menu_button(self.language.ribbon_text("Export Image"), |ui| {
                                    if ui.button(self.language.ribbon_text("Save as PNG\u{2026}")).clicked() { self.export_file(true); ui.close(); }
                                    if ui.button(self.language.ribbon_text("Save as SVG\u{2026}")).clicked() { self.export_file(false); ui.close(); }
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
                                    ui.menu_button(self.language.text("Text alignment", "\u{6587}\u{5B57}\u{63C3}\u{3048}"), |ui| self.text_alignment_menu(ui));
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
                                    for (label,d) in [("最前面へ",2),("前面へ",1),("背面へ",-1),("最背面へ",-2)] {
                                        if ui.button(label).clicked() {self.change_selected_shape_z(d);ui.close();}
                                    }
                                });
                            });
                        });
                    } else if self.ribbon_tab==1 {
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
                        Self::ribbon_group(ui, group_width(3.0), self.language.ribbon_text("Canvas"), |ui| {
                            if self.ribbon_button(ui, "insert_text", "Text Box", true) {
                                self.insert_shape_mode=None;
                                self.insert_line_mode=false;
                                self.insert_text_box_mode = true;
                                self.finish_edit(true);
                                self.finish_text_box_edit(true);
                            }
                            if self.ribbon_button(ui, "new_map", "Add Map", true) { self.add_map(); }
                            self.table_insert_picker(ui);
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
                            ui.menu_button(self.language.ribbon_text("More Shapes \u{25BE}"),|ui|{
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
                            ui.menu_button(self.language.ribbon_text("Shapes \u{25BE}"),|ui|{
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
                            ui.menu_button(self.language.ribbon_text("Connector \u{25BE}"),|ui|{
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
                    } else {
                        if let Some(id)=self.selected_shape.filter(|id|self.doc.shape(*id).is_some_and(|s|s.kind==ShapeKind::Table)) {
                            let (row,col)=self.selected_table_cell.filter(|(table,_,_)|*table==id)
                                .map(|(_,row,col)|(row,col)).unwrap_or((0,0));
                            let (rows,cols)=self.doc.shape(id).and_then(|s|s.table.as_ref())
                                .map(|t|(t.cells.len(),t.cells.first().map_or(0,Vec::len))).unwrap_or((1,1));
                            let header=self.doc.shape(id).and_then(|s|s.table.as_ref()).is_some_and(|t|t.header_row);
                            let banded=self.doc.shape(id).and_then(|s|s.table.as_ref()).is_some_and(|t|t.banded_rows);
                            let selected_range=self.selected_table_range.filter(|(table,_,_,_,_)|*table==id);
                            let can_merge=selected_range.is_some_and(|(_,r0,c0,r1,c1)|r0!=r1||c0!=c1);
                            let can_unmerge=self.doc.shape(id).and_then(|s|s.table.as_ref()).and_then(|t|t.merge_at(row,col)).is_some();
                            Self::ribbon_group(ui,group_width(4.1),self.language.text("Rows & Columns","\u{884c}\u{3068}\u{5217}"),|ui|{
                                if self.table_ribbon_button(ui,"\u{FF0B}","\u{884C}\u{8FFD}\u{52A0}",rows<50){self.table_insert_row(id,row+1);}
                                if self.table_ribbon_button(ui,"\u{FF0B}","\u{5217}\u{8FFD}\u{52A0}",cols<50){self.table_insert_column(id,col+1);}
                                if self.table_ribbon_button(ui,"\u{2212}","\u{884C}\u{524A}\u{9664}",rows>1){self.table_delete_row(id,row);}
                                if self.table_ribbon_button(ui,"\u{2212}","\u{5217}\u{524A}\u{9664}",cols>1){self.table_delete_column(id,col);}
                            });
                            ui.separator();
                            let has_filters=self.doc.shape(id).and_then(|s|s.table.as_ref()).is_some_and(|t|!t.filters.is_empty());
                            let row_hidden=self.doc.shape(id).and_then(|s|s.table.as_ref()).is_some_and(|t|t.manually_hidden_rows.contains(&row));
                            Self::ribbon_group(ui,group_width(3.4),"データ",|ui|{
                                if self.table_ribbon_button(ui,"▾","値で絞り込み",row<rows&&col<cols){self.open_table_filter_popup(ui.ctx(),id,col);}
                                if self.table_ribbon_button(ui,"×","フィルター解除",has_filters){self.clear_table_filters(id);}
                                if self.table_ribbon_button(ui,"↕",if row_hidden{"行を再表示"}else{"行を非表示"},row<rows){self.toggle_table_row_hidden(id,row);}
                            });
                            ui.separator();
                            Self::ribbon_group(ui,group_width(2.1),self.language.text("Style","\u{30B9}\u{30BF}\u{30A4}\u{30EB}"),|ui|{
                                if self.table_ribbon_button(ui,if header{"\u{25A4}"}else{"\u{25A5}"},"\u{898B}\u{51FA}\u{3057}",true){self.toggle_table_header();}
                                if self.table_ribbon_button(ui,if banded{"\u{25A6}"}else{"\u{25A5}"},"\u{7E1E}\u{6A21}\u{69D8}",true){self.toggle_table_banded_rows();}
                            });
                            ui.separator();
                            Self::ribbon_group(ui,group_width(5.1),self.language.text("Cell","\u{30BB}\u{30EB}"),|ui|{
                                for (choice,glyph,label) in [(TextHorizontal::Left,"align-left","\u{5DE6}\u{63C3}\u{3048}"),(TextHorizontal::Center,"align-center","\u{4E2D}\u{592E}\u{63C3}\u{3048}"),(TextHorizontal::Right,"align-right","\u{53F3}\u{63C3}\u{3048}")] {
                                    if self.table_ribbon_button(ui,glyph,label,true){self.set_selected_table_alignment(choice);}
                                }
                                if self.table_ribbon_button(ui,"\u{21D4}","\u{7D50}\u{5408}",can_merge){self.merge_selected_table_cells();}
                                if self.table_ribbon_button(ui,"\u{2194}","\u{89E3}\u{9664}",can_unmerge){self.unmerge_selected_table_cells();}
                            });
                            ui.separator();
                            Self::ribbon_group(ui,120.0,self.language.text("Fill Color","\u{586B}\u{308A}\u{3064}\u{3076}\u{3057}"),|ui|{
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 2.0);
                                    for row in 0..2 {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 3.0;
                                            for index in row*4..row*4+4 {
                                                let color=TINTS[index].fill;
                                                if Self::color_tile(ui,color,TINTS[index].border,COLOR_LABELS[index]) {
                                                    self.table_fill_color=rgb_of(color);
                                                    self.set_selected_table_fill(Some(self.table_fill_color));
                                                }
                                            }
                                        });
                                    }
                                    if ui.small_button(self.language.text("Default","\u{521D}\u{671F}\u{5024}")).clicked() {
                                        self.set_selected_table_fill(None);
                                    }
                                });
                            });
                            ui.separator();
                            Self::ribbon_group(ui,120.0,self.language.text("Font Color","\u{6587}\u{5B57}\u{8272}"),|ui|{
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 2.0);
                                    for row in 0..2 {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 3.0;
                                            for index in row*4..row*4+4 {
                                                let color=TINTS[index].line;
                                                if Self::color_tile(ui,color,TINTS[index].border,COLOR_LABELS[index]) {
                                                    self.table_text_color=rgb_of(color);
                                                    self.set_selected_table_text_color(Some(self.table_text_color));
                                                }
                                            }
                                        });
                                    }
                                    if ui.small_button(self.language.text("Default","\u{521D}\u{671F}\u{5024}")).clicked() {
                                        self.set_selected_table_text_color(None);
                                    }
                                });
                            });
                        }
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
        self.show_table_filter_popup(ui.ctx());
        self.calculator_window(ui.ctx());
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
