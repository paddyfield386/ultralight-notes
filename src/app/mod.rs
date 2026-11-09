
use crate::canvas_core::{CanvasTarget, ShapeKind, ConnectorEnd, Anchor, ConnectorRoute, TextHorizontal, TextVertical};
use crate::mindmap_core::{layout_with_widths, node_width_text, node_height_text, CopiedNode, Document, Node, NodeId, NodeLink, Offset, Position, ROOT};
use eframe::egui::{self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, io::Write, path::{Path, PathBuf}, time::{Instant, SystemTime, UNIX_EPOCH}};

const ACCENT: Color32 = Color32::from_rgb(101, 79, 221);
const INK: Color32 = Color32::from_rgb(27, 39, 62);
const BG: Color32 = Color32::from_rgb(253, 254, 255);
const PANEL_BG: Color32 = Color32::from_rgb(248, 249, 253);

#[derive(Clone, Copy)]
struct Tint { fill: Color32, line: Color32, border: Color32 }
const TINTS: [Tint; 8] = [
    Tint { fill: Color32::from_rgb(228, 239, 255), line: Color32::from_rgb(86, 145, 245), border: Color32::from_rgb(165, 195, 248) },
    Tint { fill: Color32::from_rgb(224, 248, 232), line: Color32::from_rgb(73, 181, 117), border: Color32::from_rgb(158, 222, 179) },
    Tint { fill: Color32::from_rgb(239, 230, 255), line: Color32::from_rgb(153, 109, 234), border: Color32::from_rgb(204, 181, 247) },
    Tint { fill: Color32::from_rgb(255, 244, 211), line: Color32::from_rgb(241, 181, 64), border: Color32::from_rgb(250, 218, 136) },
    Tint { fill: Color32::from_rgb(255, 228, 235), line: Color32::from_rgb(237, 119, 145), border: Color32::from_rgb(247, 181, 197) },
    Tint { fill: Color32::from_rgb(237, 228, 255), line: Color32::from_rgb(160, 109, 222), border: Color32::from_rgb(204, 180, 241) },
    Tint { fill: Color32::from_rgb(215, 246, 242), line: Color32::from_rgb(52, 181, 170), border: Color32::from_rgb(144, 220, 210) },
    Tint { fill: Color32::from_rgb(233, 237, 247), line: Color32::from_rgb(127, 144, 174), border: Color32::from_rgb(190, 201, 221) },
];
const COLOR_LABELS: [&str; 8] = ["Blue", "Green", "Purple", "Yellow", "Pink", "Lavender", "Light Blue", "Gray"];

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Drag {
    Marquee { start: Pos2, current: Pos2, additive: bool },
    Node(NodeId),
    TextBox(u64),
    ResizeTextBox(u64),
    Shape(u64),
    ResizeShape(u64, ResizeEdge),
    CreateShape {kind:ShapeKind,start:Pos2,current:Pos2},
    CreateLine {start:Pos2,current:Pos2},
    Line(u64), // ID retained for future per-line drag semantics
    LineEndpoint(u64,bool),
    Connector(ConnectorEnd),
    RetargetConnector(u64, bool),
    ConnectorWaypoint(u64, usize),
    ConnectorLabel(u64),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ResizeEdge { TopLeft, Top, TopRight, Right, BottomRight, Bottom, BottomLeft, Left }
impl ResizeEdge {
    fn left(self)->bool {matches!(self,Self::Left|Self::TopLeft|Self::BottomLeft)}
    fn right(self)->bool {matches!(self,Self::Right|Self::TopRight|Self::BottomRight)}
    fn top(self)->bool {matches!(self,Self::Top|Self::TopLeft|Self::TopRight)}
    fn bottom(self)->bool {matches!(self,Self::Bottom|Self::BottomLeft|Self::BottomRight)}
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DropIntent { Child(NodeId), Before(NodeId), After(NodeId) }
impl DropIntent {
    fn target(self) -> NodeId {
        match self { Self::Child(id) | Self::Before(id) | Self::After(id) => id }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileDialog { Open, SaveAs }
#[derive(Clone, Debug)]
enum PendingAction { New, Open(PathBuf), Exit }

#[derive(Clone)]
struct MindmapClipboard {
    text: String,
    forest: Vec<CopiedNode>,
}

enum ClipboardEvent {
    Copy { with_descendants: bool },
    Paste(String),
}
/// View-only preferences, stored separately from the mind-map document.
/// `serde(default)` keeps the older settings.json (recent_files only) readable.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct GridSettings {
    visible: bool,
    spacing: f32,
    dot_radius: f32,
    contrast: u8,
    major_dots: bool,
}
impl Default for GridSettings {
    fn default() -> Self {
        Self { visible: true, spacing: 20.0, dot_radius: 1.0, contrast: 30, major_dots: true }
    }
}
impl GridSettings {
    fn sanitized(mut self) -> Self {
        if !self.spacing.is_finite() { self.spacing = GridSettings::default().spacing; }
        if !self.dot_radius.is_finite() { self.dot_radius = GridSettings::default().dot_radius; }
        self.spacing = self.spacing.clamp(16.0, 96.0);
        self.dot_radius = self.dot_radius.clamp(0.6, 3.0);
        self.contrast = self.contrast.min(100);
        self
    }
}
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Preferences {
    recent_files: Vec<PathBuf>,
    grid: GridSettings,
    dark: bool,
    /// Re-layout the entire tree when a new child/sibling is inserted (default: off).
    reflow_on_add: bool,
    snap_to_grid: bool,
    language: Language,
}

fn preferences_path() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).map(|p| p.join("MindMap").join("settings.json"))
    } else {
        std::env::var_os("HOME").map(PathBuf::from).map(|p| p.join(".config").join("mindmap").join("settings.json"))
    }
}
fn card_height(node: &Node, depth: usize) -> f32 { node_height_text(&node.title, depth) }
fn ensure_json_extension(mut path: PathBuf) -> PathBuf {
    if !path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        path.set_extension("json");
    }
    path
}
/// Write into a sibling temporary file first. A failed save cannot truncate an
/// existing JSON map. Windows rename replacement is handled by std::fs::rename.
fn write_json_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().ok_or("Save先にFile名がありません")?
        .to_string_lossy();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let temp = path.with_file_name(format!(".{name}.{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&temp).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temp); }
    result
}

fn color_from_rgb(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}
fn rgb_of(c: Color32) -> [u8; 3] { [c.r(), c.g(), c.b()] }

fn blend_towards_white(color: Color32, amount: f32) -> Color32 {
    let c = |v: u8| (f32::from(v) * (1.0 - amount) + 255.0 * amount) as u8;
    Color32::from_rgb(c(color.r()), c(color.g()), c(color.b()))
}

struct MindmapApp {
    doc: Document,
    selected: NodeId,
    selection: HashSet<NodeId>,
    drag_moving: Vec<NodeId>,
    /// Unified selection across tree nodes, legacy text and free canvas shapes.
    canvas_selection: HashSet<CanvasTarget>,
    selected_shape: Option<u64>,
    selected_line: Option<u64>,
    context_line: Option<u64>,
    insert_line_mode: bool,
    shape_edit_focus_pending: bool,
    /// Multi-object arrangement grid column count.
    arrange_columns: usize,
    selected_connector: Option<u64>,
    selected_connectors: HashSet<u64>,
    editing_connector: Option<u64>,
    connector_edit_text: String,
    connector_edit_focus_pending: bool,
    insert_connector_route: ConnectorRoute,
    context_connector_point: Offset,
    snap_guides: Vec<(Pos2,Pos2)>,
    context_shape: Option<u64>,
    context_connector: Option<u64>,
    insert_shape_mode: Option<ShapeKind>,
    insert_connector_mode: bool,
    editing_shape: Option<u64>,
    shape_edit_text: String,
    snap_to_grid: bool,
    /// Preserve the starting aspect ratio during Shift+drag resizing.
    shape_resize_aspect: Option<f32>,
    icons: HashMap<&'static str, egui::TextureHandle>,
    fit_pending: bool,
    current_path: Option<PathBuf>,
    recent_files: Vec<PathBuf>,
    ask_discard: bool,
    pending_action: Option<PendingAction>,
    exit_approved: bool,
    grid_settings_open: bool,
    dirty: bool,
    status: String,
    pan: Vec2,
    zoom: f32,
    dragging: Option<Drag>,
    drop_intent: Option<DropIntent>,
    middle_pan: bool,
    suppress_canvas_context_once: bool,
    context_target: Option<NodeId>,
    context_link: Option<NodeLink>,
    selected_link: Option<NodeLink>,
    link_origin: Option<NodeId>,
    search_open: bool,
    search_query: String,
    search_index: usize,
    search_focus_pending: bool,
    context_text_box: Option<u64>,
    clipboard: Option<MindmapClipboard>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    editing: Option<NodeId>,
    edit_original: String,
    edit_text: String,
    edit_focus_pending: bool,
    edit_select_all_pending: bool,
    edit_ignore_enter_once: bool,
    edit_is_new_node: bool,
    selected_text_box: Option<u64>,
    editing_text_box: Option<u64>,
    text_box_editor: String,
    text_box_original: String,
    text_box_focus_pending: bool,
    text_box_is_new: bool,
    insert_text_box_mode: bool,
    ribbon_tab: usize,
    ribbon_compact: bool,
    reflow_on_add: bool,
    ime_composing: bool,
    ime_preedit: String,
    dark: bool,
    language: Language,
    grid: GridSettings,
    center_pending: bool,
    benchmark_ack: Option<PathBuf>,
    last_window_title: String,
    startup: Instant,
    first_ui_recorded: bool,
}

mod selection;
mod annotations;
mod editing;
mod clipboard;
mod history;
mod persistence;
mod color;
mod ribbon;
mod canvas;
mod canvas_geometry;
mod dialogs;
mod search;
mod links;
mod export;
mod canvas_objects;
mod brainstorm;
mod diagram;
mod frame_grid_ui;
mod i18n;

use i18n::Language;

impl MindmapApp {
    fn new(startup: Instant, benchmark_ack: Option<PathBuf>, sample_100: bool) -> Self {
        // Test cases must be deterministic, regardless of a developer's saved
        // %APPDATA%/MindMap/settings.json (especially `reflow_on_add`).
        // The normal GUI build still loads the real user preferences.
        #[cfg(test)]
        let preferences = Preferences::default();
        #[cfg(not(test))]
        let preferences = preferences_path().and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Preferences>(&b).ok())
            .unwrap_or_default();
        let recent_files = preferences.recent_files.into_iter()
            .filter(|path| path.is_file()).take(12).collect();
        Self {
            doc: if sample_100 { Document::sample_100() } else { Document::sample_design() },
            selected: ROOT, selection: HashSet::from([ROOT]), drag_moving: Vec::new(),
            canvas_selection: HashSet::from([CanvasTarget::Node(ROOT)]),
            selected_shape: None, selected_line:None, context_line:None,
            insert_line_mode:false,shape_edit_focus_pending:false,
            arrange_columns:4, selected_connector:None, selected_connectors:HashSet::new(),
            editing_connector:None,connector_edit_text:String::new(),connector_edit_focus_pending:false,
            insert_connector_route:ConnectorRoute::Orthogonal,context_connector_point:Offset::default(),
            snap_guides:Vec::new(),context_shape:None,context_connector:None,
            insert_shape_mode: None, insert_connector_mode: false, editing_shape: None,
            shape_edit_text: String::new(), snap_to_grid: preferences.snap_to_grid,
            shape_resize_aspect:None,
            icons: HashMap::new(), fit_pending: false, current_path: None, recent_files,
            ask_discard: false, pending_action: None, exit_approved: false, grid_settings_open: false,
            dirty: false, status: String::new(), pan: Vec2::ZERO,
            zoom: if sample_100 { 0.78 } else { 0.92 },
            dragging: None, drop_intent: None, middle_pan: false, suppress_canvas_context_once: false, context_target: None,
            context_text_box: None, context_link: None, selected_link: None,
            link_origin: None, search_open: false, search_query: String::new(),
            search_index: 0, search_focus_pending: false,
            clipboard: None, undo: Vec::new(), redo: Vec::new(),
            editing: None, edit_original: String::new(), edit_text: String::new(),
            edit_focus_pending: false, edit_select_all_pending: false,
            edit_ignore_enter_once: false, edit_is_new_node: false,
            selected_text_box: None, editing_text_box: None, text_box_editor: String::new(),
            text_box_original: String::new(), text_box_focus_pending: false,
            text_box_is_new: false, insert_text_box_mode: false, ribbon_tab: 0, ribbon_compact: false,
            reflow_on_add: preferences.reflow_on_add,
            language: preferences.language,
            ime_composing: false, ime_preedit: String::new(),
            dark: preferences.dark, grid: preferences.grid.sanitized(),
            center_pending: false, benchmark_ack, last_window_title: String::new(), startup, first_ui_recorded: false,
        }
    }
}

