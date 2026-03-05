use std::sync::{Arc, Mutex, OnceLock};

use resvg::usvg::{Options, Tree};
use tiny_skia::{Pixmap, Transform};

const DEFAULT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("../dist/cursor-default-screenstudio.svg");
const TEXT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("../dist/cursor-text-screenstudio.svg");
const POINTER_SCREENSTUDIO_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-screenstudio.svg");
const OPENHAND_SCREENSTUDIO_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-screenstudio.svg");
const CLOSEHAND_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-closehand-screenstudio.svg");
const WAIT_SCREENSTUDIO_SVG: &[u8] = include_bytes!("../dist/cursor-wait-screenstudio.svg");
const APPSTARTING_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-appstarting-screenstudio.svg");
const CROSSHAIR_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-crosshair-screenstudio.svg");
const RESIZE_NS_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-ns-screenstudio.svg");
const RESIZE_WE_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-we-screenstudio.svg");
const RESIZE_NWSE_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nwse-screenstudio.svg");
const RESIZE_NESW_SCREENSTUDIO_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nesw-screenstudio.svg");
const DEFAULT_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-default-macos26.svg");
const TEXT_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-text-macos26.svg");
const POINTER_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-macos26.svg");
const OPENHAND_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-macos26.svg");
const CLOSEHAND_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-macos26.svg");
const WAIT_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-wait-macos26.svg");
const APPSTARTING_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-macos26.svg");
const CROSSHAIR_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-macos26.svg");
const RESIZE_NS_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-macos26.svg");
const RESIZE_WE_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-macos26.svg");
const RESIZE_NWSE_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-macos26.svg");
const RESIZE_NESW_MACOS26_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-macos26.svg");
const DEFAULT_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtcute.svg");
const TEXT_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtcute.svg");
const POINTER_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtcute.svg");
const OPENHAND_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtcute.svg");
const CLOSEHAND_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtcute.svg");
const WAIT_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtcute.svg");
const APPSTARTING_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-sgtcute.svg");
const CROSSHAIR_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtcute.svg");
const RESIZE_NS_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtcute.svg");
const RESIZE_WE_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtcute.svg");
const RESIZE_NWSE_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-sgtcute.svg");
const RESIZE_NESW_SGTCUTE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-sgtcute.svg");
const DEFAULT_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtcool.svg");
const TEXT_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtcool.svg");
const POINTER_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtcool.svg");
const OPENHAND_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtcool.svg");
const CLOSEHAND_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtcool.svg");
const WAIT_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtcool.svg");
const APPSTARTING_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-sgtcool.svg");
const CROSSHAIR_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtcool.svg");
const RESIZE_NS_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtcool.svg");
const RESIZE_WE_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtcool.svg");
const RESIZE_NWSE_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-sgtcool.svg");
const RESIZE_NESW_SGTCOOL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-sgtcool.svg");
const DEFAULT_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtai.svg");
const TEXT_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtai.svg");
const POINTER_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtai.svg");
const OPENHAND_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtai.svg");
const CLOSEHAND_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtai.svg");
const WAIT_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtai.svg");
const APPSTARTING_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-sgtai.svg");
const CROSSHAIR_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtai.svg");
const RESIZE_NS_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtai.svg");
const RESIZE_WE_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtai.svg");
const RESIZE_NWSE_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-sgtai.svg");
const RESIZE_NESW_SGTAI_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-sgtai.svg");
const DEFAULT_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtpixel.svg");
const TEXT_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtpixel.svg");
const POINTER_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtpixel.svg");
const OPENHAND_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtpixel.svg");
const CLOSEHAND_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtpixel.svg");
const WAIT_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtpixel.svg");
const APPSTARTING_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-sgtpixel.svg");
const CROSSHAIR_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtpixel.svg");
const RESIZE_NS_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtpixel.svg");
const RESIZE_WE_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtpixel.svg");
const RESIZE_NWSE_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-sgtpixel.svg");
const RESIZE_NESW_SGTPIXEL_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-sgtpixel.svg");
const DEFAULT_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-default-jepriwin11.svg");
const TEXT_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-text-jepriwin11.svg");
const POINTER_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-jepriwin11.svg");
const OPENHAND_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-jepriwin11.svg");
const CLOSEHAND_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-jepriwin11.svg");
const WAIT_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-wait-jepriwin11.svg");
const APPSTARTING_JEPRIWIN11_SVG: &[u8] =
    include_bytes!("../dist/cursor-appstarting-jepriwin11.svg");
