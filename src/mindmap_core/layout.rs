//! Rightward tree layout and card-size estimation, independent of any GUI.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position { pub x: f32, pub y: f32 }

/// Each visible leaf occupies the same vertical slot. Sibling branches have
/// deterministic spacing, independent of editing and insertion order.
pub(super) const LEAF_SPACING: f32 = 76.0;
const DEPTH_GAP: f32 = 65.0;

/// Approximate the card width in logical canvas units. No fixed upper clamp:
/// committed text, not just the editor, must fit its card.
pub fn node_width_text(title: &str, depth: usize) -> f32 {
    let font = if depth == 0 { 19.0 } else if depth == 1 { 16.5 } else { 15.0 };
    let units: f32 = title.lines().map(|line| line.chars().map(|ch| if ch.is_ascii() { 0.61 } else { 1.13 }).sum::<f32>()).fold(0.0, f32::max);
    let minimum = if depth == 0 { 240.0 } else if depth == 1 { 180.0 } else { 144.0 };
    // Extra inset for Japanese glyphs and punctuation around the border.
    (font * units + 76.0).max(minimum)
}

/// Height after commit; keep the single-line default unchanged.
pub fn node_height_text(title: &str, depth: usize) -> f32 {
    let base = if depth == 0 { 62.0 } else if depth == 1 { 49.0 } else { 44.0 };
    base + title.lines().count().saturating_sub(1) as f32 * 24.0
}

#[allow(dead_code)]
pub fn layout(doc: &Document) -> HashMap<NodeId, Position> {
    layout_with_widths(doc, |id, depth| node_width_text(&doc.node(id).unwrap().title, depth))
}

/// Automatically lay out columns based on their widest node, including an
/// active inline editor. This prevents long Japanese titles from overlapping
/// the next hierarchy level even before the user commits with Enter.
pub fn layout_with_widths(doc: &Document, width: impl Fn(NodeId, usize) -> f32) -> HashMap<NodeId, Position> {
    let mut positions = HashMap::with_capacity(doc.nodes.len());
    for root_id in doc.roots() {
        let mut widest: Vec<f32> = Vec::new();
        let visible = map_visible(doc, root_id);
        for id in &visible {
            let depth = doc.depth(*id);
            if widest.len() <= depth { widest.resize(depth + 1, 0.0); }
            widest[depth] = widest[depth].max(width(*id, depth));
        }
        let mut centers = vec![0.0; widest.len()];
        for depth in 1..centers.len() {
            centers[depth] = centers[depth - 1] + widest[depth - 1] * 0.5
                + DEPTH_GAP + widest[depth] * 0.5;
        }
        let root = doc.node(root_id).expect("valid map root");
        positions.insert(root_id, Position { x: root.offset.x, y: root.offset.y });
        if root.collapsed { continue; }
        let branches = doc.children(root_id);
        let total: f32 = branches.iter().map(|&id| span(doc, id)).sum();
        let mut top = -total * 0.5;
        for id in branches {
            let height = span(doc, id);
            place(doc, id, 1, top, root.offset, &centers, &mut positions);
            top += height;
        }
    }
    positions
}
fn map_visible(doc: &Document, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    fn traverse(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
        out.push(id);
        if !doc.node(id).is_some_and(|n| n.collapsed) {
            for child in doc.children(id) { traverse(doc, child, out); }
        }
    }
    traverse(doc, root, &mut out);
    out
}
fn span(doc: &Document, id: NodeId) -> f32 {
    let children = if doc.node(id).is_some_and(|n| n.collapsed) { Vec::new() } else { doc.children(id) };
    let title = doc.node(id).map(|n| n.title.as_str()).unwrap_or("");
    // At depth 1, a single-line node is 49px high. 49 + 27 = 76,
    // matching LEAF_SPACING exactly; the old +28 yielded a 77px step.
    let minimum = LEAF_SPACING.max(node_height_text(title, doc.depth(id)) + 27.0);
    if children.is_empty() { minimum }
    else { children.into_iter().map(|id| span(doc, id)).sum::<f32>().max(minimum) }
}
fn place(doc: &Document, id: NodeId, depth: usize, top: f32, inherited: Offset,
         centers: &[f32], positions: &mut HashMap<NodeId, Position>) {
    let node = doc.node(id).expect("valid node");
    let offset = inherited.plus(node.offset);
    positions.insert(id, Position {
        x: centers[depth] + offset.x,
        y: top + span(doc, id) * 0.5 + offset.y,
    });
    if node.collapsed { return; }
    let mut next_top = top;
    for child in doc.children(id) {
        place(doc, child, depth + 1, next_top, offset, centers, positions);
        next_top += span(doc, child);
    }
}
