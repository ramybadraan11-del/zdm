'use strict';

/**
 * Zyzoom Capture — service worker.
 *
 * يجمع روابط الوسائط من نوعين:
 *  1) طلبات الشبكة المرصودة (webRequest) — يكشف ملفات mp4/m3u8/mkv/webm...
 *  2) عناصر الوسائط داخل الصفحة (video/audio/source) يرسلها content.js
 *
 * ثم يرسلها إلى مضيّف Native Messaging الذي يمرّرها لتطبيق Zyzoom.
 */

const NATIVE_HOST = 'com.zyzoom.download_manager';

const MEDIA_EXTENSIONS = [
  '.mp4', '.m4v', '.m4a', '.mkv', '.webm', '.mov', '.avi', '.flv', '.ts', '.m3u8',
  '.mp3', '.aac', '.ogg', '.oga', '.wav', '.flac', '.opus', '.wma', '.3gp',
];

const seen = new Set();
let nativePort = null;
let pending = [];
let flushTimer = null;

function isMediaUrl(url) {
  if (!url || !/^https?:/i.test(url)) return false;
  let pathname = url;
  try {
    pathname = new URL(url).pathname.toLowerCase();
  } catch (err) {
    return false;
  }
  return MEDIA_EXTENSIONS.some((ext) => pathname.endsWith(ext));
}

function nativeConnect() {
  if (nativePort) return nativePort;
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST);
  } catch (err) {
    nativePort = null;
    return null;
  }
  nativePort.onDisconnect.addListener(() => {
    nativePort = null;
  });
  nativePort.onMessage.addListener((response) => {
    if (response && response.ok === false && response.error === 'zyzoom-app-not-running') {
      chrome.storage.local.set({ lastError: ' Zyzoom Download Manager لا يعمل' });
    }
  });
  return nativePort;
}

function sendToApp(items) {
  if (!items.length) return;
  const port = nativeConnect();
  if (!port) {
    chrome.storage.local.set({ lastError: 'تعذّر الاتصال بمضيف Native Messaging' });
    return;
  }
  try {
    port.postMessage({ type: 'capture', source: 'zyzoom-capture', items });
    chrome.storage.local.set({ lastError: '', lastCaptureAt: Date.now() });
  } catch (err) {
    nativePort = null;
  }
}

function queue(item) {
  if (!item || !item.url) return;
  const key = item.url;
  if (seen.has(key)) return;
  seen.add(key);
  pending.push(item);
  if (pending.length > 200) pending = pending.slice(-200);

  if (flushTimer) clearTimeout(flushTimer);
  flushTimer = setTimeout(() => {
    flushTimer = null;
    const batch = pending;
    pending = [];
    sendToApp(batch);
  }, 400);
}

/* ---------------------------------------------- رصد طلبات الشبكة */

chrome.webRequest.onBeforeRequest.addListener(
  (details) => {
    if (details.tabId === -1) return;
    if (!isMediaUrl(details.url)) return;
    queue({
      url: details.url,
      kind: 'network',
      mime: details.type,
      pageUrl: details.documentUrl || details.originUrl || '',
      title: details.documentUrl || '',
      detectedAt: Date.now(),
    });
  },
  { urls: ['<all_urls>'] },
);

/* ---------------------------------------------- رسائل صفحة المحتوى */

chrome.runtime.onMessage.addListener((message, sender) => {
  if (!message || message.type !== 'zyzoom-media') return;
  const pageUrl = sender.tab ? sender.tab.url : message.pageUrl;
  const items = Array.isArray(message.items) ? message.items : [];
  for (const item of items) {
    queue({
      url: item.url,
      kind: item.kind || 'element',
      mime: item.mime || '',
      title: item.title || message.title || '',
      pageUrl: item.pageUrl || pageUrl || '',
      detectedAt: Date.now(),
    });
  }
});

/* ---------------------------------------------- أدوات للمتصفح */

chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.local.set({ installedAt: Date.now() });
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message) return false;
  if (message.type === 'zyzoom-ping') {
    const port = nativeConnect();
    if (!port) {
      sendResponse({ ok: false, error: 'native-host-unavailable' });
      return true;
    }
    try {
      port.postMessage({ type: 'ping' });
    } catch (err) {
      /* تجاهل */
    }
    sendResponse({ ok: true });
    return true;
  }
  if (message.type === 'zyzoom-clear') {
    seen.clear();
    pending = [];
    sendResponse({ ok: true });
    return true;
  }
  return false;
});