const CROSSHAIR_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-jepriwin11.svg");
const RESIZE_NS_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-jepriwin11.svg");
const RESIZE_WE_JEPRIWIN11_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-jepriwin11.svg");
const RESIZE_NWSE_JEPRIWIN11_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nwse-jepriwin11.svg");
const RESIZE_NESW_JEPRIWIN11_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nesw-jepriwin11.svg");
const DEFAULT_SGTWATERMELON_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtwatermelon.svg");
const TEXT_SGTWATERMELON_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtwatermelon.svg");
const POINTER_SGTWATERMELON_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtwatermelon.svg");
const OPENHAND_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-openhand-sgtwatermelon.svg");
const CLOSEHAND_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-closehand-sgtwatermelon.svg");
const WAIT_SGTWATERMELON_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtwatermelon.svg");
const APPSTARTING_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-appstarting-sgtwatermelon.svg");
const CROSSHAIR_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-crosshair-sgtwatermelon.svg");
const RESIZE_NS_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-ns-sgtwatermelon.svg");
const RESIZE_WE_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-we-sgtwatermelon.svg");
const RESIZE_NWSE_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nwse-sgtwatermelon.svg");
const RESIZE_NESW_SGTWATERMELON_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nesw-sgtwatermelon.svg");
const DEFAULT_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtfastfood.svg");
const TEXT_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtfastfood.svg");
const POINTER_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtfastfood.svg");
const OPENHAND_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtfastfood.svg");
const CLOSEHAND_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtfastfood.svg");
const WAIT_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtfastfood.svg");
const APPSTARTING_SGTFASTFOOD_SVG: &[u8] =
    include_bytes!("../dist/cursor-appstarting-sgtfastfood.svg");
const CROSSHAIR_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtfastfood.svg");
const RESIZE_NS_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtfastfood.svg");
const RESIZE_WE_SGTFASTFOOD_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtfastfood.svg");
const RESIZE_NWSE_SGTFASTFOOD_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nwse-sgtfastfood.svg");
const RESIZE_NESW_SGTFASTFOOD_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nesw-sgtfastfood.svg");
const DEFAULT_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtveggie.svg");
const TEXT_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtveggie.svg");
const POINTER_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtveggie.svg");
const OPENHAND_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtveggie.svg");
const CLOSEHAND_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtveggie.svg");
const WAIT_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtveggie.svg");
const APPSTARTING_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-sgtveggie.svg");
const CROSSHAIR_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtveggie.svg");
const RESIZE_NS_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtveggie.svg");
const RESIZE_WE_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtveggie.svg");
const RESIZE_NWSE_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-sgtveggie.svg");
const RESIZE_NESW_SGTVEGGIE_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-sgtveggie.svg");
const DEFAULT_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtvietnam.svg");
const TEXT_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtvietnam.svg");
const POINTER_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtvietnam.svg");
const OPENHAND_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtvietnam.svg");
const CLOSEHAND_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtvietnam.svg");
const WAIT_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtvietnam.svg");
const APPSTARTING_SGTVIETNAM_SVG: &[u8] =
    include_bytes!("../dist/cursor-appstarting-sgtvietnam.svg");
const CROSSHAIR_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtvietnam.svg");
const RESIZE_NS_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtvietnam.svg");
const RESIZE_WE_SGTVIETNAM_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtvietnam.svg");
const RESIZE_NWSE_SGTVIETNAM_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nwse-sgtvietnam.svg");
const RESIZE_NESW_SGTVIETNAM_SVG: &[u8] =
    include_bytes!("../dist/cursor-resize-nesw-sgtvietnam.svg");
