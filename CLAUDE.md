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
    cells and divisions of the front sketch. `shape.rs`: free-form outline: corner treatments,
    and figures (rectangle, hole, slot, polygon with a radius per vertex, notch) subtracted
    from the face with `i_overlay`; also the pure helpers the mouse gestures use. `joints.rs`, `analysis.rs`: contacts, overlaps, screws and drill access.
    `cutlist.rs`: cut list, CSV and stock estimate.
- `crates/tarugo`: the eframe app (binary `tarugo`). The 3D view renders with wgpu to an
  offscreen texture (color + depth, MSAA) shown as an egui image; dimensions, handles and
  warnings are drawn over it with the egui painter.

## Checking the interface

Debug builds run a script from `TARUGO_DEMO` (see `crates/tarugo/src/demo.rs`): actions
separated by `;` that feed fake clicks, drags and keys, and save screenshots as PPM.

```bash
TARUGO_DEMO="wait:40;stage:screws;autoscrews;analyze;shot:/tmp/a.ppm;quit" \
    cargo run -p tarugo -- samples/estante.tarugo
```

Positions are in points, not pixels: divide what a screenshot shows by the display scale.
Never capture with ImageMagick's `import -window`: it grabs the pointer waiting for a click.

## Decisions worth knowing

- Dragging a face that a parameter places (`W`, `H`, `D`) changes the parameter only in the
  front sketch. In the 3D design the part lets go of it (`Drag::via_params`), so one board
  can be stretched without resizing the whole furniture.
- egui samples user textures as gamma-encoded: the scene target is `Rgba8Unorm` and colors
  go in without linearizing.
- The cut list uses the bounding rectangle of every part; free forms never change it.
- The free form is edited in a mode of the 3D design stage (`shape_mode.rs`,
  `App::shape_editor`): the view, the bars and the panel switch to the flat face of one
  part. Cuts may overlap each other and cross the border; a cut off the part, or cuts that
  split it, are reported (`Issue::Shape`). A value set with the mouse becomes a plain number;
  typed formulas stay. Guide lines live in `Shape::guides` (they cut nothing: `is_plain`
  ignores them, `is_empty` does not).
- Free-form mode, handling figures: a selected polygon has a frame a little out of its box
  whose handles scale it (`shape::box_drag` + `Shape::resize`): the opposite side stays, the
  radii follow the side that changed the least, Shift/Ctrl keeps the proportions. The other
  figures are resized by their own handles. Figures turn only by quarters
  (`Shape::rotate_quarter`), so none changes its type. Copy and paste go through the system
  clipboard as text (`shape::feature_to_text`), with `App::shape_clipboard` as a fallback;
  egui delivers Ctrl+C/X/V as `Event::Copy/Cut/Paste`, not as keys.
- Free-form mode, reference: the other parts are drawn flat behind the face (`Face::others`,
  farthest first; their own outline if they lie the same way) and their sides are snap
  lines. `Prefs::shape_ghost` (0 hides them and their snapping) and
  `Prefs::shape_part_opacity` are set from the context bar.
- A joint is screwable if one of its parts is at most `analysis::MAX_THROUGH` thick along
  the joint's axis. Edge-to-edge joints (boards side by side) are not asked for screws.
