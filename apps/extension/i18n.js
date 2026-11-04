/**
 * ZDM extension localisation.
 * Developed by Ramy Badraan
 *
 * Why not just `chrome.i18n`? Because it follows the *browser* UI language. A
 * user running ZDM in Arabic with an English browser saw an English button —
 * that is the bug this file fixes.
 *
 * Priority:
 *   1. the language the desktop app reports over the local bridge,
 *   2. the browser UI language,
 *   3. English.
 */

/** Strings the floating button and toasts need. */
const ZDM_STRINGS = {
  ar: {
    download: 'تحميل باستخدام ZDM',
    downloading: 'جارٍ الإرسال…',
    offline: 'تطبيق ZDM غير مفتوح — افتحه ثم أعد المحاولة',
    failed: 'تعذّر بدء التنزيل',
    dismiss: 'إخفاء لهذا الفيديو',
  },
  en: {
    download: 'Download with ZDM',
    downloading: 'Sending…',
    offline: 'ZDM is not running — start it and try again',
    failed: 'Could not start the download',
    dismiss: 'Hide for this video',
  },
  fr: {
    download: 'Télécharger avec ZDM',
    downloading: 'Envoi…',
    offline: "ZDM n'est pas démarré — lancez-le puis réessayez",
    failed: 'Impossible de démarrer le téléchargement',
    dismiss: 'Masquer pour cette vidéo',
  },
};

/** Language reported by the desktop app; null until the bridge answers. */
let appLanguage = null;

function browserLanguage() {
  try {
    const ui = chrome.i18n.getUILanguage() || 'en';
    const base = ui.slice(0, 2).toLowerCase();
    if (ZDM_STRINGS[base]) return base;
  } catch {
    /* ignore */
  }
  return 'en';
}

/** The language the UI should use right now. */
function activeLanguage() {
  if (appLanguage && ZDM_STRINGS[appLanguage]) return appLanguage;
  return browserLanguage();
}

/** Replace `{n}`-style placeholders. */
function fill(template, values) {
  return template.replace(/\{(\w+)\}/g, (match, key) =>
    Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : match
  );
}

/**
 * Translate a key using the app language when known.
 * Falls back to the English table, then to the key itself.
 */
function t(key, values = {}) {
  const lang = activeLanguage();
  const table = ZDM_STRINGS[lang] || ZDM_STRINGS.en;
  const text = table[key] ?? ZDM_STRINGS.en[key] ?? key;
  return fill(text, values);
}

/** Called when the bridge reports the app's language. */
function setAppLanguage(code) {
  if (code && ZDM_STRINGS[code]) {
    appLanguage = code;
  }
}

/** Text direction for the given language. */
function activeDirection() {
  return activeLanguage() === 'ar' ? 'rtl' : 'ltr';
}