const DEFAULT_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-default-sgtkorea.svg");
const TEXT_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-text-sgtkorea.svg");
const POINTER_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-pointer-sgtkorea.svg");
const OPENHAND_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-openhand-sgtkorea.svg");
const CLOSEHAND_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-closehand-sgtkorea.svg");
const WAIT_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-wait-sgtkorea.svg");
const APPSTARTING_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-appstarting-sgtkorea.svg");
const CROSSHAIR_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-crosshair-sgtkorea.svg");
const RESIZE_NS_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-resize-ns-sgtkorea.svg");
const RESIZE_WE_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-resize-we-sgtkorea.svg");
const RESIZE_NWSE_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nwse-sgtkorea.svg");
const RESIZE_NESW_SGTKOREA_SVG: &[u8] = include_bytes!("../dist/cursor-resize-nesw-sgtkorea.svg");

pub(super) const CURSOR_ATLAS_COLS: u32 = 9;
pub(super) const CURSOR_ATLAS_SLOTS: u32 = CURSOR_SVG_DATA.len() as u32;
pub(super) const CURSOR_ATLAS_ROWS: u32 = CURSOR_ATLAS_SLOTS.div_ceil(CURSOR_ATLAS_COLS);
pub(super) const CURSOR_TILE_SIZE: u32 = 512;

