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
.region-backdrop{position:absolute;inset:0;width:100%;height:100%;object-fit:fill;pointer-events:none;z-index:0}
.direct-host,.result-frame{position:relative;z-index:1;display:block;width:100%;height:100%;border:0;background:transparent;user-select:text}
.result-card[data-presentation="text_only"] .direct-host,.result-card[data-presentation="text_only"] .result-frame{user-select:text;cursor:text}
</style></head><body><span class="font-prewarm" aria-hidden="true">SGT</span>
<main id="scene"></main><aside id="button-container"></aside>
<script>__SGT_SHAPE_RUNTIME__</script><script>__SGT_DOM_PATCH_RUNTIME__</script><script>__SGT_REVEAL_RUNTIME__</script><script>__SGT_DIRECT_RUNTIME__</script>
<script>window.__SGT_RUN_FIT__ = function(streaming) { __SGT_FIT_RUNTIME__ };
__SGT_SCENE_RUNTIME__
__SGT_HOST_COMMAND_RUNTIME__

</script><script>__SGT_BUTTON_SCRIPT__</script><script>__SGT_BUTTON_SCENE_RUNTIME__</script><script>__SGT_SETTLED_REVEAL_RUNTIME__</script><script>__SGT_RENDERER_BOOTSTRAP__</script>
</body>
</html>"#;

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
