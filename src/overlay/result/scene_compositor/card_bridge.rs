pub(super) fn with_card_bridge(mut html: String) -> String {
    const BRIDGE: &str = r#"<script>
(function() {
  var bootstrap = document.currentScript;
  var marker = document.firstChild;
  var frameIdentity = marker && marker.nodeType === 8
    ? String(marker.data || '').replace(/^sgt-card:/, '') : '';
  if (marker && marker.nodeType === 8) marker.remove();
  if (bootstrap) bootstrap.remove();
  // Identity is public routing metadata; the port itself stays private.
  var identityMarker = document.createElement('script');
  identityMarker.type = 'application/json';
  identityMarker.dataset.sgtCardIdentity = frameIdentity;
  document.head.appendChild(identityMarker);
  var frameIdentityMatch = frameIdentity.match(/^(-?\d+):(\d+)$/);
  var cardId = frameIdentityMatch ? frameIdentityMatch[1] : '';
  var documentRevision = frameIdentityMatch ? Number(frameIdentityMatch[2]) : 0;
  var topologyEpoch = '1';
  var pendingFit = 0;
  var resizeFit = 0;
  var fontReady = false;
  var pendingFontFit = false;
  var nextFrameHandle = 0;
  var frameCallbacks = new Map();
  // Create and transfer the private port before any authored script runs.
  // Bind native methods now so later page monkey-patching cannot recover it.
  var channel = typeof MessageChannel === 'function' ? new MessageChannel() : null;
  var sendPort = channel ? channel.port1.postMessage.bind(channel.port1) : null;
  var sendHandshake = window.parent.postMessage.bind(window.parent);
  var applyNative = Reflect.apply;
  var messageData = Object.getOwnPropertyDescriptor(MessageEvent.prototype, 'data').get;
  var eventTarget = Object.getOwnPropertyDescriptor(Event.prototype, 'target').get;
  var eventDefaultPrevented = Object.getOwnPropertyDescriptor(Event.prototype, 'defaultPrevented').get;
  var preventDefault = Event.prototype.preventDefault;
  var getSelection = window.getSelection.bind(window);
  var selectionText = Selection.prototype.toString;
  var closest = Element.prototype.closest;
  var anchorHref = Object.getOwnPropertyDescriptor(HTMLAnchorElement.prototype, 'href').get;
  var startsWith = String.prototype.startsWith;
  var clock = performance.now.bind(performance);
  var traffic = Object.create(null);
  document.documentElement.style.setProperty('user-select', 'text', 'important');
  document.addEventListener('DOMContentLoaded', function() {
    if (document.body) document.body.style.setProperty('user-select', 'text', 'important');
  }, { once: true });
  function postToParent(message) {
    if (!sendPort) return false;
    // Layout callbacks are reachable by page scripts; bound their traffic at
    // the source before it can fill the compositor's event queue.
    var type = message.type;
    if (type === 'copy_selection' || type === 'card_navigation' || type === 'card_interaction') {
      message.topology_epoch = topologyEpoch;
    }
    if (type !== 'copy_selection' && type !== 'card_navigation' && type !== 'card_interaction') {
      var now = clock();
      var budget = traffic[type];
      if (!budget || now - budget.start >= 1000) budget = traffic[type] = { start: now, count: 0 };
      var limit = type === 'frame_request' ? 120 : type === 'fit_request' ? 30 : 16;
      if (++budget.count > limit) return false;
    }
    sendPort(message);
    return true;
  }
  function reportCardState(phase, error, contentRevision) {
    var body = document.body;
    var style = body ? getComputedStyle(body) : null;
    var text = '';
    if (body) {
      var textRoot = body.cloneNode(true);
      textRoot.querySelectorAll('script,style,template,noscript').forEach(function(node) { node.remove(); });
      text = (textRoot.textContent || '').trim();
    }
    postToParent({
      type: 'card_diagnostic',
      phase: phase,
      payload_len: body ? body.innerHTML.length : 0,
      text_len: text.length,
      opacity: style ? style.opacity : '',
      content_revision: Number(contentRevision || 0),
      error: error ? String(error.message || error) : null
    });
  }
  function scheduleBridgeFrame(callback) {
    var handle = ++nextFrameHandle;
    frameCallbacks.set(handle, callback);
    if (!postToParent({ type: 'frame_request', handle: handle })) frameCallbacks.delete(handle);
    return handle;
  }
  function cancelBridgeFrame(handle) {
    frameCallbacks.delete(Number(handle));
  }
  window.__SGT_CANCEL_FIT_FRAME__ = cancelBridgeFrame;
  function queueFit(streaming) {
    window.__SGT_STREAMING__ = Boolean(streaming);
    if (!fontReady) {
      pendingFontFit = true;
      return;
    }
    clearTimeout(pendingFit);
    pendingFit = setTimeout(function() {
      pendingFit = 0;
      postToParent({
        type: 'fit_request',
        streaming: Boolean(window.__SGT_STREAMING__)
      });
    }, 0);
  }
  window.__SGT_REQUEST_FIT__ = queueFit;
  var receivedCommands = Object.create(null);
  function handleHostMessage(event) {
    var data = applyNative(messageData, event, []);
    if (!data) return;
    var commandType = String(data.type || '');
    if (!['run_fit', 'activate_font', 'theme_update', 'frame_tick', 'topology_epoch'].includes(commandType)) return;
    if (String(data.card_id || '') !== cardId) {
      reportCardState('command_rejected', new Error('card identity mismatch'));
      return;
    }
    if (commandType !== 'frame_tick' && !receivedCommands[commandType]) {
      receivedCommands[commandType] = true;
      reportCardState(commandType + '_received', null, data.content_revision);
    }
    if (commandType === 'topology_epoch') {
      topologyEpoch = String(data.epoch || '1');
    } else if (commandType === 'frame_tick') {
      var callback = frameCallbacks.get(Number(data.handle));
      frameCallbacks.delete(Number(data.handle));
      if (callback) callback(Number(data.timestamp) || performance.now());
    } else if (commandType === 'run_fit') {
      window.__SGT_STREAMING__ = Boolean(data.streaming);
      if (typeof window.__SGT_RUN_FIT__ === 'function') {
        window.__SGT_FIT_CONTEXT__ = {
          state: window,
          body: document.body,
          viewport: document.documentElement,
          fontReady: true,
          settleBeforeReveal: Boolean(data.settle_before_reveal),
          scheduleFrame: scheduleBridgeFrame,
          cancelFrame: cancelBridgeFrame,
          reportDiagnostic: function(payload) {
            postToParent({ type: 'fit_diagnostic', payload: payload });
          },
          complete: function() { postToParent({ type: 'fit_complete' }); }
        };
        try {
          window.__SGT_RUN_FIT__(Boolean(window.__SGT_STREAMING__));
        } finally {
          window.__SGT_FIT_CONTEXT__ = null;
        }
      } else {
        postToParent({ type: 'fit_complete' });
      }
    } else if (commandType === 'activate_font') {
      fontReady = true;
      document.documentElement.classList.add('sgt-font-ready');
      if (pendingFontFit) {
        pendingFontFit = false;
        queueFit(window.__SGT_STREAMING__);
      }
    } else if (commandType === 'theme_update') {
      var themeStyle = document.getElementById('sgt-theme-css');
      if (!themeStyle) {
        themeStyle = document.createElement('style');
        themeStyle.id = 'sgt-theme-css';
        (document.head || document.documentElement).appendChild(themeStyle);
      }
      themeStyle.textContent = String(data.css || '');
    }
  }
  if (channel) {
    channel.port1.addEventListener('message', handleHostMessage);
    channel.port1.start();
    sendHandshake({ type: 'card_diagnostic', phase: 'bridge_ready',
      card_id: cardId, document_revision: documentRevision }, '*', [channel.port2]);
  }
  window.addEventListener('copy', function(event) {
    if (!event.isTrusted) return;
    var selection = getSelection();
    var text = selection ? applyNative(selectionText, selection, []) : '';
    if (!text) return;
    applyNative(preventDefault, event, []);
    postToParent({ type: 'copy_selection', text: text });
  }, true);
  window.addEventListener('pointerdown', function(event) {
    if (!event.isTrusted) return;
    postToParent({ type: 'card_interaction' });
  }, true);
  window.addEventListener('resize', function() {
    clearTimeout(resizeFit);
    resizeFit = setTimeout(function() {
      queueFit(window.__SGT_STREAMING__);
    }, 40);
  });
  window.addEventListener('click', function(event) {
    if (!event.isTrusted || applyNative(eventDefaultPrevented, event, [])) return;
    var target = applyNative(eventTarget, event, []);
    if (!(target instanceof Element)) return;
    var anchor = applyNative(closest, target, ['a[href]']);
    if (!anchor) return;
    var url = applyNative(anchorHref, anchor, []);
    if (!applyNative(startsWith, url, ['https://']) && !applyNative(startsWith, url, ['http://'])) return;
    applyNative(preventDefault, event, []);
    postToParent({ type: 'card_navigation', url: url });
  });
  window.addEventListener('error', function(event) {
    reportCardState('script_error', event.error || event.message);
  });
  window.addEventListener('unhandledrejection', function(event) {
    reportCardState('promise_rejection', event.reason);
  });
  reportCardState('bridge_ready', null, 0);
})();
</script>"#;
    // Preserve recognized valid doctypes. For malformed doctypes, insert at
    // byte zero: an HTML parser can terminate a bogus doctype inside quotes.
    let start = html.trim_start_matches('\u{feff}').trim_start();
    let position = leading_html_doctype_end(start)
        .map(|end| html.len() - start.len() + end)
        .unwrap_or(0);
    html.insert_str(position, BRIDGE);
    html
}