// Add new cursor pack SVGs here — SLOTS, ROWS, and shader constants auto-update.
const CURSOR_SVG_DATA: &[&[u8]] = &[
    // screenstudio
    DEFAULT_SCREENSTUDIO_SVG,
    TEXT_SCREENSTUDIO_SVG,
    POINTER_SCREENSTUDIO_SVG,
    OPENHAND_SCREENSTUDIO_SVG,
    CLOSEHAND_SCREENSTUDIO_SVG,
    WAIT_SCREENSTUDIO_SVG,
    APPSTARTING_SCREENSTUDIO_SVG,
    CROSSHAIR_SCREENSTUDIO_SVG,
    RESIZE_NS_SCREENSTUDIO_SVG,
    RESIZE_WE_SCREENSTUDIO_SVG,
    RESIZE_NWSE_SCREENSTUDIO_SVG,
    RESIZE_NESW_SCREENSTUDIO_SVG,
    // macos26
    DEFAULT_MACOS26_SVG,
    TEXT_MACOS26_SVG,
    POINTER_MACOS26_SVG,
    OPENHAND_MACOS26_SVG,
    CLOSEHAND_MACOS26_SVG,
    WAIT_MACOS26_SVG,
    APPSTARTING_MACOS26_SVG,
    CROSSHAIR_MACOS26_SVG,
    RESIZE_NS_MACOS26_SVG,
    RESIZE_WE_MACOS26_SVG,
    RESIZE_NWSE_MACOS26_SVG,
    RESIZE_NESW_MACOS26_SVG,
    // sgtcute
    DEFAULT_SGTCUTE_SVG,
    TEXT_SGTCUTE_SVG,
    POINTER_SGTCUTE_SVG,
    OPENHAND_SGTCUTE_SVG,
    CLOSEHAND_SGTCUTE_SVG,
    WAIT_SGTCUTE_SVG,
    APPSTARTING_SGTCUTE_SVG,
    CROSSHAIR_SGTCUTE_SVG,
    RESIZE_NS_SGTCUTE_SVG,
    RESIZE_WE_SGTCUTE_SVG,
    RESIZE_NWSE_SGTCUTE_SVG,
    RESIZE_NESW_SGTCUTE_SVG,
    // sgtcool
    DEFAULT_SGTCOOL_SVG,
    TEXT_SGTCOOL_SVG,
    POINTER_SGTCOOL_SVG,
    OPENHAND_SGTCOOL_SVG,
    CLOSEHAND_SGTCOOL_SVG,
    WAIT_SGTCOOL_SVG,
    APPSTARTING_SGTCOOL_SVG,
    CROSSHAIR_SGTCOOL_SVG,
    RESIZE_NS_SGTCOOL_SVG,
    RESIZE_WE_SGTCOOL_SVG,
    RESIZE_NWSE_SGTCOOL_SVG,
    RESIZE_NESW_SGTCOOL_SVG,
    // sgtai
    DEFAULT_SGTAI_SVG,
    TEXT_SGTAI_SVG,
    POINTER_SGTAI_SVG,
    OPENHAND_SGTAI_SVG,
    CLOSEHAND_SGTAI_SVG,
    WAIT_SGTAI_SVG,
    APPSTARTING_SGTAI_SVG,
    CROSSHAIR_SGTAI_SVG,
    RESIZE_NS_SGTAI_SVG,
    RESIZE_WE_SGTAI_SVG,
    RESIZE_NWSE_SGTAI_SVG,
    RESIZE_NESW_SGTAI_SVG,
    // sgtpixel
    DEFAULT_SGTPIXEL_SVG,
    TEXT_SGTPIXEL_SVG,
    POINTER_SGTPIXEL_SVG,
    OPENHAND_SGTPIXEL_SVG,
    CLOSEHAND_SGTPIXEL_SVG,
    WAIT_SGTPIXEL_SVG,
    APPSTARTING_SGTPIXEL_SVG,
    CROSSHAIR_SGTPIXEL_SVG,
    RESIZE_NS_SGTPIXEL_SVG,
    RESIZE_WE_SGTPIXEL_SVG,
    RESIZE_NWSE_SGTPIXEL_SVG,
    RESIZE_NESW_SGTPIXEL_SVG,
    // jepriwin11
    DEFAULT_JEPRIWIN11_SVG,
    TEXT_JEPRIWIN11_SVG,
    POINTER_JEPRIWIN11_SVG,
    OPENHAND_JEPRIWIN11_SVG,
    CLOSEHAND_JEPRIWIN11_SVG,
    WAIT_JEPRIWIN11_SVG,
    APPSTARTING_JEPRIWIN11_SVG,
    CROSSHAIR_JEPRIWIN11_SVG,
    RESIZE_NS_JEPRIWIN11_SVG,
    RESIZE_WE_JEPRIWIN11_SVG,
    RESIZE_NWSE_JEPRIWIN11_SVG,
    RESIZE_NESW_JEPRIWIN11_SVG,
    // sgtwatermelon
    DEFAULT_SGTWATERMELON_SVG,
    TEXT_SGTWATERMELON_SVG,
    POINTER_SGTWATERMELON_SVG,
    OPENHAND_SGTWATERMELON_SVG,
    CLOSEHAND_SGTWATERMELON_SVG,
    WAIT_SGTWATERMELON_SVG,
    APPSTARTING_SGTWATERMELON_SVG,
    CROSSHAIR_SGTWATERMELON_SVG,
    RESIZE_NS_SGTWATERMELON_SVG,
    RESIZE_WE_SGTWATERMELON_SVG,
    RESIZE_NWSE_SGTWATERMELON_SVG,
    RESIZE_NESW_SGTWATERMELON_SVG,
    // sgtfastfood
    DEFAULT_SGTFASTFOOD_SVG,
    TEXT_SGTFASTFOOD_SVG,
    POINTER_SGTFASTFOOD_SVG,
    OPENHAND_SGTFASTFOOD_SVG,
    CLOSEHAND_SGTFASTFOOD_SVG,
    WAIT_SGTFASTFOOD_SVG,
    APPSTARTING_SGTFASTFOOD_SVG,
    CROSSHAIR_SGTFASTFOOD_SVG,
    RESIZE_NS_SGTFASTFOOD_SVG,
    RESIZE_WE_SGTFASTFOOD_SVG,
    RESIZE_NWSE_SGTFASTFOOD_SVG,
    RESIZE_NESW_SGTFASTFOOD_SVG,
    // sgtveggie
    DEFAULT_SGTVEGGIE_SVG,
    TEXT_SGTVEGGIE_SVG,
    POINTER_SGTVEGGIE_SVG,
    OPENHAND_SGTVEGGIE_SVG,
    CLOSEHAND_SGTVEGGIE_SVG,
    WAIT_SGTVEGGIE_SVG,
    APPSTARTING_SGTVEGGIE_SVG,
    CROSSHAIR_SGTVEGGIE_SVG,
    RESIZE_NS_SGTVEGGIE_SVG,
    RESIZE_WE_SGTVEGGIE_SVG,
    RESIZE_NWSE_SGTVEGGIE_SVG,
    RESIZE_NESW_SGTVEGGIE_SVG,
    // sgtvietnam
    DEFAULT_SGTVIETNAM_SVG,
    TEXT_SGTVIETNAM_SVG,
    POINTER_SGTVIETNAM_SVG,
    OPENHAND_SGTVIETNAM_SVG,
    CLOSEHAND_SGTVIETNAM_SVG,
    WAIT_SGTVIETNAM_SVG,
    APPSTARTING_SGTVIETNAM_SVG,
    CROSSHAIR_SGTVIETNAM_SVG,
    RESIZE_NS_SGTVIETNAM_SVG,
    RESIZE_WE_SGTVIETNAM_SVG,
    RESIZE_NWSE_SGTVIETNAM_SVG,
    RESIZE_NESW_SGTVIETNAM_SVG,
    // sgtkorea
    DEFAULT_SGTKOREA_SVG,
    TEXT_SGTKOREA_SVG,
    POINTER_SGTKOREA_SVG,
    OPENHAND_SGTKOREA_SVG,
    CLOSEHAND_SGTKOREA_SVG,
    WAIT_SGTKOREA_SVG,
    APPSTARTING_SGTKOREA_SVG,
    CROSSHAIR_SGTKOREA_SVG,
    RESIZE_NS_SGTKOREA_SVG,
    RESIZE_WE_SGTKOREA_SVG,
    RESIZE_NWSE_SGTKOREA_SVG,
    RESIZE_NESW_SGTKOREA_SVG,
];

