# ultralight-notes

A lightweight desktop mind map and diagram editor built with Rust and egui.

## Features

- Create and edit mind maps; add, move, and collapse nodes
- Add shapes, text boxes, sticky notes, and frames
- Create connectors by dragging between connection points, then adjust their routes
- Alignment, snapping, multi-selection, undo, and redo
- Save and load JSON files; export as SVG or PNG
- Light and dark themes, with Japanese text input support

## Requirements and Build

Requires the stable Rust toolchain with Edition 2024 support.

```powershell
cargo test --locked
cargo run --release --locked
```

Windows is the primary development platform. Behavior may vary on other operating systems.

## Basic Usage

Choose a shape from the ribbon and place it on the canvas. Drag from a connection point (○) on one shape to a connection point (○) on another to create a connector. Select a connector to adjust its route using the handles. Edits can be saved as JSON.

## Development

Maintainer: paddyfield386. OpenAI Codex was used as a coding assistant in developing this project. 

## Licenses

This software is dual-licensed under the MIT License or Apache License 2.0. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE) for details.

Some icons use [Tabler Icons](https://github.com/tabler/tabler-icons) (MIT License). See [`assets/tabler/LICENSE`](assets/tabler/LICENSE) for the license text.

Attributions for bundled third-party assets, including fonts, are listed in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md). Rust dependencies are covered by their respective licenses.
