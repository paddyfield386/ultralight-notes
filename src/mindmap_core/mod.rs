//! Stable, UI-independent mindmap model and JSON document format.
//! JSON document, tree editing and layout. Deliberately independent of egui.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::frame_grid::FrameGrid;
use crate::canvas_core::{CanvasShape, CanvasLine, CanvasConnector, CanvasGroup, CanvasTarget, ShapeKind, CanvasStyle, ConnectorEnd, validate_shapes};

pub type NodeId = u64;
pub const ROOT: NodeId = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}
impl Offset {
    fn plus(self, rhs: Self) -> Self {
        Self { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

// Retained for JSON compatibility with the original prototype; all branches are now rightward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side { Left, Right }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    #[serde(default, skip_serializing)]
    pub side: Option<Side>,
    pub title: String,
    /// Offset from automatic position, inherited by the node's descendants.
    #[serde(default)]
    pub offset: Offset,
    /// Optional per-node accent override. Default color comes from its root branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u8>,
    /// Children are retained in JSON but excluded from the canvas when folded.
    #[serde(default)]
    pub collapsed: bool,
}

/// A detached, portable representation of a node and all of its descendants.
/// Clipboard copies intentionally carry no document IDs or parent links.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CopiedNode {
    pub title: String,
    pub color: Option<u8>,
    pub offset: Offset,
    pub children: Vec<CopiedNode>,
    #[serde(default)]
    pub collapsed: bool,
}

impl CopiedNode {
    /// Human-readable clipboard text, suitable for pasting into a text editor.
    /// The app separately keeps this tree so pasting it into MindMap preserves
    /// all descendants and their properties.
    pub fn plain_text(&self) -> String {
        fn write_tree(node: &CopiedNode, depth: usize, result: &mut String) {
            if !result.is_empty() { result.push('\n'); }
            result.push_str(&"  ".repeat(depth));
            result.push_str(&node.title.replace('\n', " ").replace('\r', " "));
            for child in &node.children { write_tree(child, depth + 1, result); }
        }
        let mut result = String::new();
        write_tree(self, 0, &mut result);
        result
    }
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(Self::node_count).sum::<usize>()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeGroup {
    pub id: u64,
    /// Visual-only grouping: does not change node parent relationships.
    pub members: Vec<NodeId>,
}
fn initial_group_id() -> u64 { 1 }

/// Floating PowerPoint-like annotation; not a member of any mindmap tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBox {
    pub id: u64,
    pub text: String,
    pub position: Offset,
    #[serde(default = "default_text_width")]
    pub width: f32,
    #[serde(default = "default_text_height")]
    pub height: f32,
    /// Optional RGB background and border colors for independent annotations.
    /// Omitted in old JSON files, which retain their default appearance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<[u8; 3]>,
    /// Transparency is explicit, not an alias for the default background.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fill_transparent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<[u8; 3]>,
    /// Explicit no-outline, separately from the older legacy default border.
    #[serde(default, skip_serializing_if = "is_false")]
    pub border_none: bool,
}
fn is_false(value: &bool) -> bool { !*value }
fn default_text_width() -> f32 { 300.0 }
fn default_text_height() -> f32 { 100.0 }

/// A non-hierarchical, undirected relationship between two nodes.
/// Unlike `Node::parent`, this does not affect tree layout or ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeLink {
    pub from: NodeId,
    pub to: NodeId,
}

