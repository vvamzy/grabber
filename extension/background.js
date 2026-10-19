// Download Manager — background service worker.
//
// IDM's browser integration does two things:
//   1. Intercepts downloads the browser would handle and hands them to the
//      native downloader instead.
//   2. Sniffs video/media so you can grab a stream URL.
// This worker does both, forwarding every job to dm-host.exe via native
// messaging, which POSTs to the local service.

const HOST_NAME = "com.downloadmanager.host";

// ---------- native messaging ----------

let nativePort = null;

function connectNative() {
  if (nativePort) return nativePort;
  try {
    nativePort = chrome.runtime.connectNative(HOST_NAME);
    nativePort.onMessage.addListener((resp) => {
      if (resp && resp.ok) {
        notify("Download Manager", `Grabbed: ${resp.filename || resp.id || "download"}`);
      } else if (resp && resp.error) {
        notify("Download Manager error", resp.error);
      }
    });
    nativePort.onDisconnect.addListener(() => {
      nativePort = null; // reconnect lazily on next send
      const err = chrome.runtime.lastError;
      if (err) console.warn("native host disconnected:", err.message);
    });
    return nativePort;
  } catch (e) {
    console.warn("connectNative failed:", e);
    return null;
  }
}

function sendToHost(payload) {
  const port = connectNative();
  if (!port) {
    notify("Download Manager", "Native host not reachable — is dm-host installed?");
    return false;
  }
  port.postMessage(payload);
  return true;
}

// ---------- download interception ----------

// URLs we forwarded ourselves; skip re-intercepting them.
const selfHandled = new Set();

chrome.downloads.onCreated.addListener((item) => {
  if (!item || !item.url) return;
  if (selfHandled.has(item.id)) return;
  // Only take over http(s) downloads.
  if (!/^https?:\/\//i.test(item.url)) return;

  // Stop the browser's own download, then hand the URL to the service.
  chrome.downloads.cancel(item.id, () => {
    chrome.downloads.erase({ id: item.id }, () => {});
  });

  sendToHost({ url: item.url, filename: item.filename || undefined });
});

// ---------- context menu: "Download with Download Manager" ----------

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "dm-download",
    title: "Download with Download Manager",
    contexts: ["link", "image", "video", "audio", "page"],
  });
  chrome.contextMenus.create({
    id: "dm-grab-video",
    title: "Grab media URL with Download Manager",
    contexts: ["video", "audio", "link", "page"],
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "dm-download") {
    const url = info.linkUrl || info.srcUrl || info.pageUrl;
    if (url) sendToHost({ url, filename: guessFilename(url) });
  } else if (info.menuItemId === "dm-grab-video") {
    const url = info.srcUrl || info.linkUrl || info.pageUrl;
    if (url) sendToHost({ url, filename: guessFilename(url) });
  }
});

// ---------- popup / manual trigger ----------

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg && msg.type === "add-url" && msg.url) {
    const ok = sendToHost({ url: msg.url, filename: msg.filename });
    sendResponse({ ok });
    return true;
  }
  if (msg && msg.type === "ping") {
    sendResponse({ ok: true });
    return true;
  }
});

// ---------- helpers ----------

function guessFilename(url) {
  try {
    const p = new URL(url).pathname;
    const last = p.split("/").filter(Boolean).pop();
    return last || undefined;
  } catch {
    return undefined;
  }
}

function notify(title, message) {
  try {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icon.png",
      title,
      message,
    });
  } catch {
    // notifications are best-effort
  }
}