fn leading_html_doctype_end(input: &str) -> Option<usize> {
    fn space(byte: u8) -> bool {
        matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
    }
    fn skip_space(bytes: &[u8], index: &mut usize) {
        while bytes.get(*index).is_some_and(|byte| space(*byte)) {
            *index += 1;
        }
    }
    fn quoted(bytes: &[u8], index: &mut usize) -> Option<()> {
        let quote = *bytes.get(*index)?;
        if quote != b'\'' && quote != b'"' {
            return None;
        }
        *index += 1;
        while *bytes.get(*index)? != quote {
            *index += 1;
        }
        *index += 1;
        Some(())
    }

    let bytes = input.as_bytes();
    if !bytes.get(..9)?.eq_ignore_ascii_case(b"<!doctype") || !space(*bytes.get(9)?) {
        return None;
    }
    let mut index = 9;
    skip_space(bytes, &mut index);
    if !bytes.get(index..index + 4)?.eq_ignore_ascii_case(b"html") {
        return None;
    }
    index += 4;
    if *bytes.get(index)? == b'>' {
        return Some(index + 1);
    }
    if !space(*bytes.get(index)?) {
        return None;
    }
    skip_space(bytes, &mut index);
    if *bytes.get(index)? == b'>' {
        return Some(index + 1);
    }
    let public = if bytes.get(index..index + 6)?.eq_ignore_ascii_case(b"public") {
        true
    } else if bytes.get(index..index + 6)?.eq_ignore_ascii_case(b"system") {
        false
    } else {
        return None;
    };
    index += 6;
    if !space(*bytes.get(index)?) {
        return None;
    }
    skip_space(bytes, &mut index);
    quoted(bytes, &mut index)?;
    let previous = index;
    skip_space(bytes, &mut index);
    if public && index > previous && matches!(bytes.get(index), Some(b'\'' | b'"')) {
        quoted(bytes, &mut index)?;
        skip_space(bytes, &mut index);
    }
    (*bytes.get(index)? == b'>').then_some(index + 1)
}

