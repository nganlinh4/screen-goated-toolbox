setInterval(function() { window.ipc.postMessage('renderer_heartbeat'); }, 1000);
var rendererFontStarted = performance.now();
document.fonts.load("400 16px 'Google Sans Flex'").then(function(faces) {
    if (!faces.length || !document.fonts.check("400 16px 'Google Sans Flex'")) {
        throw new Error('Google Sans Flex did not enter the loaded font set');
    }
    document.documentElement.classList.add('sgt-font-ready');
    window.ipc.postMessage(JSON.stringify({
        type: 'font_ready',
        duration_ms: performance.now() - rendererFontStarted
    }));
    window.ipc.postMessage('renderer_ready');
}).catch(function(error) {
    window.ipc.postMessage(JSON.stringify({
        type: 'command_error',
        command: 'font_bootstrap',
        id: null,
        error: String(error && error.message ? error.message : error)
    }));
});
