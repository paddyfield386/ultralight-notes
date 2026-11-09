//! Viewport geometry, node hit tests, drop targets and dotted background.
use super::*;

/// Extends a world-space bounding box with a rectangular area.
fn extend_bounds(min: &mut Pos2, max: &mut Pos2, left: f32, top: f32, right: f32, bottom: f32) {
    min.x = min.x.min(left);
    min.y = min.y.min(top);
    max.x = max.x.max(right);
    max.y = max.y.max(bottom);
}

impl MindmapApp {
    pub(super) fn fit_to_board(&mut self, bounds: Rect, positions: &HashMap<NodeId, Position>) {
        if positions.is_empty() { return; }
        let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (&id, &position) in positions {
            let Some(node) = self.doc.node(id) else { continue; };
            let width = self.card_width(node, self.doc.depth(id));
            let height = card_height(node, self.doc.depth(id));
            extend_bounds(&mut min, &mut max,
                position.x - width * 0.5, position.y - height * 0.5,
                position.x + width * 0.5, position.y + height * 0.5);
        }
        for box_ in &self.doc.text_boxes {
            extend_bounds(&mut min, &mut max,
                box_.position.x, box_.position.y,
                box_.position.x + box_.width, box_.position.y + box_.height);
        }
        for shape in &self.doc.shapes {
            if !shape.visible {continue;}
            extend_bounds(&mut min, &mut max,
                shape.position.x, shape.position.y,
                shape.position.x + shape.width, shape.position.y + shape.height);
        }
        for line in &self.doc.lines {
            if !line.visible {continue;}
            extend_bounds(&mut min, &mut max,
                line.start.x.min(line.end.x), line.start.y.min(line.end.y),
                line.start.x.max(line.end.x), line.start.y.max(line.end.y));
        }
        if !min.x.is_finite() { return; }
        let width = (max.x - min.x).max(1.0);
        let height = (max.y - min.y).max(1.0);
        self.zoom = ((bounds.width() - 90.0).max(1.0) / width)
            .min((bounds.height() - 90.0).max(1.0) / height).clamp(0.25, 2.5);
        let world_mid = egui::vec2((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
        let base = Pos2::new(bounds.left() + 152.0, bounds.center().y);
        self.pan = bounds.center() - base - world_mid * self.zoom;
    }
    pub(super) fn card_width(&self, node: &Node, depth: usize) -> f32 {
        // Keep the size and tree layout immutable throughout IME preedit and
        // typing. The floating editor scrolls text until Enter commits.
        node_width_text(&node.title, depth)
    }
    pub(super) fn positions(&self) -> HashMap<NodeId, Position> {
        layout_with_widths(&self.doc, |id, depth| {
            self.doc.node(id).map(|node| self.card_width(node, depth)).unwrap_or(160.0)
        })
    }
    pub(super) fn draw_dotted_grid(&self, painter: &egui::Painter, bounds: Rect, origin: Pos2) {
        // Dots remain fixed in map/world coordinates when the user pans or zooms.
        // Keep at least ~18 screen pixels between dots when zoomed far out.
        let mut world_step = self.grid.spacing;
        while world_step * self.zoom < 18.0 { world_step *= 2.0; }
        let screen_step = world_step * self.zoom;
        let t = f32::from(self.grid.contrast) / 100.0;
        let lerp = |background: u8, foreground: u8, factor: f32| -> u8 {
            (f32::from(background) * (1.0 - factor) + f32::from(foreground) * factor).round() as u8
        };
        let (background, foreground) = if self.dark {
            ([33, 39, 54], [120, 143, 181])
        } else {
            ([248, 250, 253], [130, 151, 186])
        };
        let rgb = |factor: f32| {
            Color32::from_rgb(
                lerp(background[0], foreground[0], factor),
                lerp(background[1], foreground[1], factor),
                lerp(background[2], foreground[2], factor),
            )
        };
        let minor = rgb(t);
        let major = rgb((t + 0.22).min(1.0));
        let x0 = ((bounds.left() - origin.x) / screen_step).floor() as i32;
        let x1 = ((bounds.right() - origin.x) / screen_step).ceil() as i32;
        let y0 = ((bounds.top() - origin.y) / screen_step).floor() as i32;
        let y1 = ((bounds.bottom() - origin.y) / screen_step).ceil() as i32;
        for x in x0..=x1 {
            for y in y0..=y1 {
                let is_major = self.grid.major_dots &&
                    x.rem_euclid(5) == 0 && y.rem_euclid(5) == 0;
                let center = origin + egui::vec2(x as f32 * screen_step, y as f32 * screen_step);
                if bounds.contains(center) {
                    painter.circle_filled(
                        center,
                        if is_major { self.grid.dot_radius * 1.35 } else { self.grid.dot_radius },
                        if is_major { major } else { minor },
                    );
                }
            }
        }
    }
    pub(super) fn drop_at(&self, pointer: Pos2, moved: NodeId, positions: &std::collections::HashMap<NodeId, Position>, origin: Pos2) -> Option<DropIntent> {
        // Center hit: nesting. Under/over a node: sibling insertion and auto reflow.
        if let Some(target) = self.hit_test(pointer, positions, origin, Some(moved)) {
            if self.doc.parent(moved) != Some(target) {
                return Some(DropIntent::Child(target));
            }
            return None;
        }
        let mut best: Option<(f32, DropIntent)> = None;
        for node in &self.doc.nodes {
            if node.id == ROOT || node.id == moved || self.doc.contains_descendant(moved, node.id) { continue; }
            let Some(&pos) = positions.get(&node.id) else { continue; };
            let rect = self.screen_rect(node, pos, origin);
            let margin = (30.0 * self.zoom).clamp(12.0, 40.0);
            if pointer.x < rect.left() - 8.0 || pointer.x > rect.right() + 8.0 { continue; }
            let candidate = if pointer.y > rect.bottom() && pointer.y <= rect.bottom() + margin {
                Some((pointer.y - rect.bottom(), DropIntent::After(node.id)))
            } else if pointer.y < rect.top() && pointer.y >= rect.top() - margin {
                Some((rect.top() - pointer.y, DropIntent::Before(node.id)))
            } else { None };
            if let Some((distance, intent)) = candidate {
                if best.is_none_or(|(old, _)| distance < old) { best = Some((distance, intent)); }
            }
        }
        best.map(|(_, intent)| intent)
    }
    pub(super) fn screen_rect(&self, node: &Node, pos: Position, origin: Pos2) -> Rect {
        let depth = self.doc.depth(node.id);
        let size = egui::vec2(self.card_width(node, depth), card_height(node, depth)) * self.zoom;
        Rect::from_center_size(origin + egui::vec2(pos.x, pos.y) * self.zoom, size)
    }
    pub(super) fn hit_test(&self, pos: Pos2, positions: &std::collections::HashMap<NodeId, Position>, origin: Pos2,
                exclude_branch: Option<NodeId>) -> Option<NodeId> {
        self.doc.nodes.iter().rev().find(|n| {
            if let Some(excluded) = exclude_branch {
                if n.id == excluded || self.doc.contains_descendant(excluded, n.id) { return false; }
            }
            positions.get(&n.id).is_some_and(|p| self.screen_rect(n, *p, origin).contains(pos))
        }).map(|n| n.id)
    }
    pub(super) fn text_box_screen_rect(&self, id: u64, origin: Pos2) -> Option<Rect> {
        let box_ = self.doc.text_box(id)?;
        Some(Rect::from_min_size(
            origin + egui::vec2(box_.position.x, box_.position.y) * self.zoom,
            egui::vec2(box_.width, box_.height) * self.zoom,
        ))
    }
    pub(super) fn hit_test_text_box(&self, point: Pos2, origin: Pos2) -> Option<u64> {
        self.doc.text_boxes.iter().rev().find(|b| self.text_box_screen_rect(b.id, origin)
            .is_some_and(|rect| rect.contains(point))).map(|b| b.id)
    }
    pub(super) fn text_box_resize_handle(&self, id: u64, origin: Pos2) -> Option<Rect> {
        let r = self.text_box_screen_rect(id, origin)?;
        Some(Rect::from_center_size(r.right_bottom() - egui::vec2(3.0, 3.0), egui::vec2(16.0, 16.0)))
    }
}