#[cfg(test)]
mod tests {
    use super::{leading_html_doctype_end, with_card_bridge};

    #[test]
    fn isolated_document_bridge_preserves_authored_body_and_scripts() {
        let html = with_card_bridge("<html><body></body></html>".to_string());

        assert!(html.contains("style.setProperty('user-select', 'text', 'important')"));
        assert!(html.contains("postToParent({ type: 'copy_selection', text: text })"));
        assert!(html.contains("postToParent({ type: 'card_navigation', url: url })"));
        assert!(!html.contains("anchor.target === '_blank'"));
        assert!(html.contains("postToParent({ type: 'fit_complete' })"));
        assert!(html.contains("window.__SGT_CANCEL_FIT_FRAME__ = cancelBridgeFrame"));
        assert!(!html.contains("document.body.innerHTML"));
        assert!(!html.contains("window.__SGT_APPLY_STREAM_UPDATE__"));
        assert!(!html.contains("commandType === 'finalize'"));
    }

    #[test]
    fn bootstrap_precedes_authored_script_and_preserves_doctype() {
        for source in [
            "<!DOCTYPE html><html><head><script>authored()</script></head></html>",
            "<!DOCTYPE html SYSTEM \"x>y\"><html><script>authored()</script></html>",
            "<!DOCTYPE html WUT \"x><script>authored()</script>\" >",
            "<html><head><script>authored()</script></head></html>",
        ] {
            let html = with_card_bridge(source.to_string());
            assert!(
                html.find("var bootstrap = document.currentScript").unwrap()
                    < html.find("authored()").unwrap()
            );
            if source.starts_with("<!DOCTYPE html>") || source.contains("SYSTEM") {
                assert!(html.starts_with(source.split_once('>').unwrap().0));
            }
        }
        let valid = "<!doctype html SYSTEM \"x>y\"><html>";
        assert_eq!(
            leading_html_doctype_end(valid),
            Some(valid.find("\">").unwrap() + 2)
        );
        assert_eq!(
            leading_html_doctype_end("<!doctype html WUT \"x><script>authored()</script>\" >"),
            None
        );
    }
}
