(function() {
    const copyButton = document.getElementById("copyButton");
    const closeButton = document.getElementById("closeButton");

    function post(payload) {
        if (window.sgtAndroid && typeof window.sgtAndroid.postMessage === "function") {
            window.sgtAndroid.postMessage(JSON.stringify(payload));
        }
    }

    window.applyCanvasBootstrap = function(raw) {
        const data = typeof raw === "string" ? JSON.parse(raw) : raw;
        copyButton.textContent = data.copyLabel;
        closeButton.textContent = data.closeLabel;
    };

    copyButton.addEventListener("click", () => post({ type: "copyResult" }));
    closeButton.addEventListener("click", () => post({ type: "closeResult" }));
})();
