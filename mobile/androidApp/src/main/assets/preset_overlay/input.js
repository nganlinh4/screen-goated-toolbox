(function() {
    const title = document.getElementById("title");
    const footerHint = document.getElementById("footerHint");
    const closeButton = document.getElementById("closeButton");
    const submitButton = document.getElementById("submitButton");
    const editor = document.getElementById("editor");
    const dragRegion = document.getElementById("dragRegion");

    function post(payload) {
        if (window.sgtAndroid && typeof window.sgtAndroid.postMessage === "function") {
            window.sgtAndroid.postMessage(JSON.stringify(payload));
        }
    }

    function wireDrag(target) {
        let dragging = false;
        let lastX = 0;
        let lastY = 0;
        target.addEventListener("pointerdown", (event) => {
            dragging = true;
            lastX = event.clientX;
            lastY = event.clientY;
            target.setPointerCapture(event.pointerId);
        });
        target.addEventListener("pointermove", (event) => {
            if (!dragging) return;
            const dx = event.clientX - lastX;
            const dy = event.clientY - lastY;
            lastX = event.clientX;
            lastY = event.clientY;
            post({ type: "dragInputWindow", dx: dx, dy: dy });
        });
        target.addEventListener("pointerup", (event) => {
            dragging = false;
            target.releasePointerCapture(event.pointerId);
        });
        target.addEventListener("pointercancel", () => {
            dragging = false;
        });
    }

    function submit() {
        const value = editor.value.trim();
        if (!value) return;
        post({ type: "submitInput", text: value });
    }

    window.applyInputBootstrap = function(raw) {
        const data = typeof raw === "string" ? JSON.parse(raw) : raw;
        title.textContent = data.title;
        footerHint.textContent = data.footerHint;
        submitButton.textContent = data.submitLabel;
        editor.placeholder = data.placeholder;
        requestAnimationFrame(() => editor.focus());
    };

    window.clearInput = function() {
        editor.value = "";
        requestAnimationFrame(() => editor.focus());
    };

    closeButton.addEventListener("click", () => post({ type: "closeInputWindow" }));
    submitButton.addEventListener("click", submit);
    editor.addEventListener("keydown", (event) => {
        if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            submit();
        }
        if (event.key === "Escape") {
            event.preventDefault();
            post({ type: "closeInputWindow" });
        }
    });

    wireDrag(dragRegion);
})();
