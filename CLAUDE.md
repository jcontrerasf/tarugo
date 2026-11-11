# Tarugo

Desktop app (Linux, macOS, Windows) to design furniture out of boards and lumber: materials →
front sketch with dimensions → depth → free 3D editing with snapping → screws and their analysis
→ cut list. "Tarugo" is a provisional name. GPL-3.0-or-later.

## Conventions

- Code, identifiers, comments and commits in **English**. Visible texts go **only** in
  `crates/tarugo/src/i18n.rs` (Spanish and English; a text missing in one language does not
  compile).
- No emojis in the UI: Phosphor icons in `icons.rs` (codes of egui-phosphor 0.14).
- Colors only from `theme::pal()`.
- Layout like paint1/EdenBord: top bar, context bar, tools on the left, right panel, status bar.
- egui/eframe 0.37: `egui::Panel::top/left/...show(ui, ..)`, `App::ui` + `App::logic`.
- Lengths are millimetres (`f64`) in the model; the view uses `f32`.
- Before finishing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.

## Architecture

- `crates/tarugo-core`: no UI, everything tested.
  - `expr.rs`: formulas for dimensions. `model.rs`: `Document`, `Part`, `Extent`, `End`.
    Every part is an axis-aligned box; on each axis it is a `Span` between two ends or a
    `Thick` (size fixed by the material) placed by an anchor. An `End` is absolute, a
    reference to a face of another part, or a fraction between two faces: that one structure
    stores dimensions, snapping and parametric designs.
  - `solve.rs`: document → boxes. `edit.rs`: moves that keep references. `front.rs`: frame,
    cells and divisions of the front sketch. `shape.rs`: free-form outline (corners, notches,
    slots, holes). `joints.rs`, `analysis.rs`: contacts, overlaps, screws and drill access.
    `cutlist.rs`: cut list, CSV and stock estimate.
- `crates/tarugo`: the eframe app (binary `tarugo`). The 3D view renders with wgpu to an
  offscreen texture (color + depth, MSAA) shown as an egui image; dimensions, handles and
  warnings are drawn over it with the egui painter.
