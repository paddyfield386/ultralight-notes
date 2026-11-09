# Third-party asset notices

This repository includes third-party fonts and icon artwork with licenses distinct from the application source code license.

## Tabler Icons

Some toolbar icons are adapted from [Tabler Icons](https://github.com/tabler/tabler-icons), licensed under the MIT License. Copyright and permission information: [`assets/tabler/LICENSE`](assets/tabler/LICENSE). See [`assets/tabler/SOURCE.md`](assets/tabler/SOURCE.md) for attribution and source details.

## Fonts bundled by egui / epaint

The `epaint_default_fonts 0.37.0` dependency embeds the following font resources. Their upstream license texts are preserved without modification.

| Font resource | License text |
| --- | --- |
| Ubuntu Light | [`third_party_licenses/egui_fonts/UFL.txt`](third_party_licenses/egui_fonts/UFL.txt) — Ubuntu Font Licence 1.0 |
| Hack Regular | [`third_party_licenses/egui_fonts/Hack-Regular.txt`](third_party_licenses/egui_fonts/Hack-Regular.txt) — MIT and Bitstream Vera font license notices |
| egui-icons (subset of emoji-icon-font) | [`third_party_licenses/egui_fonts/emoji-icon-font-mit-license.txt`](third_party_licenses/egui_fonts/emoji-icon-font-mit-license.txt) — MIT |

These texts are taken from the upstream [egui 0.37.0 font directory](https://github.com/emilk/egui/tree/0.37.0/crates/epaint_default_fonts/fonts).

## Rust dependencies

Dependencies listed in `Cargo.lock` are third-party components and remain subject to their individual licenses. The asset notices above are **not** a complete license report for all transitive crates. Anyone distributing pre-built executables should verify the resolved dependency set and provide any applicable additional notices with the binaries.
