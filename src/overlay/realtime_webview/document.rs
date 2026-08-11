//! Unified renderer document for the realtime transcription and translation cards.

pub(super) fn compositor_document(transcription: &str, translation: &str) -> String {
    let transcription = script_string(transcription);
    let translation = script_string(translation);
    format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<style>
*{{box-sizing:border-box}}
html,body{{position:fixed;inset:0;margin:0;overflow:hidden;background:transparent}}
.realtime-card{{position:fixed;display:block;border:0;background:transparent;visibility:hidden}}
.realtime-card.ready.visible{{visibility:visible}}
</style>
</head>
<body>
<iframe id="transcription-card" class="realtime-card" title="Realtime transcription"></iframe>
<iframe id="translation-card" class="realtime-card" title="Realtime translation"></iframe>
<script>
const cards = {{
  transcription: document.getElementById('transcription-card'),
  translation: document.getElementById('translation-card')
}};
const model = {{transcription:null,translation:null}};
const source = {{transcription:{transcription},translation:{translation}}};
let loaded = 0;
let activeInteractionRole = null;

function physicalScale() {{ return window.devicePixelRatio || 1; }}
function applyCard(role) {{
  const frame = cards[role];
  const card = model[role];
  if (!frame || !card) return;
  const scale = physicalScale();
  frame.style.transform = 'translate3d(' + (card.x / scale) + 'px,' + (card.y / scale) + 'px,0)';
  frame.style.width = (card.width / scale) + 'px';
  frame.style.height = (card.height / scale) + 'px';
  frame.classList.toggle('visible', !!card.visible);
}}
function applyLocalDelta(role, body) {{
  const scale = physicalScale();
  const parseDelta = (prefix) => {{
    if (!body.startsWith(prefix)) return null;
    const parts = body.slice(prefix.length).split(',').map(Number);
    return parts.length === 2 && parts.every(Number.isFinite) ? parts : null;
  }};
  let delta = parseDelta('cardDragMove:');
  if (delta && model[role]) {{
    model[role].x += delta[0] * scale;
    model[role].y += delta[1] * scale;
    applyCard(role);
    return;
  }}
  delta = parseDelta('groupDragMove:');
  if (delta) {{
    for (const name of Object.keys(cards)) {{
      if (!model[name]) continue;
      model[name].x += delta[0] * scale;
      model[name].y += delta[1] * scale;
      applyCard(name);
    }}
    return;
  }}
  delta = parseDelta('resize:');
  if (delta && model[role]) {{
    model[role].width = Math.max(200, model[role].width + delta[0] * scale);
    model[role].height = Math.max(100, model[role].height + delta[1] * scale);
    applyCard(role);
    return;
  }}
  if (body.startsWith('toggleMic:') && model.transcription) {{
    model.transcription.visible = body.endsWith('1');
    applyCard('transcription');
  }} else if (body.startsWith('toggleTrans:') && model.translation) {{
    model.translation.visible = body.endsWith('1');
    applyCard('translation');
  }}
}}
window.realtimePostMessage = function(role, body) {{
  if (body === 'interactionStart') activeInteractionRole = role;
  if (body === 'interactionEnd') activeInteractionRole = null;
  applyLocalDelta(role, body);
  window.ipc.postMessage(JSON.stringify({{role,body,scale:physicalScale()}}));
}};
function endActiveInteraction() {{
  if (!activeInteractionRole) return;
  const role = activeInteractionRole;
  activeInteractionRole = null;
  window.ipc.postMessage(JSON.stringify({{role,body:'interactionEnd',scale:physicalScale()}}));
}}
window.addEventListener('mouseup', endActiveInteraction);
window.addEventListener('blur', endActiveInteraction);
window.applyRealtimeLayout = function(next) {{
  model.transcription = next.transcription;
  model.translation = next.translation;
  applyCard('transcription');
  applyCard('translation');
}};
window.runRealtimeCardScript = function(role, script) {{
  const target = cards[role] && cards[role].contentWindow;
  if (!target) return;
  target.eval(script);
}};
for (const role of Object.keys(cards)) {{
  const frame = cards[role];
  frame.addEventListener('load', function() {{
    frame.classList.add('ready');
    applyCard(role);
    loaded += 1;
    if (loaded === 2) {{
      window.ipc.postMessage(JSON.stringify({{role:'compositor',body:'ready',scale:physicalScale()}}));
    }}
  }}, {{once:true}});
  frame.srcdoc = source[role];
}}
</script>
</body>
</html>"#
    )
}

fn script_string(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".into())
        .replace("</script", "<\\/script")
}

#[cfg(test)]
mod tests {
    use super::compositor_document;

    #[test]
    fn one_document_hosts_both_realtime_cards_and_one_native_ipc_bridge() {
        let document = compositor_document("<p>mic</p>", "<p>translation</p>");
        assert_eq!(document.matches("<iframe").count(), 2);
        assert_eq!(document.matches("window.ipc.postMessage").count(), 3);
        assert!(document.contains("applyRealtimeLayout"));
        assert!(document.contains("runRealtimeCardScript"));
        assert_eq!(document.matches("</script>").count(), 1);
    }

    #[test]
    fn iframe_apertures_leave_css_in_charge_of_rounded_corners() {
        let document = compositor_document("", "");
        assert!(document.contains("border:0;background:transparent"));
    }

    #[test]
    fn parent_document_recovers_an_interaction_that_leaves_the_card() {
        let document = compositor_document("", "");
        assert!(document.contains("window.addEventListener('mouseup', endActiveInteraction)"));
        assert!(document.contains("window.addEventListener('blur', endActiveInteraction)"));
    }

    #[test]
    fn realtime_surface_does_not_propagate_a_square_root_background() {
        let css = crate::overlay::html_components::css_main::get("#00C8FF", 16, true);
        assert!(css.contains("background: transparent;"));
        assert!(css.contains("#container {"));
        assert!(css.contains("background: #1C1B1F;"));
    }
}
