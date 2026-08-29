import deployedCode from "../../ui-shared/material-symbols/deployed_code.svg?raw";
import fitScreen from "../../ui-shared/material-symbols/fit_screen.svg?raw";
import gridOn from "../../ui-shared/material-symbols/grid_on.svg?raw";
import outline from "../../ui-shared/material-symbols/outline.svg?raw";
import palette from "../../ui-shared/material-symbols/palette.svg?raw";
import rotate360 from "../../ui-shared/material-symbols/rotate_360.svg?raw";
import texture from "../../ui-shared/material-symbols/texture.svg?raw";
import viewInAr from "../../ui-shared/material-symbols/view_in_ar.svg?raw";

const VIEWER_ICONS = {
  model: deployedCode,
  toon: texture,
  palette,
  outline,
  rotate: rotate360,
  grid: gridOn,
  wire: viewInAr,
  fit: fitScreen,
};

export function viewerToolbarMarkup() {
  return `<div class="viewer-toolbar" id="viewerToolbar">
    <span class="tool-segment" role="group">
      <button class="view-tool shading-tool" type="button" data-shading="original" data-i18n-title="originalMaterials">${VIEWER_ICONS.model}</button>
      <button class="view-tool shading-tool" type="button" data-shading="toon" data-i18n-title="toonOutline">${VIEWER_ICONS.toon}</button>
      <button class="view-tool shading-tool" type="button" data-shading="parts" data-i18n-title="partColors">${VIEWER_ICONS.palette}</button>
    </span>
    <span class="tool-divider"></span>
    <button class="view-tool active" id="outlineButton" type="button" data-i18n-title="toggleOutline">${VIEWER_ICONS.outline}</button>
    <button class="view-tool" id="rotateButton" type="button" data-i18n-title="toggleRotation">${VIEWER_ICONS.rotate}</button>
    <button class="view-tool" id="gridButton" type="button" data-i18n-title="toggleGrid">${VIEWER_ICONS.grid}</button>
    <button class="view-tool" id="wireButton" type="button" data-i18n-title="toggleWireframe">${VIEWER_ICONS.wire}</button>
    <button class="view-tool" id="fitButton" type="button" data-i18n-title="resetView">${VIEWER_ICONS.fit}</button>
  </div>`;
}