impl eframe::App for MindmapApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        static FIRST_UI: std::sync::Once = std::sync::Once::new();
        FIRST_UI.call_once(|| diagnostic_mark("first ui frame entered"));
        if !self.first_ui_recorded {
            self.first_ui_recorded = true;
            if let Some(p) = self.benchmark_ack.as_ref() {
                let _ = std::fs::write(p, format!("nodes={},first_ui=true,elapsed_ms={}\n", self.doc.nodes.len(), self.startup.elapsed().as_millis()));
            }
        }
        let ctx = ui.ctx().clone();
        // Also intercept the native window X button when changes are unsaved.
        if ctx.input(|i| i.viewport().close_requested()) && !self.exit_approved {
            // A focused inline editor may still hold unsaved text.
            self.finish_edit(true);
            self.finish_text_box_edit(true);
            if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if !self.ask_discard { self.request_exit(&ctx); }
            } else {
                self.exit_approved = true;
                self.save_preferences();
            }
        }
        self.handle_keys(&ctx);
        self.handle_canvas_keyboard(&ctx);
        egui::Panel::top("compact-ribbon")
            .frame(egui::Frame::new()
                .fill(if self.dark { Color32::from_rgb(35, 41, 55) } else { PANEL_BG })
                .inner_margin(egui::Margin::symmetric(10, 7))
                .stroke(Stroke::new(0.5, Color32::from_rgb(227, 234, 244))))
            .show(ui, |ui| self.ribbon(ui));
        egui::Panel::bottom("status-line")
            .frame(egui::Frame::new().fill(if self.dark { Color32::from_rgb(32, 39, 53) } else { PANEL_BG })
                .inner_margin(egui::Margin::symmetric(16, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Node: {}", self.doc.nodes.len()));
                    if !self.doc.shapes.is_empty() { ui.separator(); ui.label(format!("Shapes: {}", self.doc.shapes.len())); }
                    if !self.doc.lines.is_empty(){ui.separator();ui.label(format!("直Lines: {}",self.doc.lines.len()));}
                    if !self.doc.links.is_empty() { ui.separator(); ui.label(format!("Link: {}", self.doc.links.len())); }
                    ui.separator();
                    ui.label(format!("Selected: {}", if self.selected_link.is_some() {1} else if !self.selected_connectors.is_empty() {self.selected_connectors.len()} else {self.canvas_selection.len()}));
                    if !self.status.is_empty() { ui.separator(); ui.label(&self.status); }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{}%", (self.zoom * 100.0).round() as i32));
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(if self.dark { Color32::from_rgb(24, 29, 41) } else { BG }))
            .show(ui, |ui| self.canvas(ui));
        self.dialogs(&ctx);
        static FIRST_PAINT: std::sync::Once = std::sync::Once::new();
        FIRST_PAINT.call_once(|| diagnostic_mark("first canvas frame completed"));

        self.search_ui(&ctx);
        let title = format!("MindMap — {}{}", self.filename(), if self.dirty { " *" } else { "" });
        if title != self.last_window_title {
            self.last_window_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }
}

/// Opt-in diagnostics for native Windows exits such as 0xc0000005.
/// Windows access violations cannot be captured by `RUST_BACKTRACE` alone.
/// The log deliberately records stages only, no board contents or filenames.
fn diagnostic_mark(stage: &str) {
    if !std::env::args_os().any(|a|a.to_string_lossy() == "--diagnose") {return;}
    let Ok(dir)=std::env::current_dir() else {return};
    let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true)
        .open(dir.join("mindmap-diagnostic.log")) else {return};
    let _=writeln!(f,"{stage}");
    let _=f.flush();
}

