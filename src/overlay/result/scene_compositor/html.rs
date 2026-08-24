pub const DOCUMENT: &str = r#"<!doctype html>
<html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<style id="sgt-theme-css"></style><style id="sgt-controls-theme-css"></style>
<style>__SGT_FONT_FACE____SGT_BUTTON_CSS__
html,body,#scene{position:fixed;inset:0;margin:0;overflow:hidden;background:transparent}
body{font-family:'Google Sans Flex';user-select:none}
#scene{pointer-events:none}
.font-prewarm{position:absolute;visibility:hidden;pointer-events:none;font:400 16px 'Google Sans Flex'}
.result-card{position:absolute;overflow:hidden;border-radius:var(--sgt-box-radius,__SGT_BOX_RADIUS_PX__px);pointer-events:auto;
  left:0;top:0;box-shadow:inset 0 0 0 1px var(--result-outline);contain:layout paint style;user-select:text}
.result-card[data-presentation="text_only"]{background:transparent!important;box-shadow:none;border-radius:3px;pointer-events:auto;user-select:text}
.processing-aura{position:absolute;inset:0;z-index:3;width:100%;height:100%;overflow:visible;
  pointer-events:none;opacity:0;transition:opacity 120ms ease-out}
.processing-track,.processing-runner-glow,.processing-runner{fill:none;vector-effect:non-scaling-stroke}
.processing-track{stroke:var(--processing-track);stroke-linecap:round}
.processing-runner-glow,.processing-runner{stroke-linecap:round;stroke-linejoin:round}
.processing-runner{opacity:.88}
.processing-runner-glow{opacity:.3;filter:blur(1.5px)}
.processing-scan{display:none;stroke:#00ff00;stroke-linecap:round;
  filter:drop-shadow(0 0 2px #00ff00);transform:translate3d(0,0,0)}
.result-card[data-processing="true"] .processing-aura{opacity:1}
.result-card[data-surface="native"]{background:transparent!important;box-shadow:none!important}
.result-card[data-processing-effect="minimal"] .processing-runner-glow,
.result-card[data-processing-effect="minimal"] .processing-runner{display:none}
.result-card[data-processing-effect="minimal"] .processing-scan{display:block;
  animation:sgt-processing-scan .58s ease-in-out infinite alternate paused}
.result-card[data-processing="true"][data-processing-effect="minimal"] .processing-scan{
  animation-play-state:running;will-change:transform}
@keyframes sgt-processing-scan{to{transform:translate3d(0,var(--sgt-scan-travel,0px),0)}}
@media (prefers-reduced-motion:reduce){.processing-scan{animation:none!important}}
.region-backdrop{position:absolute;inset:0;width:100%;height:100%;object-fit:fill;pointer-events:none;z-index:0}
.direct-host,.result-frame{position:absolute;inset:0;z-index:1;display:block;width:100%;height:100%;border:0;background:transparent;user-select:text}
.result-frame{border-radius:inherit;clip-path:inset(0 round var(--sgt-box-radius,__SGT_BOX_RADIUS_PX__px))}
.direct-host[hidden],.result-frame[hidden]{display:none!important}
.result-card[data-presentation="text_only"] .direct-host,.result-card[data-presentation="text_only"] .result-frame{user-select:text;cursor:text}
.resize-handle{position:absolute;z-index:4;touch-action:none;user-select:none}
.resize-handle[data-edge="n"],.resize-handle[data-edge="s"]{left:8px;right:8px;height:6px;cursor:ns-resize}
.resize-handle[data-edge="n"]{top:0}.resize-handle[data-edge="s"]{bottom:0}
.resize-handle[data-edge="e"],.resize-handle[data-edge="w"]{top:8px;bottom:8px;width:6px;cursor:ew-resize}
.resize-handle[data-edge="e"]{right:0}.resize-handle[data-edge="w"]{left:0}
.resize-handle[data-edge="nw"],.resize-handle[data-edge="ne"],.resize-handle[data-edge="sw"],.resize-handle[data-edge="se"]{width:10px;height:10px}
.resize-handle[data-edge="nw"]{left:0;top:0;cursor:nwse-resize}.resize-handle[data-edge="ne"]{right:0;top:0;cursor:nesw-resize}
.resize-handle[data-edge="sw"]{left:0;bottom:0;cursor:nesw-resize}.resize-handle[data-edge="se"]{right:0;bottom:0;cursor:nwse-resize}
</style></head><body><span class="font-prewarm" aria-hidden="true">SGT</span>
<main id="scene"></main><aside id="button-container"></aside>
<script>__SGT_SHAPE_RUNTIME__</script><script>__SGT_DOM_PATCH_RUNTIME__</script><script>__SGT_REVEAL_RUNTIME__</script><script>__SGT_DIRECT_RUNTIME__</script><script>__SGT_PROCESSING_RUNTIME__</script><script>__SGT_RESIZE_RUNTIME__</script>
<script>__SGT_SURFACE_RUNTIME__
window.__SGT_RUN_FIT__ = function(streaming) { __SGT_FIT_RUNTIME__ };
__SGT_SCENE_RUNTIME__
__SGT_HOST_COMMAND_RUNTIME__

</script><script>__SGT_BUTTON_SCRIPT__</script><script>__SGT_BUTTON_SCENE_RUNTIME__</script><script>__SGT_SETTLED_REVEAL_RUNTIME__</script><script>__SGT_RENDERER_BOOTSTRAP__</script>
</body>
</html>"#;

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
