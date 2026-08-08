pub(super) fn with_card_bridge(mut html: String) -> String {
    const BRIDGE: &str = r#"<script>
(function() {
  var pendingFit = 0;
  var resizeFit = 0;
  var reportedStreamUpdate = false;
  var fontReady = false;
  var pendingFontFit = false;
  var fontStarted = performance.now();
  function reportCardState(phase, error) {
    var body = document.body;
    var style = body ? getComputedStyle(body) : null;
    window.parent.postMessage({
      type: 'card_diagnostic',
      phase: phase,
      payload_len: body ? body.innerHTML.length : 0,
      text_len: body ? (body.innerText || body.textContent || '').trim().length : 0,
      opacity: style ? style.opacity : '',
      error: error ? String(error.message || error) : null
    }, '*');
  }
  function requestFit(streaming) {
    window.__SGT_STREAMING__ = Boolean(streaming);
    if (!fontReady) {
      pendingFontFit = true;
      return;
    }
    clearTimeout(pendingFit);
    var run = function() {
      pendingFit = 0;
      if (window._sgtFitting) {
        pendingFit = setTimeout(run, 16);
      } else if (typeof window.__SGT_RUN_FIT__ === 'function') {
        window.__SGT_RUN_FIT__(Boolean(window.__SGT_STREAMING__));
      }
    };
    run();
  }
  window.__SGT_REQUEST_FIT__ = requestFit;
  window.addEventListener('message', function(event) {
    if (event.source !== window.parent || !event.data) return;
    if (event.data.type === 'stream_update') {
      try {
        window.__SGT_APPLY_STREAM_UPDATE__({
          html: event.data.html,
          runInlineSizing: true,
          animateNewWords: true,
          smoothScroll: true
        });
        requestFit(true);
        if (!reportedStreamUpdate) {
          reportedStreamUpdate = true;
          reportCardState('stream_applied', null);
        }
      } catch (error) {
        reportCardState('stream_failed', error);
      }
    } else if (event.data.type === 'finalize') {
      try {
        window.__SGT_APPLY_STREAM_UPDATE__({
          html: event.data.html,
          runInlineSizing: false,
          animateNewWords: false,
          smoothScroll: false
        });
        window.__SGT_INIT_STREAM_GRIDS__();
        requestFit(false);
        reportCardState('finalize_applied', null);
      } catch (error) {
        reportCardState('finalize_failed', error);
      }
    } else if (event.data.type === 'run_fit') {
      requestFit(event.data.streaming);
    }
  });
  window.addEventListener('resize', function() {
    clearTimeout(resizeFit);
    resizeFit = setTimeout(function() {
      requestFit(window.__SGT_STREAMING__);
    }, 40);
  });
  document.addEventListener('click', function(event) {
    var anchor = event.target && event.target.closest
      ? event.target.closest('a[href]')
      : null;
    if (!anchor || anchor.target === '_blank' || event.defaultPrevented) return;
    var url = anchor.href;
    if (!/^https?:\/\//i.test(url)) return;
    window.parent.postMessage({ type: 'card_navigation', url: url }, '*');
  }, true);
  window.addEventListener('error', function(event) {
    reportCardState('script_error', event.error || event.message);
  });
  window.addEventListener('unhandledrejection', function(event) {
    reportCardState('promise_rejection', event.reason);
  });
  reportCardState('bridge_ready', null);
  if (!document.fonts || typeof document.fonts.load !== 'function') {
    reportCardState('font_failed', new Error('FontFaceSet is unavailable'));
    return;
  }
  document.fonts.load("400 16px 'Google Sans Flex'").then(function(faces) {
    if (!faces.length || !document.fonts.check("400 16px 'Google Sans Flex'")) {
      throw new Error('Google Sans Flex did not enter the loaded font set');
    }
    fontReady = true;
    document.documentElement.classList.add('sgt-font-ready');
    reportCardState('font_ready_' + Math.round(performance.now() - fontStarted) + 'ms', null);
    if (pendingFontFit) {
      pendingFontFit = false;
      requestFit(window.__SGT_STREAMING__);
    }
  }).catch(function(error) {
    reportCardState('font_failed', error);
  });
})();
</script>"#;
    let stream_runtime = include_str!("stream_runtime.js");
    let bridge = format!("<script>{stream_runtime}</script>{BRIDGE}");
    if let Some(position) = html.to_ascii_lowercase().rfind("</body>") {
        html.insert_str(position, &bridge);
    } else {
        html.push_str(&bridge);
    }
    html
}
