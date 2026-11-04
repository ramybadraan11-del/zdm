/**
 * ZDM Background Service Worker (MV3, Chromium + Firefox compatible).
 * Developed by Ramy Badraan
 *
 * Responsibilities:
 *  - forward captures to the desktop app over the local bridge,
 *  - keep native messaging as a fallback only,
 *  - *not* cancel a browser download until the desktop app has confirmed it, so
 *    a stopped app can never swallow a file (the old behaviour),
 *  - rebuild context menus for the current UI language.
 */

importScripts('lib/ext-of.js');

const BRIDGE = 'http://127.0.0.1:46800';
const NATIVE_HOST = 'com.zyzoom.zdm';

const msg = (key, fallback) => {
  try {
    return chrome.i18n.getMessage(key) || fallback;
  } catch {
    return fallback;
  }
};

chrome.runtime.onInstalled.addListener(() => {
  rebuildMenus();
  chrome.storage.local.get({ captureDownloads: true, notifications: true }).then((cfg) => {
    chrome.storage.local.set(cfg);
  });
});

chrome.runtime.onStartup.addListener(rebuildMenus);

function rebuildMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: 'zdm-download-link',
      title: msg('menu_download_link', 'Download with ZDM'),
      contexts: ['link', 'video', 'audio', 'image'],
    });
    chrome.contextMenus.create({
      id: 'zdm-download-video',
      title: msg('menu_download_video', 'Download this video with ZDM'),
      contexts: ['video', 'audio'],
    });
    chrome.contextMenus.create({
      id: 'zdm-open-app',
      title: msg('menu_open_app', 'Open ZDM'),
      contexts: ['all'],
    });
  });
}

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === 'zdm-open-app') {
    deliver({ type: 'focus_window' });
    return;
  }

  const url =
    info.menuItemId === 'zdm-download-video'
      ? tab?.url || info.pageUrl
      : info.linkUrl || info.srcUrl || info.pageUrl;

  if (!url) return;

  deliver({
    type: 'add_download',
    url,
    referer: tab?.url || '',
    filename: guessFilename(url),
  });
});

/**
 * Capture preferences live in the desktop app; cache them locally so the
 * extension still behaves sensibly while the app is closed.
 */
let remotePrefs = {
  capture_downloads: true,
  sniffer_enabled: true,
  intercept_extensions: null, // null = not loaded yet, [] = capture nothing
  language: null,
  reachable: false,
};

async function refreshPrefs() {
  try {
    const res = await fetch(BRIDGE, { cache: 'no-store' });
    if (!res.ok) throw new Error('offline');
    const data = await res.json();
    remotePrefs = {
      capture_downloads: data.capture_downloads !== false,
      sniffer_enabled: data.sniffer_enabled !== false,
      // null => the app is unreachable or too old to send the list; we only
      // auto-capture when we know which types the user wants.
      intercept_extensions: Array.isArray(data.intercept_extensions) ? data.intercept_extensions : null,
      // The floating button mirrors the desktop language, not the browser's.
      language: typeof data.language === 'string' ? data.language : null,
      reachable: true,
    };
    chrome.storage.local.set({ remotePrefs });
  } catch {
    remotePrefs.reachable = false;
  }
  return remotePrefs;
}

chrome.runtime.onInstalled.addListener(() => { refreshPrefs(); });
chrome.runtime.onStartup.addListener(() => { refreshPrefs(); });

/** Catching browser downloads is opt-in and always confirmed first. */
chrome.downloads.onCreated.addListener(async (item) => {
  if (!item.url || /^(blob|data|about):/i.test(item.url)) return;
  if (item.byExtensionId) return;

  const cfg = await chrome.storage.local.get({ captureDownloads: true });
  if (!cfg.captureDownloads) return;

  // The desktop setting wins when the app is reachable.
  const prefs = await refreshPrefs();
  if (prefs.reachable && !prefs.capture_downloads) return;

  // Only take over content the download manager can actually fetch.
  if (!/^https?:/i.test(item.url)) return;

  // Leave anything outside the configured type list to the browser — images in
  // particular are the browser's job.
  const ext = extOf(item.filename || item.url);
  if (remotePrefs.reachable && Array.isArray(remotePrefs.intercept_extensions)) {
    if (ext && !remotePrefs.intercept_extensions.includes(ext)) return;
    if (!ext && remotePrefs.intercept_extensions.length === 0) return;
  }

  const ok = await deliver({
    type: 'add_download',
    url: item.url,
    filename: guessFilename(item.filename || item.url),
    referer: item.referrer || '',
  });

  if (ok) {
    // Confirmed by the app: remove the browser's own copy so the user does not
    // end up with two downloads.
    chrome.downloads.cancel(item.id, () => {
      chrome.downloads.erase({ id: item.id });
    });
  }
  // If not confirmed, the browser download simply continues. Never lose a file.
});

/** POST to the local bridge; resolves true only on a real acknowledgement. */
async function deliver(payload) {
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 2500);
    const res = await fetch(BRIDGE, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal: controller.signal,
    });
    clearTimeout(timer);
    if (res.ok) {
      const data = await res.json().catch(() => ({}));
      if (data.status !== 'error') return true;
      notifyError(data.error);
      // Surface the app's own reason instead of a generic failure.
      return { ok: false, error: data.error || '' };
    }
  } catch {
    /* fall through to native messaging */
  }

  return new Promise((resolve) => {
    try {
      chrome.runtime.sendNativeMessage(NATIVE_HOST, payload, (response) => {
        if (chrome.runtime.lastError || !response) {
          resolve(false);
          return;
        }
        resolve(response.status === 'queued' || response.status === 'modal_opened');
      });
    } catch {
      resolve(false);
    }
  });
}

function notifyError(detail) {
  chrome.storage.local.get({ notifications: true }).then((cfg) => {
    if (!cfg.notifications || !chrome.notifications) return;
    try {
      chrome.notifications.create({
        type: 'basic',
        iconUrl: chrome.runtime.getURL('icons/icon128.png'),
        title: msg('notify_error_title', 'ZDM'),
        message: String(detail || msg('notify_error_body', 'Could not start the download')),
      });
    } catch {
      /* notifications are optional */
    }
  });
}

function guessFilename(url) {
  try {
    const parsed = new URL(url);
    const last = decodeURIComponent(parsed.pathname.split('/').filter(Boolean).pop() || '');
    if (last && last.includes('.')) return last;
  } catch {
    /* ignore */
  }
  return undefined;
}

/** Messages from the content script. */
chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
  if (request?.action === 'native_forward') {
    // deliver() answers {ok:true} | {ok:false,error} | false (app not running).
    deliver(request.payload).then((result) => {
      if (result === true) return sendResponse({ ok: true });
      if (result && typeof result === 'object') {
        return sendResponse({ ok: false, error: result.error || '' });
      }
      return sendResponse({ ok: false });
    });
    return true; // keep the channel open for the async answer
  }

  if (request?.action === 'get_prefs') {
    refreshPrefs().then((prefs) => sendResponse(prefs));
    return true;
  }

  if (request?.action === 'bridge_status') {
    fetch(BRIDGE)
      .then((r) => r.json())
      .then((data) => sendResponse({ online: true, data }))
      .catch(() => sendResponse({ online: false }));
    return true;
  }

  if (request?.action === 'set_capture') {
    chrome.storage.local.set({ captureDownloads: !!request.value }).then(() => {
      sendResponse({ ok: true });
    });
    return true;
  }

  return false;
});