pub(crate) fn run() -> eframe::Result {
    diagnostic_mark("run entered");
    let startup = Instant::now();
    let args: Vec<_> = std::env::args_os().collect();
    let benchmark_ack = args.windows(2).find(|w| w[0] == std::ffi::OsStr::new("--benchmark-ack"))
        .map(|w| PathBuf::from(&w[1]));
    let sample_100 = args.iter().any(|s| s.to_string_lossy() == "--sample-100");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("MindMap").with_inner_size([1510.0, 880.0])
            .with_min_inner_size([760.0, 470.0]),
        renderer: eframe::Renderer::Glow,
        system_font_fallback: true,
        ..Default::default()
    };
    diagnostic_mark("before native window / renderer initialization");
    eframe::run_native("mindmap-egui", options, Box::new(move |cc| {
        diagnostic_mark("native app factory entered");
        cc.egui_ctx.set_theme(egui::Theme::Light);
        cc.egui_ctx.global_style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 7.0);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(224, 234, 254);
            style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(235, 241, 254);
            style.visuals.selection.bg_fill = Color32::from_rgb(75, 131, 240);
        });
        let app = MindmapApp::new(startup, benchmark_ack, sample_100);
        diagnostic_mark("app document and preferences initialized");
        if app.dark { cc.egui_ctx.set_theme(egui::Theme::Dark); }
        Ok(Box::new(app))
    }))
}

#[cfg(test)]
mod tests;