type TileCache = Mutex<Vec<Option<Arc<Vec<u8>>>>>;

static CURSOR_TILE_CACHE: OnceLock<TileCache> = OnceLock::new();

fn cursor_tile_cache() -> &'static TileCache {
    CURSOR_TILE_CACHE.get_or_init(|| Mutex::new(vec![None; CURSOR_ATLAS_SLOTS as usize]))
}

fn render_cursor_tile_rgba(slot: u32) -> Option<Vec<u8>> {
    if slot >= CURSOR_ATLAS_SLOTS {
        return None;
    }

    let tile_size = CURSOR_TILE_SIZE;
    let center = tile_size as f32 / 2.0;
    let mut tile = Pixmap::new(tile_size, tile_size).unwrap();
    let target = tile_size as f32;

    let opt = Options::default();
    let tree = Tree::from_data(CURSOR_SVG_DATA[slot as usize], &opt).ok()?;
    let svg_size = tree.size();
    let svg_w = svg_size.width().max(1.0);
    let svg_h = svg_size.height().max(1.0);
    let base_scale = target / svg_w.max(svg_h);
    let hotspot_px_x = (svg_w * 0.5) * base_scale;
    let hotspot_px_y = (svg_h * 0.5) * base_scale;
    let x = center - hotspot_px_x;
    let y = center - hotspot_px_y;
    let ts = Transform::from_translate(x, y).pre_scale(base_scale, base_scale);
    resvg::render(&tree, ts, &mut tile.as_mut());

    Some(tile.data().to_vec())
}

pub(super) fn get_or_render_cursor_tile(slot: u32) -> Option<Arc<Vec<u8>>> {
    if slot >= CURSOR_ATLAS_SLOTS {
        return None;
    }

    {
        let cache = cursor_tile_cache().lock().unwrap();
        if let Some(bytes) = &cache[slot as usize] {
            return Some(Arc::clone(bytes));
        }
    }

    let rendered = Arc::new(render_cursor_tile_rgba(slot)?);
    let mut cache = cursor_tile_cache().lock().unwrap();
    if let Some(bytes) = &cache[slot as usize] {
        Some(Arc::clone(bytes))
    } else {
        cache[slot as usize] = Some(Arc::clone(&rendered));
        Some(rendered)
    }
}

pub(super) fn dedupe_valid_slots(slots: &[u32]) -> Vec<u32> {
    let mut seen = [false; CURSOR_ATLAS_SLOTS as usize];
    let mut out = Vec::with_capacity(slots.len().max(1));
    for slot in slots {
        let idx = *slot as usize;
        if idx >= CURSOR_ATLAS_SLOTS as usize || seen[idx] {
            continue;
        }
        seen[idx] = true;
        out.push(*slot);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}
