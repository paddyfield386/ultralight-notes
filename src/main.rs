#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]
mod canvas_core;
mod diagram_core;
mod frame_grid;
mod mindmap_core;
mod spreadsheet;
mod app;

fn main() -> eframe::Result {
    app::run()
}
