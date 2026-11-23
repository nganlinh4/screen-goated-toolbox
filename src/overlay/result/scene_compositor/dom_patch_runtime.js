(function() {
    var authoredStyles = new WeakMap();
    var allowedElements = new Set([
        'A', 'B', 'BLOCKQUOTE', 'BR', 'CODE', 'DEL', 'DIV', 'EM', 'H1', 'H2', 'H3',
        'H4', 'H5', 'H6', 'HR', 'I', 'IMG', 'INPUT', 'LI', 'OL', 'P', 'PRE', 'S',
        'SPAN', 'STRONG', 'SUB', 'SUP', 'TABLE', 'TBODY', 'TD', 'TH', 'THEAD', 'TR', 'U', 'UL'
    ]);
    var discardedElements = new Set([
        'SCRIPT', 'STYLE', 'IFRAME', 'OBJECT', 'EMBED', 'FORM', 'META', 'LINK',
        'BASE', 'SVG', 'MATH', 'TEMPLATE', 'AUDIO', 'VIDEO'
    ]);
    function safeUrl(value, image) {
        var trimmed = value.trim();
        if (image && /^data:image\/(?:png|jpeg|gif|webp);base64,[a-z0-9+/=\s]+$/i.test(trimmed)) return true;
        try {
            var url = new URL(trimmed, document.baseURI);
            return url.protocol === 'http:' || url.protocol === 'https:';
        } catch (_error) {
            return false;
        }
    }
    function safeAttribute(element, attribute) {
        var name = attribute.name.toLowerCase();
        var tag = element.tagName;
        if (name === 'class' || name === 'title') return true;
        if (name === 'href' && tag === 'A') return safeUrl(attribute.value, false);
        if (name === 'src' && tag === 'IMG') return safeUrl(attribute.value, true);
        if (tag === 'IMG' && ['alt', 'width', 'height'].includes(name)) return true;
        if (tag === 'INPUT' && name === 'type') return attribute.value.toLowerCase() === 'checkbox';
        if (tag === 'INPUT' && ['disabled', 'checked'].includes(name)) return true;
        if (tag === 'OL' && name === 'start') return /^\d+$/.test(attribute.value);
        if ((tag === 'TD' || tag === 'TH') && ['colspan', 'rowspan'].includes(name)) return /^\d+$/.test(attribute.value);
        if ((tag === 'TD' || tag === 'TH') && name === 'style') {
            return /^\s*text-align\s*:\s*(?:left|right|center)\s*;?\s*$/i.test(attribute.value);
        }
        return false;
    }
    function safeFragment(html) {
        var template = document.createElement('template');
        template.innerHTML = html;
        for (var element of template.content.querySelectorAll('*')) {
            if (element.namespaceURI !== 'http://www.w3.org/1999/xhtml' || discardedElements.has(element.tagName)) {
                element.remove();
                continue;
            }
            if (!allowedElements.has(element.tagName)) {
                element.replaceWith(...element.childNodes);
                continue;
            }
            for (var attribute of Array.from(element.attributes)) {
                if (!safeAttribute(element, attribute)) element.removeAttribute(attribute.name);
            }
            if (element.tagName === 'INPUT') {
                if (element.getAttribute('type') !== 'checkbox') element.remove();
                else element.disabled = true;
            }
        }
        return template.content;
    }
    window.__SGT_SAFE_FRAGMENT__ = safeFragment;
    function cloneAuthored(node) {
        var clone = node.cloneNode(true);
        if (clone.nodeType === Node.ELEMENT_NODE) {
            authoredStyles.set(clone, clone.getAttribute('style'));
            for (var element of clone.querySelectorAll('*')) {
                authoredStyles.set(element, element.getAttribute('style'));
            }
        }
        return clone;
    }
    function syncAttributes(current, fresh) {
        var style = fresh.getAttribute('style');
        // Unchanged source styles must not erase the fitter/reveal runtime's state.
        var preserveStyle = authoredStyles.has(current) && authoredStyles.get(current) === style
            && current.getAttribute('class') === fresh.getAttribute('class');
        for (var index = current.attributes.length - 1; index >= 0; index--) {
            var name = current.attributes[index].name;
            if (name === 'style' && preserveStyle) continue;
            if (!fresh.hasAttribute(name)) current.removeAttribute(name);
        }
        for (var next = 0; next < fresh.attributes.length; next++) {
            var attribute = fresh.attributes[next];
            if (attribute.name === 'style' && preserveStyle) continue;
            if (current.getAttribute(attribute.name) !== attribute.value) {
                current.setAttribute(attribute.name, attribute.value);
            }
        }
        authoredStyles.set(current, style);
    }

    function compatible(current, fresh) {
        return current.nodeType === fresh.nodeType
            && (current.nodeType !== Node.ELEMENT_NODE || current.nodeName === fresh.nodeName);
    }

    function syncNode(current, fresh) {
        if (!compatible(current, fresh)) {
            current.replaceWith(cloneAuthored(fresh));
            return;
        }
        if (current.nodeType === Node.TEXT_NODE) {
            if (current.nodeValue !== fresh.nodeValue) current.nodeValue = fresh.nodeValue;
            return;
        }
        if (current.nodeType !== Node.ELEMENT_NODE) return;
        syncAttributes(current, fresh);
        syncChildren(current, fresh);
    }

    function syncChildren(current, fresh) {
        var index = 0;
        while (index < fresh.childNodes.length) {
            var next = fresh.childNodes[index];
            var existing = current.childNodes[index];
            if (!existing) current.appendChild(cloneAuthored(next));
            else syncNode(existing, next);
            index++;
        }
        while (current.childNodes.length > fresh.childNodes.length) {
            current.removeChild(current.lastChild);
        }
    }

    window.__SGT_PATCH_BODY__ = function(body, html) {
        syncChildren(body, safeFragment(html));
    };
})();
