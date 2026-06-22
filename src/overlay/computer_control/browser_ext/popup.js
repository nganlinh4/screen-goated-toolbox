// Pairing UI: store the shared secret + port, then poke the service worker to
// (re)connect. The secret never leaves this machine; the SW uses it for
// challenge-response on every connection.

const $ = (id) => document.getElementById(id);

async function refresh() {
  const { secret, port } = await chrome.storage.local.get(["secret", "port"]);
  $("port").value = port || 47800;
  const el = $("status");
  if (secret) {
    el.textContent = "Paired ✓ (the SGT app must be running to connect)";
    el.className = "ok";
  } else {
    el.textContent = "Not paired";
    el.className = "off";
  }
}

$("save").addEventListener("click", async () => {
  const secret = $("secret").value.trim();
  const port = parseInt($("port").value, 10) || 47800;
  if (!secret) {
    $("status").textContent = "Enter the pairing code first";
    return;
  }
  await chrome.storage.local.set({ secret, port });
  $("secret").value = "";
  chrome.runtime.sendMessage("reconnect");
  refresh();
});

$("forget").addEventListener("click", async () => {
  await chrome.storage.local.remove("secret");
  refresh();
});

refresh();
