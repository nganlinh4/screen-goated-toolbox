(() => {
    const MAX_CAPTURE_CHARS = 262144;
    const MAX_INSPECTED_TEXT_NODES = 50000;
    const MAX_VISIBLE_TEXT_NODES = 20000;
    const MAX_SEMANTIC_CHARS = 4000;
    const MAX_STRUCTURAL_CHARS = 8000;
    const MAX_INSPECTED_SEMANTIC_NODES = 20000;
    const MAX_VISIBLE_SEMANTIC_NODES = 2000;
    const MAX_INSPECTED_STRUCTURAL_NODES = 30000;
    const MAX_VISIBLE_STRUCTURAL_NODES = 4000;
    const MAX_IFRAME_VISITS = 64;
    const MAX_IFRAME_DEPTH = 4;
    const MAX_TITLE_CHARS = 512;
    const MAX_URL_CHARS = 8192;

    const seenDocuments = new WeakSet();
    const capturedDocuments = [];
    let sameOriginIframes = 0;
    let inspectedIframes = 0;
    let invisibleIframes = 0;
    let inaccessibleIframes = 0;
    let iframeCaptureTruncated = false;
    let inspectedTextNodes = 0;
    let eligibleTextNodes = 0;
    let textInspectionTruncated = false;
    let textEvidenceTruncated = false;
    let captureTruncated = false;
    let inspectedSemanticNodes = 0;
    let eligibleSemanticNodes = 0;
    let semanticInspectionTruncated = false;
    let semanticEvidenceTruncated = false;
    let inspectedStructuralNodes = 0;
    let eligibleStructuralNodes = 0;
    let structuralInspectionTruncated = false;
    let structuralEvidenceTruncated = false;

    const visible = (el) => {
        if (!el || !el.getClientRects || el.getClientRects().length === 0) return false;
        const visited = new Set();
        for (let current = el; current && !visited.has(current);) {
            visited.add(current);
            if (current.hidden || current.getAttribute?.("aria-hidden") === "true") return false;
            const view = current.ownerDocument?.defaultView;
            const style = view ? view.getComputedStyle(current) : null;
            if (style && (style.display === "none" || style.visibility === "hidden" ||
                style.visibility === "collapse" || Number(style.opacity) <= 0)) return false;
            current = current.parentElement || current.getRootNode?.().host || null;
        }
        return true;
    };

    const boundedElementText = (el, maxChars) => {
        const parts = [];
        let chars = 0;
        let inspected = 0;
        let eligible = 0;
        const walker = el.ownerDocument.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        let node = walker.nextNode();
        for (; node && chars < maxChars; node = walker.nextNode()) {
            inspected++;
            if (inspected > 500) {
                semanticInspectionTruncated = true;
                break;
            }
            if (!visible(node.parentElement)) continue;
            const clean = String(node.nodeValue || "").replace(/\s+/g, " ").trim();
            if (!clean) continue;
            eligible++;
            if (eligible > 100) {
                semanticEvidenceTruncated = true;
                break;
            }
            const separator = parts.length ? 1 : 0;
            const remaining = maxChars - chars - separator;
            if (remaining <= 0) break;
            const bounded = clean.slice(0, remaining);
            parts.push(bounded);
            chars += bounded.length + separator;
            if (bounded.length < clean.length) semanticEvidenceTruncated = true;
        }
        if (node) semanticEvidenceTruncated = true;
        return parts.join(" ");
    };

    const captureParts = [];
    let captureChars = 0;
    const appendCaptured = (clean) => {
        if (!clean) return;
        const separator = captureParts.length ? 1 : 0;
        const remaining = MAX_CAPTURE_CHARS - captureChars - separator;
        if (remaining <= 0) {
            textEvidenceTruncated = true;
            captureTruncated = true;
            return;
        }
        const bounded = clean.slice(0, remaining);
        captureParts.push(bounded);
        captureChars += bounded.length + separator;
        if (bounded.length < clean.length) {
            textEvidenceTruncated = true;
            captureTruncated = true;
        }
    };

    const frameText = (doc, depth) => {
        if (!doc || seenDocuments.has(doc) || captureTruncated) return;
        seenDocuments.add(doc);
        capturedDocuments.push(doc);
        if (doc.body) {
            const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_TEXT);
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {
                inspectedTextNodes++;
                if (inspectedTextNodes > MAX_INSPECTED_TEXT_NODES) {
                    textInspectionTruncated = true;
                    captureTruncated = true;
                    break;
                }
                const parent = node.parentElement;
                if (!parent || /^(SCRIPT|STYLE|NOSCRIPT|TEMPLATE)$/i.test(parent.tagName)) continue;
                if (!visible(parent)) continue;
                const clean = String(node.nodeValue || "").replace(/\s+/g, " ").trim();
                if (!clean) continue;
                eligibleTextNodes++;
                if (eligibleTextNodes > MAX_VISIBLE_TEXT_NODES) {
                    textEvidenceTruncated = true;
                    captureTruncated = true;
                    break;
                }
                appendCaptured(clean);
                if (captureTruncated) break;
            }
        }
        const frames = doc.getElementsByTagName("iframe");
        if (captureTruncated) {
            if (Array.from(frames).some(visible)) iframeCaptureTruncated = true;
            return;
        }
        if (depth >= MAX_IFRAME_DEPTH) {
            if (Array.from(frames).some(visible)) {
                iframeCaptureTruncated = true;
                captureTruncated = true;
            }
            return;
        }
        for (let index = 0; index < frames.length; index++) {
            if (inspectedIframes >= MAX_IFRAME_VISITS) {
                iframeCaptureTruncated = true;
                captureTruncated = true;
                break;
            }
            const frame = frames[index];
            inspectedIframes++;
            if (!visible(frame)) {
                invisibleIframes++;
                continue;
            }
            let childDocument = null;
            try {
                childDocument = frame.contentDocument;
            } catch (_) {}
            if (childDocument) {
                sameOriginIframes++;
                try {
                    appendCaptured("[iframe]");
                    frameText(childDocument, depth + 1);
                } catch (_) {
                    sameOriginIframes--;
                    inaccessibleIframes++;
                }
            } else {
                inaccessibleIframes++;
            }
            if (captureTruncated) break;
        }
    };

    const semanticAnnotations = (documents) => {
        const lines = [];
        const seen = new Set();
        let chars = 0;
        const add = (line) => {
            const clean = String(line || "").replace(/\s+/g, " ").trim();
            if (!clean || seen.has(clean)) return true;
            const separator = lines.length ? 1 : 0;
            const remaining = MAX_SEMANTIC_CHARS - chars - separator;
            if (remaining <= 0) {
                semanticEvidenceTruncated = true;
                return false;
            }
            const bounded = clean.slice(0, remaining);
            seen.add(clean);
            lines.push(bounded);
            chars += bounded.length + separator;
            if (bounded.length < clean.length || chars >= MAX_SEMANTIC_CHARS) {
                semanticEvidenceTruncated = true;
            }
            return chars < MAX_SEMANTIC_CHARS;
        };
        for (const doc of documents) {
            const root = doc.body || doc.documentElement;
            if (!root) continue;
            const walker = doc.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            for (let el = walker.nextNode(); el; el = walker.nextNode()) {
                inspectedSemanticNodes++;
                if (inspectedSemanticNodes > MAX_INSPECTED_SEMANTIC_NODES) {
                    semanticInspectionTruncated = true;
                    return lines.join("\n");
                }
                const tag = el.tagName?.toLowerCase();
                if (tag !== "s" && tag !== "del" && tag !== "a") continue;
                if (tag === "a" && !el.hasAttribute("href")) continue;
                if (!visible(el)) continue;
                eligibleSemanticNodes++;
                if (eligibleSemanticNodes > MAX_VISIBLE_SEMANTIC_NODES) {
                    semanticEvidenceTruncated = true;
                    return lines.join("\n");
                }
                const text = boundedElementText(el, 500);
                if (!text) continue;
                if (tag === "s" || tag === "del") {
                    if (!add("[deleted text] " + text)) return lines.join("\n");
                    continue;
                }
                let origin = "";
                try {
                    const target = new URL(el.href, doc.baseURI);
                    if (/^https?:$/i.test(target.protocol)) origin = target.origin;
                } catch (_) {}
                if (origin && !add("[link] " + text + " -> " + origin)) {
                    return lines.join("\n");
                }
            }
        }
        return lines.join("\n");
    };

    const structuralAnnotations = (documents) => {
        const lines = [];
        const seen = new Set();
        let chars = 0;
        const add = (line) => {
            const clean = String(line || "").replace(/\s+/g, " ").trim();
            if (!clean || seen.has(clean)) return true;
            const separator = lines.length ? 1 : 0;
            const remaining = MAX_STRUCTURAL_CHARS - chars - separator;
            if (remaining <= 0) {
                structuralEvidenceTruncated = true;
                return false;
            }
            const bounded = clean.slice(0, remaining);
            lines.push(bounded);
            seen.add(clean);
            chars += bounded.length + separator;
            if (bounded.length < clean.length || chars >= MAX_STRUCTURAL_CHARS) {
                structuralEvidenceTruncated = true;
            }
            return chars < MAX_STRUCTURAL_CHARS;
        };
        const boundedText = (el, maxChars) => {
            if (!el) return "";
            const parts = [];
            let localChars = 0;
            const walker = el.ownerDocument.createTreeWalker(el, NodeFilter.SHOW_TEXT);
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {
                inspectedStructuralNodes++;
                if (inspectedStructuralNodes > MAX_INSPECTED_STRUCTURAL_NODES) {
                    structuralInspectionTruncated = true;
                    break;
                }
                if (!visible(node.parentElement)) continue;
                const clean = String(node.nodeValue || "").replace(/\s+/g, " ").trim();
                if (!clean) continue;
                eligibleStructuralNodes++;
                if (eligibleStructuralNodes > MAX_VISIBLE_STRUCTURAL_NODES) {
                    structuralEvidenceTruncated = true;
                    break;
                }
                const remaining = maxChars - localChars - (parts.length ? 1 : 0);
                if (remaining <= 0) {
                    structuralEvidenceTruncated = true;
                    break;
                }
                const bounded = clean.slice(0, remaining);
                parts.push(bounded);
                localChars += bounded.length + (parts.length > 1 ? 1 : 0);
                if (bounded.length < clean.length) structuralEvidenceTruncated = true;
            }
            return parts.join(" ");
        };
        const tableSelector = "table,[role='table'],[role='grid'],[role='treegrid']";
        const rowSelector = "tr,[role='row']";
        const cellSelector = "th,td,[role='columnheader'],[role='rowheader'],[role='cell'],[role='gridcell']";
        for (const doc of documents) {
            const tables = doc.querySelectorAll?.(tableSelector) || [];
            for (let tableIndex = 0; tableIndex < tables.length && tableIndex < 24; tableIndex++) {
                const table = tables[tableIndex];
                if (!visible(table) || table.parentElement?.closest?.(tableSelector)) continue;
                const caption = table.querySelector?.("caption") || null;
                const label = boundedText(caption, 240) ||
                    String(table.getAttribute?.("aria-label") || "").replace(/\s+/g, " ").trim();
                if (!add(label ? `[table] ${label}` : `[table ${tableIndex + 1}]`)) return lines.join("\n");
                const rows = table.querySelectorAll?.(rowSelector) || [];
                for (let rowIndex = 0; rowIndex < rows.length && rowIndex < 64; rowIndex++) {
                    const row = rows[rowIndex];
                    if (!visible(row) || row.closest?.(tableSelector) !== table) continue;
                    const cells = Array.from(row.querySelectorAll?.(cellSelector) || [])
                        .filter((cell) => visible(cell) && cell.closest?.(rowSelector) === row)
                        .slice(0, 24)
                        .map((cell) => boundedText(cell, 320))
                        .filter(Boolean);
                    if (cells.length && !add(`[row] ${cells.join(" | ")}`)) return lines.join("\n");
                }
            }
            const lists = doc.querySelectorAll?.("dl") || [];
            for (let listIndex = 0; listIndex < lists.length && listIndex < 32; listIndex++) {
                const list = lists[listIndex];
                if (!visible(list) || list.parentElement?.closest?.("dl")) continue;
                const children = Array.from(list.children || []);
                for (let index = 0; index < children.length; index++) {
                    const term = children[index];
                    if (term.tagName?.toLowerCase() !== "dt" || !visible(term)) continue;
                    const definitions = [];
                    for (let next = index + 1; next < children.length; next++) {
                        const item = children[next];
                        if (item.tagName?.toLowerCase() === "dt") break;
                        if (item.tagName?.toLowerCase() === "dd" && visible(item)) {
                            const text = boundedText(item, 500);
                            if (text) definitions.push(text);
                        }
                    }
                    const key = boundedText(term, 300);
                    if (key && definitions.length && !add(`[definition] ${key} => ${definitions.join(" | ")}`)) {
                        return lines.join("\n");
                    }
                }
            }
            const root = doc.body || doc.documentElement;
            if (!root) continue;
            const headingSelector = "h1,h2,h3,h4,[role='heading']";
            const walker = doc.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
            let heading = "";
            let sectionParts = [];
            let sectionChars = 0;
            const flushSection = () => {
                if (heading && sectionParts.length) add(`[section] ${heading} :: ${sectionParts.join(" ")}`);
                sectionParts = [];
                sectionChars = 0;
            };
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {
                inspectedStructuralNodes++;
                if (inspectedStructuralNodes > MAX_INSPECTED_STRUCTURAL_NODES) {
                    structuralInspectionTruncated = true;
                    break;
                }
                if (node.nodeType === Node.ELEMENT_NODE && node.matches?.(headingSelector) && visible(node)) {
                    flushSection();
                    heading = boundedText(node, 300);
                    continue;
                }
                if (node.nodeType !== Node.TEXT_NODE || !heading) continue;
                const parent = node.parentElement;
                if (!visible(parent) || parent.closest?.(headingSelector) ||
                    parent.closest?.(`${tableSelector},dl`)) continue;
                const clean = String(node.nodeValue || "").replace(/\s+/g, " ").trim();
                if (!clean) continue;
                eligibleStructuralNodes++;
                if (eligibleStructuralNodes > MAX_VISIBLE_STRUCTURAL_NODES) {
                    structuralEvidenceTruncated = true;
                    break;
                }
                const remaining = 1200 - sectionChars - (sectionParts.length ? 1 : 0);
                if (remaining <= 0) {
                    structuralEvidenceTruncated = true;
                    continue;
                }
                const bounded = clean.slice(0, remaining);
                sectionParts.push(bounded);
                sectionChars += bounded.length + (sectionParts.length > 1 ? 1 : 0);
                if (bounded.length < clean.length) structuralEvidenceTruncated = true;
            }
            flushSection();
            if (chars >= MAX_STRUCTURAL_CHARS) break;
        }
        return lines.join("\n");
    };

    frameText(document, 0);
    const rawTitle = String(document.title || "");
    const rawUrl = String(location.href || "");
    const annotations = semanticAnnotations(capturedDocuments);
    const structure = structuralAnnotations(capturedDocuments);
    return {
        title: rawTitle.slice(0, MAX_TITLE_CHARS),
        titleCharCount: rawTitle.length,
        titleTruncated: rawTitle.length > MAX_TITLE_CHARS,
        url: rawUrl.length <= MAX_URL_CHARS ? rawUrl : "",
        urlTooLong: rawUrl.length > MAX_URL_CHARS,
        text: captureParts.join("\n"),
        semanticAnnotations: annotations,
        structuralAnnotations: structure,
        sameOriginIframes,
        inspectedIframes,
        invisibleIframes,
        inaccessibleIframes,
        iframeCaptureTruncated,
        captureTruncated,
        inspectedTextNodes,
        eligibleTextNodes,
        textInspectionTruncated,
        textEvidenceTruncated,
        semanticTruncated: semanticInspectionTruncated || semanticEvidenceTruncated,
        inspectedSemanticNodes,
        eligibleSemanticNodes,
        semanticInspectionTruncated,
        semanticEvidenceTruncated,
        structuralTruncated: structuralInspectionTruncated || structuralEvidenceTruncated,
        inspectedStructuralNodes,
        eligibleStructuralNodes,
        structuralInspectionTruncated,
        structuralEvidenceTruncated,
    };
})()
