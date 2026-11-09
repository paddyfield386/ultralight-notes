//! Cross-links (associations) between nodes; they never affect tree layout.
use super::*;

impl MindmapApp {
    pub(super) fn begin_link(&mut self, from: NodeId) {
        if self.doc.node(from).is_none() { return; }
        self.finish_edit(true);
        self.link_origin = Some(from);
        self.status = "Click the node to link to (Esc to cancel)".into();
    }
    pub(super) fn create_link(&mut self, from: NodeId, to: NodeId) {
        if from == to { self.status = "A node cannot be linked to itself".into(); return; }
        if self.doc.links.iter().any(|l| l.normalized() == (NodeLink { from, to }).normalized()) {
            self.status = "This link already exists".into(); return;
        }
        if self.doc.node(from).is_none() || self.doc.node(to).is_none() { return; }
        self.checkpoint();
        self.doc.add_link(from, to);
        self.selected_link = Some(NodeLink { from, to });
        self.status.clear();
    }
    pub(super) fn link_two_selected(&mut self) {
        if self.selection.len() == 2 {
            let mut ids: Vec<_> = self.selection.iter().copied().collect();
            ids.sort_unstable();
            self.create_link(ids[0], ids[1]);
        } else if self.doc.node(self.selected).is_some() {
            self.begin_link(self.selected);
        }
    }
    pub(super) fn delete_selected_link(&mut self) {
        if let Some(link) = self.selected_link.take() {
            if self.doc.links.contains(&link) || self.doc.links.iter().any(|l| l.normalized() == link.normalized()) {
                self.checkpoint();
                self.doc.remove_link(link.from, link.to);
                self.status.clear();
            }
        }
    }
    fn association_curve(&self, link: NodeLink, positions: &HashMap<NodeId, Position>, origin: Pos2)
        -> Option<[Pos2; 4]> {
        let a = self.doc.node(link.from)?;
        let b = self.doc.node(link.to)?;
        let a_pos = *positions.get(&a.id)?;
        let b_pos = *positions.get(&b.id)?;
        let a_rect = self.screen_rect(a, a_pos, origin);
        let b_rect = self.screen_rect(b, b_pos, origin);
        let (start, end) = if a_rect.center().x < b_rect.center().x {
            (a_rect.right_center(), b_rect.left_center())
        } else {
            (a_rect.left_center(), b_rect.right_center())
        };
        let direction = if end.x >= start.x { 1.0 } else { -1.0 };
        let bend = ((end.x - start.x).abs() * 0.42).max(24.0 * self.zoom);
        Some([start, start + Vec2::new(direction * bend, 0.0),
              end - Vec2::new(direction * bend, 0.0), end])
    }
    fn curve_at(points: [Pos2; 4], t: f32) -> Pos2 {
        let u = 1.0 - t;
        let w0 = u*u*u;
        let w1 = 3.0*u*u*t;
        let w2 = 3.0*u*t*t;
        let w3 = t*t*t;
        Pos2::new(points[0].x*w0 + points[1].x*w1 + points[2].x*w2 + points[3].x*w3,
                  points[0].y*w0 + points[1].y*w1 + points[2].y*w2 + points[3].y*w3)
    }
    pub(super) fn hit_test_link(&self, pointer: Pos2, positions: &HashMap<NodeId, Position>, origin: Pos2) -> Option<NodeLink> {
        for &link in self.doc.links.iter().rev() {
            let Some(points) = self.association_curve(link, positions, origin) else { continue; };
            for j in 0..48 {
                let p0 = Self::curve_at(points, j as f32 / 48.0);
                let p1 = Self::curve_at(points, (j + 1) as f32 / 48.0);
                let delta = p1 - p0;
                let d = delta.length_sq();
                let fraction = if d > 0.0 { ((pointer - p0).dot(delta) / d).clamp(0.0, 1.0) } else { 0.0 };
                let closest = p0 + delta * fraction;
                if pointer.distance_sq(closest) < 49.0 { return Some(link); }
            }
        }
        None
    }
    pub(super) fn paint_links(&self, painter: &egui::Painter, positions: &HashMap<NodeId, Position>, origin: Pos2) {
        for &link in &self.doc.links {
            let Some(points) = self.association_curve(link, positions, origin) else { continue };
            let active = self.selected_link.is_some_and(|l| l.normalized() == link.normalized());
            let tint = self.doc.legacy_link_color(link.from, link.to)
                .map(color_from_rgb).unwrap_or_else(|| if active { Color32::from_rgb(68, 120, 240) }
                    else if self.dark { Color32::from_rgb(162, 172, 195) }
                    else { Color32::from_rgb(120, 135, 160) });
            // Sample in screen pixels, then accumulate actual arc length.
            // Dash/gap are constant (9px/6px) regardless of link length.
            let mut previous = Self::curve_at(points, 0.0);
            let mut traveled = 0.0_f32;
            for j in 1..=192 {
                let current = Self::curve_at(points, j as f32 / 192.0);
                let segment = current.distance(previous);
                if segment > f32::EPSILON {
                    let mut used = 0.0;
                    while used < segment {
                        let phase = (traveled + used) % 15.0;
                        let length = (if phase < 9.0 { 9.0 - phase } else { 15.0 - phase })
                            .min(segment - used);
                        if phase < 9.0 {
                            painter.line_segment([
                                previous.lerp(current, used / segment),
                                previous.lerp(current, (used + length) / segment),
                            ], Stroke::new(if active { 2.5 } else { 1.8 }, tint));
                        }
                        used += length.max(0.0001);
                    }
                    traveled += segment;
                }
                previous = current;
            }
        }
        if let Some(from) = self.link_origin {
            if let Some(&pos) = positions.get(&from) {
                if let Some(node) = self.doc.node(from) {
                    if let Some(mouse) = painter.ctx().input(|i| i.pointer.hover_pos()) {
                        let start = self.screen_rect(node, pos, origin).center();
                        painter.line_segment([start, mouse], Stroke::new(1.5, ACCENT));
                    }
                }
            }
        }
    }
}
