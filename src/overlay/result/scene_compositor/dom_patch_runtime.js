(function() {
    function syncAttributes(current, fresh) {
        for (var index = current.attributes.length - 1; index >= 0; index--) {
            var name = current.attributes[index].name;
            if (!fresh.hasAttribute(name)) current.removeAttribute(name);
        }
        for (var next = 0; next < fresh.attributes.length; next++) {
            var attribute = fresh.attributes[next];
            if (current.getAttribute(attribute.name) !== attribute.value) {
                current.setAttribute(attribute.name, attribute.value);
            }
        }
    }

    function compatible(current, fresh) {
        return current.nodeType === fresh.nodeType
            && (current.nodeType !== Node.ELEMENT_NODE || current.nodeName === fresh.nodeName);
    }

    function syncNode(current, fresh) {
        if (!compatible(current, fresh)) {
            current.replaceWith(fresh.cloneNode(true));
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
            if (!existing) current.appendChild(next.cloneNode(true));
            else syncNode(existing, next);
            index++;
        }
        while (current.childNodes.length > fresh.childNodes.length) {
            current.removeChild(current.lastChild);
        }
    }

    window.__SGT_PATCH_BODY__ = function(body, html) {
        var template = document.createElement('template');
        template.innerHTML = html;
        syncChildren(body, template.content);
    };
})();
