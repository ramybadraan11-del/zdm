//! ZDM Extension Background Service Worker
//! Developed by Ramy Badraan

const NATIVE_HOST = "com.zyzoom.zdm";
const LOCAL_BRIDGE = "http://127.0.0.1:46800";

// Setup Context Menus
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "zdm-download-link",
    title: "تحميل باستخدام ZDM (32-Stream)",
    contexts: ["link", "video", "audio", "image"],
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  const targetUrl = info.linkUrl || info.srcUrl || info.pageUrl;
  if (targetUrl) {
    sendToZdm({
      type: "add_download",
      url: targetUrl,
      referer: tab?.url || "",
    });
  }
});

// Intercept browser downloads
chrome.downloads.onCreated.addListener((downloadItem) => {
  // Ignore downloads initiated by ZDM itself or local schemes
  if (!downloadItem.url || downloadItem.url.startsWith("blob:") || downloadItem.url.startsWith("data:")) {
    return;
  }

  // Check if extension capture is enabled
  chrome.storage.local.get({ captureDownloads: true }, (items) => {
    if (items.captureDownloads) {
      // Cancel browser download
      chrome.downloads.cancel(downloadItem.id, () => {
        chrome.downloads.erase({ id: downloadItem.id });
      });

      sendToZdm({
        type: "add_download",
        url: downloadItem.url,
        filename: downloadItem.filename ? downloadItem.filename.split(/[\\/]/).pop() : undefined,
        referer: downloadItem.referrer || "",
      });
    }
  });
});

// Communication with ZDM Core
async function sendToZdm(message) {
  // Method 1: Try Native Messaging Host
  try {
    chrome.runtime.sendNativeMessage(NATIVE_HOST, message, (response) => {
      if (chrome.runtime.lastError) {
        // Fallback to local HTTP bridge
        sendViaHttp(message);
      } else {
        console.log("[ZDM Extension] Native Messaging Response:", response);
      }
    });
  } catch (err) {
    sendViaHttp(message);
  }
}

async function sendViaHttp(message) {
  try {
    await fetch(LOCAL_BRIDGE, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(message),
      mode: "no-cors",
    });
    console.log("[ZDM Extension] Forwarded via HTTP bridge");
  } catch (e) {
    console.warn("[ZDM Extension] ZDM Desktop is offline.");
  }
}

// Receive messages from content script
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.action === "download_media") {
    sendToZdm({
      type: "add_download",
      url: msg.url,
      filename: msg.filename,
      referer: sender.tab?.url || "",
    });
    sendResponse({ status: "queued" });
  }
});
