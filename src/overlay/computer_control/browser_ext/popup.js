// Pairing UI: store the shared secret + port, then poke the service worker to
// (re)connect. The secret never leaves this machine; the SW uses it for
// challenge-response on every connection.

const $ = (id) => document.getElementById(id);
let resumeTabId = null;

async function refresh() {
  const { secretV8, port, debuggerRevocationsV1 } = await chrome.storage.local.get(["secretV8", "port", "debuggerRevocationsV1"]);
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  const revoked = Array.isArray(debuggerRevocationsV1?.tabs) && debuggerRevocationsV1.tabs.includes(tab?.id);
  $("resume").disabled = !revoked;
  resumeTabId = revoked ? tab.id : null;
  $("port").value = port || 47800;
  const el = $("status");
  if (revoked) {
    el.textContent = "Control paused for this tab";
    el.className = "off";
  } else if (secretV8) {
    el.textContent = "Paired ✓ (the SGT app must be running to connect)";
    el.className = "ok";
  } else {
    el.textContent = "Not paired";
    el.className = "off";
  }
}

$("save").addEventListener("click", async () => {
  const secret = $("secret").value.trim();
  const port = Number($("port").value || 47800);
  if (!secret) {
    $("status").textContent = "Enter the pairing code first";
    return;
  }
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    $("status").textContent = "Port must be a whole number from 1 to 65535";
    return;
  }
  await chrome.storage.local.set({ secretV8: secret, port });
  $("secret").value = "";
  chrome.runtime.sendMessage("reconnect");
  refresh();
});

$("forget").addEventListener("click", async () => {
  await chrome.storage.local.remove(["secret", "secretV8"]);
  chrome.runtime.sendMessage("reconnect");
  refresh();
});

$("resume").addEventListener("click", async (event) => {
  if (!event.isTrusted || !navigator.userActivation.isActive) return;
  const expectedTab = resumeTabId;
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!expectedTab || tab?.id !== expectedTab) { await refresh(); return; }
  const result = await chrome.runtime.sendMessage({ type: "resume-debugging", tabId: tab.id });
  if (!result?.ok) { $("status").textContent = result?.error || "Could not resume control"; return; }
  await refresh();
});

refresh();
