(() => {
    const MAX_LINKS = 96;
    const MAX_LABEL_CHARS = 256;
    const MAX_URL_CHARS = 2048;
    const visible = (el) => {
        if (!el || !el.getClientRects || el.getClientRects().length === 0) return false;
        const seen = new Set();
        for (let current = el; current && !seen.has(current);) {
            seen.add(current);
            if (current.hidden || current.getAttribute?.("aria-hidden") === "true") return false;
            const view = current.ownerDocument?.defaultView;
            const style = view ? view.getComputedStyle(current) : null;
            if (style && (style.display === "none" || style.visibility === "hidden" ||
                style.visibility === "collapse" || Number(style.opacity) <= 0)) return false;
            current = current.parentElement || current.getRootNode?.().host || null;
        }
        return true;
    };
    const links = [];
    const seen = new Set();
    const diagnostics = {scanned: 0, hidden: 0, missing_label: 0, invalid_href: 0};
    const anchors = document.links || [];
    for (let i = 0; i < anchors.length && i < 2000 && links.length < MAX_LINKS; i++) {
        const anchor = anchors[i];
        diagnostics.scanned++;
        if (!visible(anchor)) {
            diagnostics.hidden++;
            continue;
        }
        const label = [
            anchor.innerText,
            anchor.getAttribute?.("aria-label"),
            anchor.textContent,
            anchor.getAttribute?.("title"),
        ].map((value) => String(value || "").replace(/\s+/g, " ").trim())
            .find((value) => value.length > 0) || "";
        if (!label) {
            diagnostics.missing_label++;
            continue;
        }
        let target;
        try {
            target = new URL(anchor.href || "", document.baseURI);
            if (!/^https?:$/i.test(target.protocol) || target.username || target.password) {
                throw new Error("unsupported target");
            }
            target.hash = "";
        } catch (_) {
            diagnostics.invalid_href++;
            continue;
        }
        const url = target.href;
        if (url.length > MAX_URL_CHARS || seen.has(url)) {
            diagnostics.invalid_href += Number(url.length > MAX_URL_CHARS);
            continue;
        }
        seen.add(url);
        links.push({url, label: label.slice(0, MAX_LABEL_CHARS)});
    }
    return {url: location.href, links, diagnostics};
})()