impl NodeLink {
    pub fn normalized(self) -> (NodeId, NodeId) {
        if self.from < self.to { (self.from, self.to) } else { (self.to, self.from) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    /// New, independently positioned canvas objects (v2).
    #[serde(default, skip_serializing_if="Vec::is_empty")]
    pub shapes: Vec<CanvasShape>,
    /// World-space free lines; separate from object-following connectors.
    #[serde(default, skip_serializing_if="Vec::is_empty")]
    pub lines: Vec<CanvasLine>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub canvas_groups: Vec<CanvasGroup>,
    #[serde(default="first_canvas_id")]
    pub next_canvas_group_id: u64,
    /// Object-to-object connectors, separate from tree edges and legacy node links.
    #[serde(default, skip_serializing_if="Vec::is_empty")]
    pub connectors: Vec<CanvasConnector>,
    #[serde(default="first_canvas_id")]
    pub next_canvas_id: u64,
    /// Legacy node-link colors, keyed by normalized endpoint IDs.
    #[serde(default, skip_serializing_if="HashMap::is_empty")]
    pub link_colors: HashMap<String, [u8;3]>,
    pub next_id: NodeId,
    pub nodes: Vec<Node>,
    /// Explicit relationships, independent of parent/child edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<NodeLink>,
    /// Floating annotations are independent of the mindmap hierarchy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_boxes: Vec<TextBox>,
    /// Multiple parentless nodes represent independent mindmaps on one board.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<NodeGroup>,
    #[serde(default = "initial_group_id", skip_serializing_if = "is_first_group_id")]
    pub next_group_id: u64,
}
fn first_canvas_id() -> u64 { 1 }
fn is_first_group_id(id: &u64) -> bool { *id == 1 }


impl Document {
    pub fn blank() -> Self {
        Self {
            version: 2,
            shapes: Vec::new(), lines:Vec::new(), connectors: Vec::new(), next_canvas_id: 1, link_colors: HashMap::new(),
            canvas_groups: Vec::new(), next_canvas_group_id: 1,
            next_id: 2,
            groups: Vec::new(), next_group_id: 1,
            links: Vec::new(), text_boxes: Vec::new(),
            nodes: vec![Node {
                id: ROOT, parent: None, side: None, title: "New Mind Map".into(),
                offset: Offset::default(), color: None, collapsed: false,
            }],
        }
    }

    /// Sample matching the approved compact, right-only design.
    pub fn sample_design() -> Self {
        let mut doc = Self::blank();
        doc.node_mut(ROOT).unwrap().title = "Product Development Plan".into();
        let sections: &[(&str, &[&str])] = &[
            ("Market & Users", &["Market Analysis", "User Research", "Needs Assessment"]),
            ("Planning & Requirements", &["Product Direction", "Requirements", "Prioritization"]),
            ("Design", &["System Design", "UI/UX Design"]),
            ("Development", &["Development Plan", "Implementation"]),
            ("Quality & Testing", &["Test Plan", "Quality Improvement"]),
            ("Release & Rollout", &["Release Preparation", "Post-release Support"]),
        ];
        for (section, children) in sections {
            let p = doc.add_child(ROOT, *section).unwrap();
            for title in *children {
                let child = doc.add_child(p, *title).unwrap();
                if *title == "Requirements" {
                    doc.add_child(child, "Functional Requirements");
                    doc.add_child(child, "Non-functional Requirements");
                }
            }
        }
        doc
    }

    /// Stress-test fixture preserved for the original 100-node startup benchmark.
    pub fn sample_100() -> Self {
        let mut doc = Self::blank();
        doc.node_mut(ROOT).unwrap().title = "100-Node Sample".into();
        for topic in ["Goals", "Product", "Users", "Design", "Implementation", "Quality", "Planning", "Release"] {
            let branch = doc.add_child(ROOT, topic).unwrap();
            for j in 0..3 {
                let child = doc.add_child(branch, format!("{topic} / Item {}", j + 1)).unwrap();
                for k in 0..2 {
                    doc.add_child(child, format!("Detail {}", k + 1));
                }
            }
        }
        // 1 + 8 * (1 + 3 + 3*2) = 81
        for i in 0..19 {
            doc.add_child(doc.nodes[i + 2].id, format!("Note {}", i + 1));
        }
        assert_eq!(doc.nodes.len(), 100);
        doc
    }

    pub fn roots(&self) -> Vec<NodeId> {
        self.nodes.iter().filter(|n| n.parent.is_none()).map(|n| n.id).collect()
    }
    pub fn map_root(&self, mut id: NodeId) -> Option<NodeId> {
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(id) { return None; }
            let node = self.node(id)?;
            if let Some(parent) = node.parent { id = parent; } else { return Some(id); }
        }
    }
    /// Create an independent map to the right of existing maps. The anchor is
    /// a world-space offset and is never cleared by auto-aligning its branches.
    pub fn add_map(&mut self, title: impl Into<String>) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        let rightmost = self.roots().into_iter().filter_map(|root| {
            self.node(root).map(|n| n.offset.x)
        }).fold(0.0_f32, f32::max);
        self.nodes.push(Node {
            id, parent: None, side: None, title: title.into(),
            offset: Offset { x: rightmost + 1040.0, y: 0.0 },
            color: None, collapsed: false,
        });
        id
    }
    pub fn add_text_box(&mut self, x: f32, y: f32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.text_boxes.push(TextBox {
            id, text: String::new(), position: Offset { x, y },
            width: default_text_width(), height: default_text_height(),
            fill_color: None, fill_transparent: true, border_color: None, border_none: true,
        });
        id
    }
    pub fn text_box(&self, id: u64) -> Option<&TextBox> {
        self.text_boxes.iter().find(|box_| box_.id == id)
    }
    pub fn text_box_mut(&mut self, id: u64) -> Option<&mut TextBox> {
        self.text_boxes.iter_mut().find(|box_| box_.id == id)
    }
    pub fn delete_text_box(&mut self, id: u64) -> bool {
        let previous = self.text_boxes.len();
        self.text_boxes.retain(|box_| box_.id != id);
        let deleted = self.text_boxes.len() != previous;
        if deleted { self.prune_canvas_connectors(); }
        deleted
    }

