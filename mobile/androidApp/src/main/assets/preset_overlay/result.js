(function() {
    const resultTitle = document.getElementById("resultTitle");
    const resultStatus = document.getElementById("resultStatus");
    const resultBody = document.getElementById("resultBody");
    const dragRegion = document.getElementById("dragRegion");

    window.ipc = {
        postMessage: function(message) {
            if (window.sgtAndroid && typeof window.sgtAndroid.postMessage === "function") {
                window.sgtAndroid.postMessage(message);
            }
        }
    };

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
            post({ type: "dragResultWindow", dx: dx, dy: dy });
        });
        target.addEventListener("pointerup", (event) => {
            dragging = false;
            target.releasePointerCapture(event.pointerId);
        });
        target.addEventListener("pointercancel", () => {
            dragging = false;
        });
    }

    window.applyResultBootstrap = function(raw) {
        const data = typeof raw === "string" ? JSON.parse(raw) : raw;
        resultTitle.textContent = data.title;
        resultStatus.textContent = data.status;
    };

    window.updateResultState = function(raw) {
        const data = typeof raw === "string" ? JSON.parse(raw) : raw;
        resultTitle.textContent = data.title;
        resultStatus.textContent = data.status;
        resultBody.innerHTML = data.html || "";
        if (window.runWindowsMarkdownFit) {
            window.runWindowsMarkdownFit(!!data.streaming, data.streaming ? "mobile_streaming_fit" : "mobile_final_fit");
        }
    };

    wireDrag(dragRegion);
})();
