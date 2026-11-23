(function() {
  // Keep ordinary user-opened links in the existing navigation view. No page
  // message is trusted to request host focus or mutate host history.
  document.addEventListener('click', function(event) {
    if (!event.isTrusted || event.defaultPrevented || event.button !== 0 ||
        event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const anchor = event.target && event.target.closest ? event.target.closest('a[href]') : null;
    if (anchor && anchor.target.toLowerCase() === '_blank' && /^https?:\/\//i.test(anchor.href)) {
      anchor.target = '_self';
    }
  }, true);
})();