    /// AI-ready pure tree: titles and child ordering only. No layout, IDs,
    /// colors, collapse state or group metadata. Text boxes are excluded because
    /// they have no parent-child relationship.
    pub fn pure_tree_json(&self, root: Option<NodeId>) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct Branch<'a> {
            title: &'a str,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            children: Vec<Branch<'a>>,
        }
        fn branch(doc: &Document, id: NodeId) -> Branch<'_> {
            let node = doc.node(id).expect("tree must be valid");
            Branch {
                title: &node.title,
                children: doc.children(id).into_iter().map(|id| branch(doc, id)).collect(),
            }
        }
        let roots = match root {
            Some(id) if self.node(id).is_some() => vec![id],
            _ => self.roots(),
        };
        if roots.len() == 1 {
            serde_json::to_string_pretty(&branch(self, roots[0]))
        } else {
            serde_json::to_string_pretty(&roots.into_iter().map(|id| branch(self, id)).collect::<Vec<_>>())
        }
    }

    /// Insert a distinct relationship, but never mutate either node's parent.
    pub fn add_link(&mut self, from: NodeId, to: NodeId) -> bool {
        if from == to || self.node(from).is_none() || self.node(to).is_none() { return false; }
        let link = NodeLink { from, to };
        if self.links.iter().any(|existing| existing.normalized() == link.normalized()) {
            return false;
        }
        self.links.push(link);
        true
    }
    pub fn remove_link(&mut self, from: NodeId, to: NodeId) -> bool {
        let key = NodeLink { from, to }.normalized();
        let old_len = self.links.len();
        self.links.retain(|link| link.normalized() != key);
        old_len != self.links.len()
    }
    fn prune_links(&mut self) {
        let ids: HashSet<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        self.links.retain(|l| ids.contains(&l.from) && ids.contains(&l.to) && l.from != l.to);
    }

    pub fn group_for(&self, id: NodeId) -> Option<u64> {
        self.groups.iter().find(|g| g.members.contains(&id)).map(|g| g.id)
    }
    pub fn members_of(&self, id: NodeId) -> Vec<NodeId> {
        self.group_for(id)
            .and_then(|gid| self.groups.iter().find(|g| g.id == gid))
            .map(|g| g.members.clone()).unwrap_or_else(|| vec![id])
    }
    pub fn group_nodes(&mut self, selected: &HashSet<NodeId>) -> Option<u64> {
        let mut members: Vec<_> = self.nodes.iter().filter(|n| selected.contains(&n.id))
            .map(|n| n.id).collect();
        if members.len() < 2 { return None; }
        // Detach members from previous groups, dropping groups with <2 members.
        for group in &mut self.groups { group.members.retain(|id| !selected.contains(id)); }
        self.groups.retain(|g| g.members.len() >= 2);
        let id = self.next_group_id;
        self.next_group_id += 1;
        members.dedup();
        self.groups.push(NodeGroup { id, members });
        Some(id)
    }
    #[allow(dead_code)]
    pub fn ungroup(&mut self, id: NodeId) -> bool {
        let old_len = self.groups.len();
        self.groups.retain(|g| !g.members.contains(&id));
        self.groups.len() != old_len
    }
    pub fn visible_ids(&self) -> Vec<NodeId> {
        let mut ids = Vec::new();
        fn walk(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
            out.push(id);
            if !doc.node(id).is_some_and(|n| n.collapsed) {
                for child in doc.children(id) { walk(doc, child, out); }
            }
        }
        for root in self.roots() { walk(self, root, &mut ids); }
        ids
    }
    pub fn reset_branch_offsets(&mut self, id: NodeId) {
        let root = self.map_root(id);
        let mut todo = vec![id];
        while let Some(cur) = todo.pop() {
            todo.extend(self.children(cur));
            // Keep each map's board anchor fixed.
            if Some(cur) != root || cur != id {
                if let Some(node) = self.node_mut(cur) { node.offset = Offset::default(); }
            }
        }
    }
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }
    pub fn children(&self, parent: NodeId) -> Vec<NodeId> {
        self.nodes.iter().filter(|node| node.parent == Some(parent)).map(|node| node.id).collect()
    }
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).and_then(|node| node.parent)
    }
    pub fn depth(&self, id: NodeId) -> usize {
        let (mut cur, mut depth) = (id, 0);
        while let Some(parent) = self.parent(cur) {
            depth += 1;
            cur = parent;
            if depth > self.nodes.len() { break; }
        }
        depth
    }
    pub fn root_branch(&self, id: NodeId) -> Option<NodeId> {
        if self.parent(id).is_none() { return None; }
        let mut current = id;
        loop {
            let parent = self.parent(current)?;
            if self.parent(parent).is_none() { return Some(current); }
            current = parent;
        }
    }
    pub fn contains_descendant(&self, ancestor: NodeId, candidate: NodeId) -> bool {
        let mut cur = candidate;
        while let Some(p) = self.parent(cur) {
            if p == ancestor { return true; }
            cur = p;
        }
        false
    }
    pub fn add_child(&mut self, parent: NodeId, title: impl Into<String>) -> Option<NodeId> {
        self.node(parent)?;
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(Node {
            id, parent: Some(parent), side: None,
            title: title.into(), offset: Offset::default(), color: None, collapsed: false,
        });
        Some(id)
    }
    /// Insert immediately after the selected sibling rather than at the end of the group.
    pub fn add_sibling(&mut self, id: NodeId, title: impl Into<String>) -> Option<NodeId> {
        let parent = self.parent(id)?;
        let index = self.nodes.iter().position(|node| node.id == id)?;
        let next = self.next_id;
        self.next_id += 1;
        self.nodes.insert(index + 1, Node {
            id: next, parent: Some(parent), side: None,
            title: title.into(), offset: Offset::default(), color: None, collapsed: false,
        });
        Some(next)
    }
    /// Copy exactly one node. The original children are not included.
    /// The paste destination gets its own newly allocated node ID.
    pub fn copy_node_only(&self, id: NodeId) -> Option<CopiedNode> {
        let node = self.node(id)?;
        Some(CopiedNode {
            title: node.title.clone(),
            color: node.color,
            offset: node.offset,
            collapsed: node.collapsed,
            children: Vec::new(),
        })
    }

    /// Deep-copy a node without exposing IDs from the source document.
    pub fn copy_subtree(&self, id: NodeId) -> Option<CopiedNode> {
        let node = self.node(id)?;
        Some(CopiedNode {
            title: node.title.clone(),
            color: node.color,
            offset: node.offset,
            collapsed: node.collapsed,
            children: self.children(id).into_iter()
                .filter_map(|child| self.copy_subtree(child)).collect(),
        })
    }

    /// Paste as the next sibling of `selected`, except that selecting the root
    /// pastes as the root's last child. Returns the new selected node ID.
    ///
    /// Descendant offsets are retained; the pasted branch anchor is reset to
    /// the auto-layout so copying a manually positioned branch doesn't overlap.
    pub fn paste_subtree_after(&mut self, selected: NodeId, tree: &CopiedNode) -> Option<NodeId> {
        const MAX_NODES: usize = 10_000;
        if self.node(selected).is_none() ||
            self.nodes.len().checked_add(tree.node_count())? > MAX_NODES { return None; }
        let parent = if self.parent(selected).is_none() { selected } else { self.parent(selected)? };
        let insert_at = if self.parent(selected).is_none() { self.nodes.len() }
            else { self.nodes.iter().position(|n| n.id == selected)? + 1 };
        fn materialize(doc: &mut Document, tree: &CopiedNode, parent: NodeId,
                       is_anchor: bool, into: &mut Vec<Node>) -> NodeId {
            let id = doc.next_id;
            doc.next_id += 1;
            into.push(Node {
                id, parent: Some(parent), side: None,
                title: tree.title.clone(),
                offset: if is_anchor { Offset::default() } else { tree.offset },
                color: tree.color, collapsed: tree.collapsed,
            });
            for child in &tree.children { materialize(doc, child, id, false, into); }
            id
        }
        let mut added = Vec::with_capacity(tree.node_count());
        let new_id = materialize(self, tree, parent, true, &mut added);
        self.nodes.splice(insert_at..insert_at, added);
        Some(new_id)
    }

    pub fn delete_subtree(&mut self, id: NodeId) -> bool {
        if self.node(id).is_none() || id == ROOT || (self.parent(id).is_none() && self.roots().len() <= 1) {
            return false;
        }
        let mut removed = HashSet::from([id]);
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            for child in self.children(current) {
                if removed.insert(child) { stack.push(child); }
            }
        }
        self.nodes.retain(|node| !removed.contains(&node.id));
        self.prune_groups();
        self.prune_links();
        self.prune_canvas_connectors();
        true
    }
    /// Move a subtree onto a target node: target becomes its parent.
    /// The moved branch is auto-placed as the last child of its new parent.
    pub fn reparent(&mut self, moved: NodeId, target: NodeId) -> bool {
        if self.parent(moved).is_none() || moved == target || self.node(target).is_none()
            || self.node(moved).is_none() || self.contains_descendant(moved, target)
            || self.parent(moved) == Some(target)
        {
            return false;
        }
        let Some(index) = self.nodes.iter().position(|n| n.id == moved) else { return false; };
        let mut node = self.nodes.remove(index);
        node.parent = Some(target);
        node.side = None;
        node.offset = Offset::default();
        self.nodes.push(node);
        true
    }

    /// Insert a moved subtree before/after another node in the sibling order.
    /// Works across parents as well as within a sibling group, and rejects cycles.
    /// The moved subtree's manual anchor is reset so auto-layout can reserve space.
    pub fn move_next_to(&mut self, moved: NodeId, sibling: NodeId, after: bool) -> bool {
        if self.parent(moved).is_none() || self.parent(sibling).is_none() || moved == sibling || self.node(moved).is_none() {
            return false;
        }
        let Some(parent) = self.parent(sibling) else { return false; };
        if moved == parent || self.contains_descendant(moved, parent) { return false; }
        let old_parent = self.parent(moved);
        let siblings_before = self.children(parent);
        let original = self.nodes.iter().position(|n| n.id == moved).unwrap();
        let mut node = self.nodes.remove(original);
        node.parent = Some(parent);
        node.side = None;
        node.offset = Offset::default();
        let sibling_index = self.nodes.iter().position(|n| n.id == sibling).unwrap();
        self.nodes.insert(sibling_index + usize::from(after), node);
        old_parent != Some(parent) || self.children(parent) != siblings_before
    }
    pub fn reset_all_offsets(&mut self) {
        for node in &mut self.nodes {
            if node.parent.is_some() { node.offset = Offset::default(); }
        }
    }
    pub fn prune_groups(&mut self) {
        let existing: HashSet<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        for group in &mut self.groups { group.members.retain(|id| existing.contains(id)); }
        self.groups.retain(|g| g.members.len() >= 2);
    }
    pub fn paste_forest_after(&mut self, selected: NodeId, trees: &[CopiedNode]) -> Vec<NodeId> {
        let count: usize = trees.iter().map(CopiedNode::node_count).sum();
        if count == 0 || self.nodes.len().saturating_add(count) > 10_000 { return Vec::new(); }
        let mut result = Vec::new();
        let mut anchor = selected;
        for tree in trees {
            if let Some(id) = self.paste_subtree_after(anchor, tree) {
                result.push(id);
                anchor = id;
            }
        }
        result
    }
    /// Add a free object in world coordinates; type-independent identity and geometry.
    pub fn add_shape(&mut self, kind: ShapeKind, position: Offset) -> u64 {
        let id = self.next_canvas_id;
        self.next_canvas_id += 1;
        let z = self.shapes.iter().map(|s| s.z).max().unwrap_or(-1).saturating_add(1);
        let (width, height) = match kind {
            ShapeKind::Text=>(260.0,90.0),
            ShapeKind::StickyNote=>(240.0,160.0),
            ShapeKind::Frame=>(360.0,240.0),
            _=>(180.0,100.0),
        };
        let mut style = CanvasStyle::default();
        if kind == ShapeKind::Text { style.fill = None; style.outline = None; }
        if kind == ShapeKind::StickyNote {style.fill=Some([255,244,178]);style.outline=None;style.text=[52,50,40];}
        if kind == ShapeKind::Frame {style.fill=Some([247,250,255]);style.outline=Some([126,150,198]);style.stroke_width=2.0;}
        self.shapes.push(CanvasShape { id, kind, position, width, height, z,
            visible: true, locked: false, style,
            text:if kind==ShapeKind::Frame {"Frame".into()}else{String::new()},members:Vec::new(),
            grid:if kind==ShapeKind::Frame {Some(FrameGrid::default())} else {None} });
        id
    }
    pub fn shape(&self, id: u64) -> Option<&CanvasShape> { self.shapes.iter().find(|s| s.id == id) }
    pub fn shape_mut(&mut self, id: u64) -> Option<&mut CanvasShape> { self.shapes.iter_mut().find(|s| s.id == id) }
    pub fn add_line(&mut self,start:Offset,end:Offset)->Option<u64> {
        if ![start.x,start.y,end.x,end.y].iter().all(|v|v.is_finite()) ||
            (start.x-end.x).hypot(start.y-end.y)<1.0 {return None;}
        let id=self.next_canvas_id;self.next_canvas_id+=1;
        let z=self.shapes.iter().map(|s|s.z).chain(self.lines.iter().map(|l|l.z)).max().unwrap_or(-1)+1;
        self.lines.push(CanvasLine{id,start,end,z,color:[109,128,162],width:2.0,dashed:false,visible:true,locked:false});
        Some(id)
    }
    pub fn line(&self,id:u64)->Option<&CanvasLine> {self.lines.iter().find(|l|l.id==id)}
    pub fn line_mut(&mut self,id:u64)->Option<&mut CanvasLine> {self.lines.iter_mut().find(|l|l.id==id)}
    pub fn delete_line(&mut self,id:u64)->bool {
        let before=self.lines.len();self.lines.retain(|l|l.id!=id);
        if before!=self.lines.len() {self.prune_canvas_connectors();true}else{false}
    }
    /// Frame members are explicit; moving objects into its bounds does not attach them.
    pub fn add_frame_for(&mut self,targets:&HashSet<CanvasTarget>,position:Offset,width:f32,height:f32)->Option<u64> {
        if targets.is_empty(){return None;}
        let id=self.add_shape(ShapeKind::Frame,position);
        let members:Vec<_>=targets.iter().copied().filter(|t|self.target_exists(*t) && match *t {
            CanvasTarget::Shape(shape)=>self.shape(shape).is_some_and(|s|s.kind!=ShapeKind::Frame),
            _=>true,
        }).collect();
        if let Some(frame)=self.shape_mut(id) {frame.width=width.max(100.0);frame.height=height.max(80.0);frame.members=members;frame.text="Frame".into();}
        Some(id)
    }
    pub fn target_exists(&self, target: CanvasTarget) -> bool {
        match target { CanvasTarget::Node(id) => self.node(id).is_some(),
            CanvasTarget::TextBox(id) => self.text_box(id).is_some(),
            CanvasTarget::Shape(id) => self.shape(id).is_some(),
            CanvasTarget::Line(id) => self.line(id).is_some() }
    }
    pub fn add_connector(&mut self, from: ConnectorEnd, to: ConnectorEnd, arrow: bool) -> Option<u64> {
        if from.target == to.target || matches!(from.target,CanvasTarget::Line(_))
            || matches!(to.target,CanvasTarget::Line(_))
            || !self.target_exists(from.target) || !self.target_exists(to.target) { return None; }
        let id = self.next_canvas_id; self.next_canvas_id += 1;
        self.connectors.push(CanvasConnector::new(id,from,to,arrow));
        Some(id)
    }
    pub fn canvas_group_for(&self,target:CanvasTarget)->Option<u64> {
        self.canvas_groups.iter().find(|g|g.members.contains(&target)).map(|g|g.id)
    }
    pub fn canvas_group_members(&self,target:CanvasTarget)->Vec<CanvasTarget> {
        self.canvas_group_for(target).and_then(|id|self.canvas_groups.iter().find(|g|g.id==id))
            .map(|g|g.members.clone()).unwrap_or_else(||vec![target])
    }
    pub fn group_canvas(&mut self,targets:&HashSet<CanvasTarget>)->Option<u64> {
        let members:Vec<_>=targets.iter().copied().filter(|t|self.target_exists(*t)).collect();
        if members.len()<2{return None;}
        for group in &mut self.canvas_groups {group.members.retain(|t| !targets.contains(t));}
        self.canvas_groups.retain(|g|g.members.len()>=2);
        let id=self.next_canvas_group_id;self.next_canvas_group_id+=1;
        self.canvas_groups.push(CanvasGroup{id,members});Some(id)
    }
    pub fn ungroup_canvas(&mut self,targets:&HashSet<CanvasTarget>)->bool {
        let len=self.canvas_groups.len();
        self.canvas_groups.retain(|g|!g.members.iter().any(|t|targets.contains(t)));
        len!=self.canvas_groups.len()
    }
    pub fn prune_canvas_connectors(&mut self) {
        let valid: HashSet<_> = self.nodes.iter().map(|n| CanvasTarget::Node(n.id))
            .chain(self.text_boxes.iter().map(|b| CanvasTarget::TextBox(b.id)))
            .chain(self.shapes.iter().map(|s| CanvasTarget::Shape(s.id)))
            .chain(self.lines.iter().map(|l|CanvasTarget::Line(l.id))).collect();
        let mut frame_layouts=Vec::new();
        for frame in &mut self.shapes {
            frame.members.retain(|t|valid.contains(t));
            if frame.kind==ShapeKind::Frame {
                if let Some(grid)=frame.grid.as_mut() {
                    grid.slots.retain(|slot|frame.members.contains(&slot.target));
                    grid.rearrange(None,true);
                    let (w,h)=grid.dimensions();
                    frame.width=w;frame.height=h;
                    for slot in &grid.slots {
                        if let CanvasTarget::Shape(id)=slot.target {
                            let (x,y,width,height)=grid.cell_rect(*slot);
                            frame_layouts.push((id,frame.position.x+x,frame.position.y+y,width,height));
                        }
                    }
                }
            }
        }
        for (id,x,y,w,h) in frame_layouts {
            if let Some(note)=self.shape_mut(id) {
                note.position=Offset{x,y};note.width=w;note.height=h;
            }
        }
        self.connectors.retain(|c| valid.contains(&c.from.target) && valid.contains(&c.to.target));
        for group in &mut self.canvas_groups {group.members.retain(|t|valid.contains(t));}
        self.canvas_groups.retain(|g|g.members.len()>=2);
    }
    pub fn delete_shape(&mut self, id: u64) -> bool {
        let before = self.shapes.len();
        self.shapes.retain(|s| s.id != id);
        if before != self.shapes.len() { self.prune_canvas_connectors(); true } else { false }
    }
    #[allow(dead_code)]
    pub fn normalize_z(&mut self) {
        self.shapes.sort_by_key(|s| (s.z, s.id));
        for (z,s) in self.shapes.iter_mut().enumerate() { s.z = z as i32; }
    }
    #[allow(dead_code)]
    pub fn change_z_for_selection(&mut self, chosen:&HashSet<u64>,direction:i32) -> bool {
        self.normalize_z();
        let before=self.shapes.iter().map(|s|s.id).collect::<Vec<_>>();
        match direction {
            2 | -2 => {
                let mut selected=Vec::new(); let mut rest=Vec::new();
                for shape in self.shapes.drain(..) {
                    if chosen.contains(&shape.id){selected.push(shape)}else{rest.push(shape)}
                }
                self.shapes=if direction==2 {rest.into_iter().chain(selected).collect()}
                    else {selected.into_iter().chain(rest).collect()};
            }
            1 => {
                for i in (0..self.shapes.len().saturating_sub(1)).rev() {
                    if chosen.contains(&self.shapes[i].id)&&!chosen.contains(&self.shapes[i+1].id) {
                        self.shapes.swap(i,i+1);
                    }
                }
            }
            -1 => {
                for i in 1..self.shapes.len() {
                    if chosen.contains(&self.shapes[i].id)&&!chosen.contains(&self.shapes[i-1].id) {
                        self.shapes.swap(i,i-1);
                    }
                }
            }
            _=>{}
        }
        // The Vec order is now authoritative. Sorting by the old `z` here
        // would silently undo the reordering before we could persist it.
        for (z, shape) in self.shapes.iter_mut().enumerate() {
            shape.z = z as i32;
        }
        before != self.shapes.iter().map(|s| s.id).collect::<Vec<_>>()
    }
    pub fn legacy_link_key(from: NodeId, to: NodeId) -> String {
        format!("{}:{}", from.min(to), from.max(to))
    }
    pub fn set_link_color(&mut self, from: NodeId, to: NodeId, rgb: [u8;3]) {
        self.link_colors.insert(Self::legacy_link_key(from, to), rgb);
    }
    pub fn legacy_link_color(&self, from: NodeId, to: NodeId) -> Option<[u8;3]> {
        self.link_colors.get(&Self::legacy_link_key(from,to)).copied()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 && self.version != 2 { return Err("未対応のJSONFileバージョンです".into()); }
        if self.nodes.is_empty() || self.nodes.len() + self.text_boxes.len() + self.shapes.len() + self.lines.len()> 10_000 { return Err("Node数が不正です".into()); }
        let ids: HashSet<_> = self.nodes.iter().map(|node| node.id).collect();
        if ids.len() != self.nodes.len() { return Err("NodeIDが重複しています".into()); }
        let box_ids: HashSet<_> = self.text_boxes.iter().map(|box_| box_.id).collect();
        if box_ids.len() != self.text_boxes.len() || !box_ids.is_disjoint(&ids) {
            return Err("TextボックスIDが重複しています".into());
        }
        for box_ in &self.text_boxes {
            if !box_.position.x.is_finite() || !box_.position.y.is_finite() ||
               !box_.width.is_finite() || !box_.height.is_finite() ||
               !(80.0..=2500.0).contains(&box_.width) || !(40.0..=2500.0).contains(&box_.height) {
                return Err("Textボックスのサイズか座標が不正です".into());
            }
        }
        if self.node(ROOT).is_none() || self.parent(ROOT).is_some() { return Err("ルートNodeが不正です".into()); }
        if self.roots().is_empty() { return Err("マップがありません".into()); }
        for node in &self.nodes {
            if node.parent.is_some_and(|p| !ids.contains(&p)) {
                return Err(format!("親が不明なNode: {}", node.id));
            }
            if !node.offset.x.is_finite() || !node.offset.y.is_finite() {
                return Err("座標に無効な値があります".into());
            }
            if node.color.is_some_and(|i| i >= 8) {
                return Err("色の値が不正です".into());
            }
        }
        let mut visited = HashSet::new();
        let mut todo = self.roots();
        while let Some(id) = todo.pop() {
            if !visited.insert(id) { return Err("ツリーが循環しています".into()); }
            todo.extend(self.children(id));
        }
        if visited.len() != self.nodes.len() { return Err("到達不能なNodeがあります".into()); }
        let mut grouped = HashSet::new();
        for group in &self.groups {
            if group.members.len() < 2 { return Err("グループ人数が不正です".into()); }
            for id in &group.members {
                if !ids.contains(id) || !grouped.insert(*id) { return Err("グループの参照が不正です".into()); }
            }
        }
        if self.groups.iter().map(|g| g.id).collect::<HashSet<_>>().len() != self.groups.len() {
            return Err("グループIDが重複しています".into());
        }
        if self.groups.iter().any(|g| g.id >= self.next_group_id) { return Err("next_group_idが不正です".into()); }
        let mut link_keys = HashSet::new();
        for link in &self.links {
            if link.from == link.to || !ids.contains(&link.from) || !ids.contains(&link.to)
                || !link_keys.insert(link.normalized()) {
                return Err("参照Linkが不正です".into());
            }
        }
        validate_shapes(&self.shapes, &self.lines, &self.connectors, &ids, &box_ids, self.next_canvas_id)?;
        let group_ids:HashSet<u64>=self.canvas_groups.iter().map(|g|g.id).collect();
        if group_ids.len()!=self.canvas_groups.len() || group_ids.iter().any(|id|*id>=self.next_canvas_group_id) {
            return Err("CanvasグループIDが不正です".into());
        }
        let mut grouped=HashSet::new();
        for group in &self.canvas_groups {
            if group.members.len()<2 {return Err("グループの要素数が不足しています".into());}
            for &target in &group.members {
                if !self.target_exists(target)||!grouped.insert(target) {return Err("不正なグループ参照です".into());}
            }
        }
        if self.next_id <= self.nodes.iter().map(|n| n.id).chain(self.text_boxes.iter().map(|n| n.id)).max().unwrap_or(0) {
            return Err("next_idが不正です".into());
        }
        Ok(())
    }
}

