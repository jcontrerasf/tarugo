//! Phosphor icons (<https://phosphoricons.com>) used in the UI.
//!
//! The font is embedded (see `theme::install`) as a fallback of Inter, so an icon can be mixed
//! with text: `format!("{OPEN} Open…")`. Regular and Fill share codepoints; the filled one is
//! requested with the [`crate::theme::FILL`] family. Codes taken from `egui-phosphor` 0.14.

// File and edit.
pub const OPEN: &str = "\u{E256}"; // folder-open
pub const NEW: &str = "\u{E236}"; // file-plus
pub const SAVE: &str = "\u{E248}"; // floppy-disk
pub const EXPORT: &str = "\u{EAF0}"; // export
pub const UNDO: &str = "\u{E08A}"; // arrow-u-up-left
pub const REDO: &str = "\u{E08C}"; // arrow-u-up-right
pub const DELETE: &str = "\u{E4A6}"; // trash
pub const DUPLICATE: &str = "\u{E1CA}"; // copy
pub const COPY: &str = "\u{E196}"; // clipboard
pub const INFO: &str = "\u{E2CE}"; // info
pub const MENU: &str = "\u{E2F0}"; // list
pub const RECENT: &str = "\u{E1A0}"; // clock-counter-clockwise
pub const FILE: &str = "\u{E230}"; // file
pub const LANGUAGE: &str = "\u{E4A2}"; // translate
pub const THEME_LIGHT: &str = "\u{E472}"; // sun
pub const THEME_DARK: &str = "\u{E330}"; // moon
pub const THEME_SYSTEM: &str = "\u{E18C}"; // circle-half
pub const OK: &str = "\u{E182}"; // check
pub const CANCEL: &str = "\u{E4F6}"; // x
pub const ADD: &str = "\u{E3D4}"; // plus
pub const LOCK: &str = "\u{E2FA}"; // lock
pub const LOCK_OPEN: &str = "\u{E306}"; // lock-open

// Stages.
pub const MATERIALS: &str = "\u{E466}"; // stack
pub const FRONT: &str = "\u{E626}"; // frame-corners
pub const DESIGN: &str = "\u{E1DA}"; // cube
pub const SCREWS: &str = "\u{E86E}"; // screwdriver
pub const CUT_LIST: &str = "\u{E476}"; // table

// Tools.
pub const SELECT: &str = "\u{E1DC}"; // cursor
pub const DIVIDE_VERTICAL: &str = "\u{E870}"; // square-split-horizontal (a vertical board)
pub const DIVIDE_HORIZONTAL: &str = "\u{E874}"; // square-split-vertical (a horizontal board)
pub const ADD_BOARD: &str = "\u{E3F0}"; // rectangle
pub const ADD_STICK: &str = "\u{E6D2}"; // line-segment
pub const SCREW: &str = "\u{E38C}"; // nut
pub const AUTO: &str = "\u{E6B6}"; // magic-wand
pub const ANALYZE: &str = "\u{EADC}"; // list-checks
pub const EQUAL: &str = "\u{E21C}"; // equals
pub const SHAPE: &str = "\u{EC5E}"; // shapes
pub const SNAP: &str = "\u{E680}"; // magnet

// View.
pub const FIT: &str = "\u{E1D0}"; // corners-out
pub const PERSPECTIVE: &str = "\u{EBE6}"; // perspective

// State.
pub const WARNING: &str = "\u{E4E0}"; // warning
pub const ERROR: &str = "\u{E4E2}"; // warning-circle
pub const ALL_GOOD: &str = "\u{E184}"; // check-circle
pub const BOARD: &str = "\u{E45E}"; // square
pub const STICK: &str = "\u{ED86}"; // pipe
pub const MAIN: &str = "\u{E46A}"; // star
pub const NEXT: &str = "\u{E13A}"; // caret-right
