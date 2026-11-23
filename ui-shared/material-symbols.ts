import add from "./material-symbols/add.svg?raw";
import autoAwesome from "./material-symbols/auto_awesome.svg?raw";
import check from "./material-symbols/check.svg?raw";
import close from "./material-symbols/close.svg?raw";
import compareArrows from "./material-symbols/compare_arrows.svg?raw";
import deleteIcon from "./material-symbols/delete.svg?raw";
import deployedCode from "./material-symbols/deployed_code.svg?raw";
import download from "./material-symbols/download.svg?raw";
import drawCollage from "./material-symbols/draw_collage.svg?raw";
import edit from "./material-symbols/edit.svg?raw";
import fitScreen from "./material-symbols/fit_screen.svg?raw";
import folder from "./material-symbols/folder.svg?raw";
import graphicEq from "./material-symbols/graphic_eq.svg?raw";
import gridOn from "./material-symbols/grid_on.svg?raw";
import image from "./material-symbols/image.svg?raw";
import info from "./material-symbols/info.svg?raw";
import keyboardArrowDown from "./material-symbols/keyboard_arrow_down.svg?raw";
import outline from "./material-symbols/outline.svg?raw";
import palette from "./material-symbols/palette.svg?raw";
import redo from "./material-symbols/redo.svg?raw";
import remove from "./material-symbols/remove.svg?raw";
import rotate360 from "./material-symbols/rotate_360.svg?raw";
import save from "./material-symbols/save.svg?raw";
import selectAll from "./material-symbols/select_all.svg?raw";
import settings from "./material-symbols/settings.svg?raw";
import stop from "./material-symbols/stop.svg?raw";
import texture from "./material-symbols/texture.svg?raw";
import undo from "./material-symbols/undo.svg?raw";
import viewInAr from "./material-symbols/view_in_ar.svg?raw";
import volumeOff from "./material-symbols/volume_off.svg?raw";
import volumeUp from "./material-symbols/volume_up.svg?raw";
import zoomIn from "./material-symbols/zoom_in.svg?raw";
import zoomOut from "./material-symbols/zoom_out.svg?raw";

export const MATERIAL_SYMBOLS = {
  add,
  autoAwesome,
  check,
  close,
  compareArrows,
  delete: deleteIcon,
  deployedCode,
  download,
  drawCollage,
  edit,
  fitScreen,
  folder,
  graphicEq,
  gridOn,
  image,
  info,
  keyboardArrowDown,
  outline,
  palette,
  redo,
  remove,
  rotate360,
  save,
  selectAll,
  settings,
  stop,
  texture,
  undo,
  viewInAr,
  volumeOff,
  volumeUp,
  zoomIn,
  zoomOut,
} as const;

export type MaterialSymbolName = keyof typeof MATERIAL_SYMBOLS;

/**
 * Shared symbol markup at an explicit pixel size, optionally classed, and hidden
 * from assistive technology (the owning control carries the accessible name).
 */
export function materialSymbolSvg(name: MaterialSymbolName, size: number, className = ""): string {
  const classAttr = className ? ` class="${className}"` : "";
  return MATERIAL_SYMBOLS[name].trim().replace(
    'width="1em" height="1em"',
    `width="${size}" height="${size}" aria-hidden="true" focusable="false"${classAttr}`,
  );
}

/** The single path of a shared symbol, for renderers that build their own <svg>. */
export function materialSymbolPath(name: MaterialSymbolName): string {
  const match = /\sd="([^"]+)"/.exec(MATERIAL_SYMBOLS[name]);
  if (!match) throw new Error(`Material symbol ${name} has no path data`);
  return match[1];
}
