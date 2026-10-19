'use strict';

/**
 * Zyzoom Capture — content script.
 * يلتقط عناصر video/audio/source وروابط الوسائط داخل الصفحة ويرسلها للخدمة الخلفية.
 */

(function () {
  const MEDIA_EXTENSIONS = [
    '.mp4', '.m4v', '.m4a', '.mkv', '.webm', '.mov', '.avi', '.flv', '.ts', '.m3u8',
    '.mp3', '.aac', '.ogg', '.oga', '.wav', '.flac', '.opus', '.wma', '.3gp',
  ];

  function absolute(url) {
    try {
      return new URL(url, document.baseURI).href;
    } catch (err) {
      return null;
    }
  }

  function looksLikeMedia(url) {
    if (!url) return false;
    let pathname = url;
    try {
      pathname = new URL(url, document.baseURI).pathname.toLowerCase();
    } catch (err) {
      return false;
    }
    return MEDIA_EXTENSIONS.some((ext) => pathname.endsWith(ext));
  }

  function collect() {
    const items = [];
    const push = (url, kind, mime) => {
      const resolved = absolute(url);
      if (!resolved || !/^https?:/i.test(resolved)) return;
      items.push({
        url: resolved,
        kind,
        mime: mime || '',
        title: document.title || '',
        pageUrl: location.href,
      });
    };

    document.querySelectorAll('video').forEach((node) => {
      if (node.currentSrc) push(node.currentSrc, 'video', 'video/mp4');
      if (node.src) push(node.src, 'video', '');
    });

    document.querySelectorAll('audio').forEach((node) => {
      if (node.currentSrc) push(node.currentSrc, 'audio', 'audio/mpeg');
      if (node.src) push(node.src, 'audio', '');
    });

    document.querySelectorAll('source').forEach((node) => {
      const type = node.getAttribute('type') || '';
      if (node.src) push(node.src, type.startsWith('audio') ? 'audio' : 'video', type);
    });

    document.querySelectorAll('a[href]').forEach((node) => {
      if (looksLikeMedia(node.getAttribute('href'))) push(node.getAttribute('href'), 'link', '');
    });

    // ملفات HLS الشائعة داخل كائنات الصفحة
    try {
      if (window.performance && typeof window.performance.getEntriesByType === 'function') {
        window.performance
          .getEntriesByType('resource')
          .map((entry) => entry.name)
          .filter((name) => looksLikeMedia(name) || /\.m3u8(\?|$)/i.test(name))
          .forEach((name) => push(name, 'resource', ''));
      }
    } catch (err) {
      /* تجاهل */
    }

    return items;
  }

  function report() {
    const items = collect();
    if (!items.length) return;
    try {
      chrome.runtime.sendMessage({ type: 'zyzoom-media', items, title: document.title });
    } catch (err) {
      /* الخدمة الخلفية قد لا تكون جاهزة بعد */
    }
  }

  report();

  let timer = null;
  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(report, 1200);
  };

  try {
    const observer = new MutationObserver(schedule);
    observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true });
    window.addEventListener('popstate', schedule);
    window.addEventListener('hashchange', schedule);
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) schedule();
    });
  } catch (err) {
    /* تجاهل */
  }
})();
