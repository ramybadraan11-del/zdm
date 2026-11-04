/**
 * ZDM Extension Popup
 * Developed by Ramy Badraan
 *
 * Localised through `chrome.i18n` so the popup speaks the same language as the
 * desktop app without duplicating strings.
 */

const t = (key, fallback) => {
  try {
    return chrome.i18n.getMessage(key) || fallback;
  } catch {
    return fallback;
  }
};

document.addEventListener('DOMContentLoaded', async () => {
  const titleEl = document.getElementById('title');
  const statusText = document.getElementById('statusText');
  const dot = document.getElementById('dot');
  const toggle = document.getElementById('captureToggle');
  const captureLabel = document.getElementById('captureLabel');
  const captureHint = document.getElementById('captureHint');
  const openBtn = document.getElementById('openBtn');
  const help = document.getElementById('help');

  titleEl.textContent = t('popup_title', 'Zyzoom Download Manager');
  captureLabel.textContent = t('popup_capture', 'Take over browser downloads');
  captureHint.textContent = t('popup_capture_hint', '');
  openBtn.textContent = t('popup_open', 'Open ZDM');
  help.textContent = t('popup_help', '');

  // Localise the direction from the UI language so Arabic reads naturally.
  try {
    const uiLang = chrome.i18n.getUILanguage() || 'en';
    const rtl = /^(ar|he|fa|ur)/i.test(uiLang);
    document.documentElement.lang = uiLang;
    document.documentElement.dir = rtl ? 'rtl' : 'ltr';
  } catch {
    /* keep defaults */
  }

  const stored = await chrome.storage.local.get({ captureDownloads: true });
  toggle.checked = stored.captureDownloads;

  toggle.addEventListener('change', async () => {
    await chrome.storage.local.set({ captureDownloads: toggle.checked });
    chrome.runtime.sendMessage({ action: 'set_capture', value: toggle.checked });
  });

  const setStatus = (online) => {
    dot.classList.toggle('on', online);
    statusText.textContent = online
      ? t('popup_status_online', 'Connected to ZDM')
      : t('popup_status_offline', 'ZDM is closed');
  };

  try {
    const res = await fetch('http://127.0.0.1:46800/', { cache: 'no-store' });
    setStatus(res.ok);
  } catch {
    setStatus(false);
  }

  openBtn.addEventListener('click', async () => {
    try {
      await fetch('http://127.0.0.1:46800/', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ type: 'focus_window' }),
      });
      setStatus(true);
    } catch {
      setStatus(false);
    }
  });
});
