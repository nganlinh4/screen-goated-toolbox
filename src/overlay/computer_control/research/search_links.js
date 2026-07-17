(() => {
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
    const resultHeading = (anchor) => {
        const selector = "h1,h2,h3,h4,[role='heading']";
        const ownHeading = anchor.matches?.(selector) ? anchor : anchor.querySelector?.(selector);
        if (visible(ownHeading)) return ownHeading;
        const containingHeading = anchor.closest?.(selector);
        return visible(containingHeading) ? containingHeading : null;
    };
    const isPageChrome = (anchor) => Boolean(anchor.closest?.(
        "header,nav,footer,aside,[role='banner'],[role='navigation'],[role='contentinfo']",
    ));
    const resultContainer = (anchor) => {
        if (anchor.closest?.("article,[role='article'],[role='listitem']")) return true;
        return Boolean(anchor.closest?.("main,[role='main']"));
    };
    const links = [];
    const anchors = document.links || [];
    const diagnostics = {
        scanned: 0,
        hidden: 0,
        page_chrome: 0,
        heading_match: 0,
        container_match: 0,
        missing_structure: 0,
        missing_label: 0,
        invalid_href: 0,
    };
    for (let i = 0; i < anchors.length && i < 2000 && links.length < 128; i++) {
        const anchor = anchors[i];
        diagnostics.scanned++;
        if (!visible(anchor)) {
            diagnostics.hidden++;
            continue;
        }
        if (isPageChrome(anchor)) {
            diagnostics.page_chrome++;
            continue;
        }
        const headingMatch = Boolean(resultHeading(anchor));
        const containerMatch = resultContainer(anchor);
        if (!headingMatch && !containerMatch) {
            diagnostics.missing_structure++;
            continue;
        }
        if (headingMatch) diagnostics.heading_match++;
        else diagnostics.container_match++;
        const label = [
            anchor.innerText,
            anchor.getAttribute?.("aria-label"),
            anchor.textContent,
        ].map((value) => String(value || "").replace(/\s+/g, " ").trim())
            .find((value) => value.length > 0) || "";
        const href = anchor.href || "";
        if (!label) {
            diagnostics.missing_label++;
            continue;
        }
        if (href.length > 2048 || !/^https?:\/\//i.test(href)) {
            diagnostics.invalid_href++;
            continue;
        }
        links.push({url: href, label: label.slice(0, 256)});
    }
    return {url: location.href, links, diagnostics};
})()
