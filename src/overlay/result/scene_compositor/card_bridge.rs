pub(super) fn with_card_bridge(mut html: String) -> String {
    const BRIDGE: &str = r#"<script data-sgt-card-identity="__SGT_CARD_FRAME_IDENTITY__">
(function() {
  var frameIdentity = document.currentScript
    ? String(document.currentScript.dataset.sgtCardIdentity || '') : '';
  var frameIdentityMatch = frameIdentity.match(/^(-?\d+):(\d+)$/);
  var cardPathMatch = location.pathname.match(/\/card\/(-?\d+)/);
  var cardId = frameIdentityMatch ? frameIdentityMatch[1] : (cardPathMatch ? cardPathMatch[1] : '');
  var documentRevision = frameIdentityMatch ? Number(frameIdentityMatch[2])
    : Number(new URLSearchParams(location.search).get('revision') || 0);
  var pendingFit = 0;
  var resizeFit = 0;
  var fontReady = false;
  var pendingFontFit = false;
  var nextFrameHandle = 0;
  var frameCallbacks = new Map();
  document.documentElement.style.setProperty('user-select', 'text', 'important');
  if (document.body) document.body.style.setProperty('user-select', 'text', 'important');
  function postToParent(message, transfer) {
    message.card_id = cardId;
    message.document_revision = documentRevision;
    window.parent.postMessage(message, '*', transfer || []);
  }
  function reportCardState(phase, error, contentRevision, transfer) {
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
    }, transfer);
  }
  function scheduleBridgeFrame(callback) {
    var handle = ++nextFrameHandle;
    frameCallbacks.set(handle, callback);
    postToParent({ type: 'frame_request', handle: handle });
    return handle;
  }
  function cancelBridgeFrame(handle) {
    frameCallbacks.delete(Number(handle));
  }
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
    if (!event.data) return;
    var commandType = String(event.data.type || '');
    if (!['run_fit', 'activate_font', 'theme_update', 'frame_tick'].includes(commandType)) return;
    if (String(event.data.card_id || '') !== cardId) {
      reportCardState('command_rejected', new Error('card identity mismatch'));
      return;
    }
    if (commandType !== 'frame_tick' && !receivedCommands[commandType]) {
      receivedCommands[commandType] = true;
      reportCardState(commandType + '_received', null, event.data.content_revision);
    }
    if (commandType === 'frame_tick') {
      var callback = frameCallbacks.get(Number(event.data.handle));
      frameCallbacks.delete(Number(event.data.handle));
      if (callback) callback(Number(event.data.timestamp) || performance.now());
    } else if (commandType === 'run_fit') {
      window.__SGT_STREAMING__ = Boolean(event.data.streaming);
      if (typeof window.__SGT_RUN_FIT__ === 'function') {
        window.__SGT_FIT_CONTEXT__ = {
          state: window,
          body: document.body,
          viewport: document.documentElement,
          fontReady: true,
          settleBeforeReveal: Boolean(event.data.settle_before_reveal),
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
      themeStyle.textContent = String(event.data.css || '');
    }
  }
  window.addEventListener('message', handleHostMessage);
  var channel = typeof MessageChannel === 'function' ? new MessageChannel() : null;
  if (channel) {
    channel.port1.addEventListener('message', handleHostMessage);
    channel.port1.start();
  }
  document.addEventListener('copy', function(event) {
    var selection = window.getSelection();
    var text = selection ? selection.toString() : '';
    if (!text) return;
    event.preventDefault();
    postToParent({ type: 'copy_selection', text: text });
  }, true);
  document.addEventListener('pointerdown', function() {
    postToParent({ type: 'card_interaction' });
  }, true);
  window.addEventListener('resize', function() {
    clearTimeout(resizeFit);
    resizeFit = setTimeout(function() {
      queueFit(window.__SGT_STREAMING__);
    }, 40);
  });
  document.addEventListener('click', function(event) {
    var anchor = event.target && event.target.closest
      ? event.target.closest('a[href]')
      : null;
    if (!anchor || event.defaultPrevented) return;
    var url = anchor.href;
    if (!/^https?:\/\//i.test(url)) return;
    postToParent({ type: 'card_navigation', url: url });
  }, true);
  window.addEventListener('error', function(event) {
    reportCardState('script_error', event.error || event.message);
  });
  window.addEventListener('unhandledrejection', function(event) {
    reportCardState('promise_rejection', event.reason);
  });
  reportCardState('bridge_ready', null, 0, channel ? [channel.port2] : []);
})();
</script>"#;
    if let Some(position) = html.to_ascii_lowercase().rfind("</body>") {
        html.insert_str(position, BRIDGE);
    } else {
        html.push_str(BRIDGE);
    }
    html
}

#[cfg(test)]
mod tests {
    use super::with_card_bridge;

    #[test]
    fn isolated_document_bridge_preserves_authored_body_and_scripts() {
        let html = with_card_bridge("<html><body></body></html>".to_string());

        assert!(html.contains("style.setProperty('user-select', 'text', 'important')"));
        assert!(html.contains("postToParent({ type: 'copy_selection', text: text })"));
        assert!(html.contains("postToParent({ type: 'card_navigation', url: url })"));
        assert!(!html.contains("anchor.target === '_blank'"));
        assert!(html.contains("postToParent({ type: 'fit_complete' })"));
        assert!(!html.contains("document.body.innerHTML"));
        assert!(!html.contains("window.__SGT_APPLY_STREAM_UPDATE__"));
        assert!(!html.contains("commandType === 'finalize'"));
    }
}