mod layout;
#[allow(unused_imports)]
pub use layout::{layout, layout_with_widths, node_width_text, node_height_text, Position};

#[cfg(test)]
use layout::LEAF_SPACING;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod canvas_v2_tests;

#[cfg(test)]
mod brainstorming_v08_tests {
    use super::*;
    #[test]
    fn sticky_has_expected_initial_style_and_geometry() {
        let mut d=Document::blank();
        let id=d.add_shape(ShapeKind::StickyNote,Offset{x:120.0,y:200.0});
        let note=d.shape(id).unwrap();
        assert_eq!((note.width,note.height),(240.0,160.0));
        assert_eq!(note.style.fill,Some([255,244,178]));
        assert_eq!(note.style.outline,None);
        assert!(d.validate().is_ok());
    }
    #[test]
    fn line_is_independent_and_round_trips() {
        let mut d=Document::blank();
        let id=d.add_line(Offset{x:2.0,y:4.0},Offset{x:420.0,y:105.0}).unwrap();
        d.line_mut(id).unwrap().dashed=true;
        assert!(d.add_line(Offset::default(),Offset::default()).is_none());
        let json=serde_json::to_string(&d).unwrap();
        let restored:Document=serde_json::from_str(&json).unwrap();
        assert_eq!(restored.line(id).unwrap().end,Offset{x:420.0,y:105.0});
        assert!(restored.line(id).unwrap().dashed);
        assert!(restored.validate().is_ok());
    }
    #[test]
    fn frame_owns_references_not_enclosed_geometry() {
        let mut d=Document::blank();
        let note=d.add_shape(ShapeKind::StickyNote,Offset{x:0.0,y:0.0});
        let other=d.add_shape(ShapeKind::Rectangle,Offset{x:500.0,y:0.0});
        let frame=d.add_frame_for(&HashSet::from([CanvasTarget::Shape(note)]),
            Offset{x:-20.0,y:-30.0},300.0,220.0).unwrap();
        assert_eq!(d.shape(frame).unwrap().members,vec![CanvasTarget::Shape(note)]);
        assert!(!d.shape(frame).unwrap().members.contains(&CanvasTarget::Shape(other)));
        assert!(d.validate().is_ok());
        d.delete_shape(note);
        assert!(d.shape(frame).unwrap().members.is_empty());
        assert!(d.validate().is_ok());
        d.delete_shape(frame);
        assert!(d.shape(other).is_some());
    }
    #[test]
    fn old_v2_without_new_members_and_lines_still_loads() {
        let d=Document::blank();
        let mut v=serde_json::to_value(&d).unwrap();
        v.as_object_mut().unwrap().remove("lines");
        let mut restored:Document=serde_json::from_value(v).unwrap();
        let id=restored.add_shape(ShapeKind::Ellipse,Offset::default());
        assert!(restored.shape(id).unwrap().members.is_empty());
        assert!(restored.lines.is_empty());
        assert!(restored.validate().is_ok());
    }
    #[test]
    fn frame_cannot_contain_another_frame() {
        let mut d=Document::blank();
        let first=d.add_shape(ShapeKind::Frame,Offset::default());
        let next=d.add_shape(ShapeKind::Frame,Offset{x:500.0,y:0.0});
        d.shape_mut(first).unwrap().members.push(CanvasTarget::Shape(next));
        assert!(d.validate().is_err());
    }
    #[test]
    fn invalid_line_is_rejected() {
        let mut d=Document::blank();
        let id=d.add_line(Offset::default(),Offset{x:90.0,y:20.0}).unwrap();
        d.line_mut(id).unwrap().start.x=f32::NAN;
        assert!(d.validate().is_err());
    }
}
