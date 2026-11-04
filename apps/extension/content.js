/**
 * ZDM Content Script — floating "download this video" control.
 * Developed by Ramy Badraan
 *
 * Behaviour contract (each point fixes a reported bug):
 *
 * 1. The button appears only while a *visible* video is actually playing, and
 *    only over that video. It is not a page-wide overlay, so it cannot linger on
 *    unrelated pages or after the video element is gone.
 * 2. Clicking it hides it immediately for that playback (the old code left it
 *    pinned to the last known rectangle).
 * 3. When the video is paused, scrolled away, hidden, or replaced by a new
 *    video in the same tab, the button follows the DOM — a fresh `play` event on
 *    any video re-arms it, so no new tab is needed.
 * 4. State is per video element and per playback session, never a permanent
 *    `dataset` flag that survives navigation inside a SPA.
 */
(() => {
  if (window.__ZDM_CONTENT_LOADED__) return;
  window.__ZDM_CONTENT_LOADED__ = true;

  /** Active controller per <video>, so SPA swaps cannot leak old buttons. */
  const controllers = new WeakMap();
  /** The single button currently on screen. */
  let activeButton = null;
  /** The video the mounted button currently posts. */
  let activeVideo = null;
  /**
   * Playback identity the user dismissed the button for. Keyed by the media URL
   * (plus the page path) rather than the element: SPA players reuse a single
   * <video> across episodes, and an element-keyed set hid the button for good.
   */
  let dismissedFor = new Set();

  const BRIDGE = 'http://127.0.0.1:46800';

  /**
   * The user can turn the floating button off in ZDM's settings. We ask once at
   * startup and cache the answer; if the app is unreachable we stay enabled,
   * because a working sniffer is the more useful default.
   */
  let snifferEnabled = true;
  const MIN_W = 160;
  const MIN_H = 90;

  const log = (...args) => {
    if (window.__ZDM_DEBUG__) console.debug('[ZDM]', ...args);
  };

  /**
   * The label comes from the shared dictionary, which follows the *desktop app*
   * language reported over the bridge (falling back to the browser locale).
   */
  function strings() {
    return window.ZDM_I18N ? window.ZDM_I18N.t('download') : 'Download with ZDM';
  }

  function isUsableVideo(el) {
    if (!(el instanceof HTMLVideoElement)) return false;
    if (!el.isConnected) return false;
    const rect = el.getBoundingClientRect();
    if (rect.width < MIN_W || rect.height < MIN_H) return false;
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') {
      return false;
    }
    return true;
  }

  /** Best-effort direct media URL; only used when the page URL is not useful. */
  function directMediaUrl(el) {
    const src = el.currentSrc || el.src;
    if (src && /^https?:/i.test(src) && !src.startsWith('blob:')) return src;
    for (const source of el.querySelectorAll('source')) {
      if (source.src && /^https?:/i.test(source.src)) return source.src;
    }
    return null;
  }

  /**
   * Platforms whose playable URL is the page itself (yt-dlp understands them).
   * Sending a raw CDN/blob URL from these sites would fail, so we prefer the
   * page URL there — the same decision that later drives the video dialog.
   */
  const SITE_SCOPED = [
    'youtube.com', 'youtu.be', 'facebook.com', 'fb.watch', 'instagram.com',
    'tiktok.com', 'twitter.com', 'x.com', 'vimeo.com', 'dailymotion.com',
    'twitch.tv', 'reddit.com', 'rumble.com', 'bilibili.com', 'pinterest.com',
    'linkedin.com', 'threads.net', 'netflix.com', 'primevideo.com',
  ];

  function isSiteScoped() {
    const host = location.hostname.toLowerCase();
    return SITE_SCOPED.some((s) => host === s || host.endsWith('.' + s));
  }

  /**
   * Stable identity of the current playback.
   *
   * Element reuse (SPA episode swap) must yield a *different* key, so the media
   * URL alone is not enough for blob/MSE players that expose an empty source:
   * the source, the page path, and the video's duration are combined. Duration
   * changes when a new title loads in the same element, which is exactly the
   * signal an element-keyed set was missing.
   */
  function playbackKey(el) {
    const source = el.currentSrc || el.src || directMediaUrl(el) || '';
    const duration = Number.isFinite(el.duration) ? Math.round(el.duration) : 0;
    return `${location.pathname}::${source || 'inline'}::${duration}`;
  }

  function resolveTargetUrl(el) {
    if (isSiteScoped()) return normalizePlayerUrl(location.href);
    return directMediaUrl(el) || normalizePlayerUrl(location.href);
  }

  /**
   * Turn an embedded player URL into the canonical page URL that yt-dlp can
   * read. Running inside an iframe (all_frames) means `location.href` is the
   * embed, not the real video page.
   */
  function normalizePlayerUrl(raw) {
    try {
      const url = new URL(raw);
      const host = url.hostname.toLowerCase();
      if (host === 'player.vimeo.com') {
        const match = url.pathname.match(/\/video\/(\d+)/);
        if (match) return `https://vimeo.com/${match[1]}`;
      }
      if (host === 'youtube.com' || host === 'www.youtube.com' || host === 'youtube-nocookie.com') {
        const match = url.pathname.match(/\/embed\/([\w-]+)/);
        if (match) return `https://www.youtube.com/watch?v=${match[1]}`;
      }
      if (host === 'player.twitch.tv') {
        const channel = url.searchParams.get('channel');
        if (channel) return `https://www.twitch.tv/${channel}`;
        const video = url.searchParams.get('video');
        if (video) return `https://www.twitch.tv/videos/${video.replace(/^v/, '')}`;
      }
    } catch {
      /* keep the original */
    }
    return raw;
  }

  /**
   * Send the capture to the desktop app, preferring the HTTP bridge.
   * Resolves true only when the app confirmed it accepted the request: a 200
   * carrying {"status":"error"} is a failure (checking `res.ok` alone made the
   * button vanish silently when the app was closed).
   */
  /**
   * Send a capture to the desktop app.
   *
   * Returns a result object rather than a boolean, because "the app is closed"
   * and "the app refused this request" are different problems and used to be
   * reported as the same misleading "ZDM is not running" toast — which is what
   * appeared when the app was actually running but could not open its prompt
   * window.
   */
  async function sendToApp(payload) {
    let refused = null;

    try {
      const res = await fetch(BRIDGE, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (res.ok) {
        const data = await res.json().catch(() => ({}));
        if (data.status !== 'error') return { ok: true };
        // The app answered, so it is running; it just could not comply.
        refused = data.error || data.message || '';
      }
    } catch {
      /* bridge not reachable: fall through to native messaging */
    }

    try {
      const reply = await chrome.runtime.sendMessage({ action: 'native_forward', payload });
      if (reply && reply.ok) return { ok: true };
      if (reply && reply.error) refused = reply.error;
    } catch {
      /* no native host either */
    }

    return refused ? { ok: false, refused, message: refused } : { ok: false, offline: true };
  }

  function buildButton() {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'zdm-float-btn';
    btn.setAttribute('aria-label', strings());
    // Arabic reads right-to-left; mirror the icon/label order accordingly.
    const direction = window.ZDM_I18N ? window.ZDM_I18N.activeDirection() : 'ltr';
    btn.setAttribute('dir', direction);
    btn.style.direction = direction;
    // Build the contents with DOM APIs instead of innerHTML: the label is
    // translated text, and assigning it through innerHTML is both an XSS risk and
    // an AMO review warning (UNSAFE_VAR_ASSIGNMENT).
    const inner = document.createElement('span');
    inner.className = 'zdm-float-inner';

    const svgNs = 'http://www.w3.org/2000/svg';
    const svg = document.createElementNS(svgNs, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('aria-hidden', 'true');
    svg.setAttribute('width', '16');
    svg.setAttribute('height', '16');

    const arrow = document.createElementNS(svgNs, 'path');
    arrow.setAttribute('d', 'M12 3v10m0 0 4-4m-4 4-4-4');
    arrow.setAttribute('fill', 'none');
    arrow.setAttribute('stroke', 'currentColor');
    arrow.setAttribute('stroke-width', '2.2');
    arrow.setAttribute('stroke-linecap', 'round');
    arrow.setAttribute('stroke-linejoin', 'round');

    const tray = document.createElementNS(svgNs, 'path');
    tray.setAttribute('d', 'M4 15v3a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-3');
    tray.setAttribute('fill', 'none');
    tray.setAttribute('stroke', 'currentColor');
    tray.setAttribute('stroke-width', '2.2');
    tray.setAttribute('stroke-linecap', 'round');

    svg.appendChild(arrow);
    svg.appendChild(tray);

    const label = document.createElement('span');
    label.className = 'zdm-float-label';
    label.textContent = strings();  // textContent cannot inject markup

    inner.appendChild(svg);
    inner.appendChild(label);

    // Close affordance. Separate element so a click on it does not start a
    // download — the container is one big <button>.
    const dismiss = document.createElement('span');
    dismiss.className = 'zdm-float-close';
    dismiss.setAttribute('role', 'button');
    dismiss.setAttribute('tabindex', '0');
    dismiss.setAttribute('aria-label', 'Dismiss');
    dismiss.title = window.ZDM_I18N ? window.ZDM_I18N.t('dismiss') : 'Dismiss';
    dismiss.textContent = '×';
    dismiss.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      // Remember only this video, so the next one shows the chip again.
      if (activeVideo) dismissedFor.add(playbackKey(activeVideo));
      unmountButton();
    });
    dismiss.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      event.stopPropagation();
      if (activeVideo) dismissedFor.add(playbackKey(activeVideo));
      unmountButton();
    });

    btn.appendChild(inner);
    btn.appendChild(dismiss);

    return btn;
  }

  function mountButton(video) {
    // The button must always post the video the user is actually looking at.
    // Returning the existing element kept the *first* video's closure, so a second
    // player (or an SPA episode swap) silently downloaded the wrong URL.
    if (activeButton && activeVideo === video) return activeButton;
    if (activeButton) unmountButton();

    const btn = buildButton();

    // Click: capture, then disappear — never stay floating over the page.
    btn.addEventListener('click', async (event) => {
      event.preventDefault();
      event.stopPropagation();

      const target = resolveTargetUrl(video);
      dismissedFor.add(playbackKey(video));
      unmountButton();

      const result = await sendToApp({
        type: 'open_video_modal',
        url: target,
        title: document.title,
        referer: location.href,
      });

      if (!result.ok) {
        // Distinguish a closed app from a rejected request: showing "ZDM is not
        // running" for a running app sent the user chasing the wrong problem.
        const text = result.offline
          ? window.ZDM_I18N
            ? window.ZDM_I18N.t('offline')
            : 'ZDM is not running'
          : `${window.ZDM_I18N ? window.ZDM_I18N.t('failed') : 'ZDM could not start'}${
              result.refused ? `: ${result.refused}` : ''
            }`;
        showToast(text);
      }
    });

    btn.addEventListener('mouseenter', () => btn.classList.add('zdm-hover'));
    btn.addEventListener('mouseleave', () => btn.classList.remove('zdm-hover'));

    document.documentElement.appendChild(btn);
    activeButton = btn;
    activeVideo = video;
    return btn;
  }

  /** Update the label without rebuilding the element. */
  function relabelButton() {
    if (!activeButton) return;
    const label = activeButton.querySelector('.zdm-float-label');
    if (label) label.textContent = strings();
    activeButton.setAttribute('aria-label', strings());
  }

  function unmountButton() {
    if (activeButton) {
      activeButton.remove();
      activeButton = null;
    }
    activeVideo = null;
  }

  function showToast(text) {
    const toast = document.createElement('div');
    toast.className = 'zdm-toast';
    toast.setAttribute('dir', window.ZDM_I18N ? window.ZDM_I18N.activeDirection() : 'ltr');
    toast.textContent = text;
    document.documentElement.appendChild(toast);
    setTimeout(() => toast.classList.add('zdm-toast-out'), 2400);
    setTimeout(() => toast.remove(), 3000);
  }

  /** Position the button over the video's top-right corner. */
  function reposition(video) {
    if (!activeButton) return;
    const rect = video.getBoundingClientRect();
    const offscreen =
      rect.bottom < 0 ||
      rect.top > window.innerHeight ||
      rect.right < 0 ||
      rect.left > window.innerWidth;

    if (offscreen || !isUsableVideo(video)) {
      activeButton.classList.add('zdm-hidden');
      return;
    }

    activeButton.classList.remove('zdm-hidden');
    const top = Math.max(12, rect.top + 12);
    const left = Math.min(window.innerWidth - 12, rect.right - 12);
    activeButton.style.top = `${top}px`;
    activeButton.style.left = `${left}px`;
    activeButton.style.transform = 'translateX(-100%)';
  }

  let rafHandle = 0;
  function scheduleReposition(video) {
    if (rafHandle) return;
    rafHandle = requestAnimationFrame(() => {
      rafHandle = 0;
      reposition(video);
    });
  }

  /** Wire one video element exactly once. */
  function attach(video) {
    if (!snifferEnabled) return;
    if (controllers.has(video)) return;

    const onPlay = () => {
      if (dismissedFor.has(playbackKey(video))) return;
      if (!isUsableVideo(video)) return;
      mountButton(video);
      reposition(video);
    };

    let lastKey = '';

    const onSourceChange = () => {
      const key = playbackKey(video);
      if (key !== lastKey) {
        lastKey = key;
        // New title in the same element: it deserves its own button again.
        dismissedFor.delete(key);
      }
    };

    const onIdle = () => {
      // Pause / ended / emptied: the download prompt is no longer relevant.
      if (activeButton) unmountButton();
    };

    const onScroll = () => scheduleReposition(video);

    video.addEventListener('loadstart', onSourceChange);
    video.addEventListener('loadedmetadata', onSourceChange);
    video.addEventListener('play', () => {
      onSourceChange();
      onPlay();
    });
    video.addEventListener('playing', () => {
      onSourceChange();
      onPlay();
    });
    video.addEventListener('pause', onIdle);
    video.addEventListener('ended', onIdle);
    video.addEventListener('emptied', onIdle);
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll, { passive: true });

    controllers.set(video, () => {
      video.removeEventListener('loadstart', onSourceChange);
      video.removeEventListener('loadedmetadata', onSourceChange);
      video.removeEventListener('pause', onIdle);
      video.removeEventListener('ended', onIdle);
      video.removeEventListener('emptied', onIdle);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    });

    if (!video.paused && isUsableVideo(video)) onPlay();
  }

  /**
   * Many players (ok.ru, vimeo, and countless custom ones) render their <video>
   * inside an open shadow root, which `document.querySelectorAll` never sees and
   * a normal MutationObserver never fires for. Collect videos by walking the
   * tree through every open shadow root, and observe each root as we discover
   * it so videos added later are still caught.
   */
  const observedRoots = new WeakSet();
  const rootObservers = [];

  function observeRoot(root) {
    if (observedRoots.has(root)) return;
    observedRoots.add(root);
    try {
      const mo = new MutationObserver(() => scan());
      mo.observe(root, { childList: true, subtree: true });
      rootObservers.push(mo);
    } catch {
      /* detached root */
    }
  }

  function collectVideos(root, out) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
    let node = walker.nextNode();
    while (node) {
      if (node instanceof HTMLVideoElement) out.push(node);
      if (node.shadowRoot) {
        observeRoot(node.shadowRoot);
        collectVideos(node.shadowRoot, out);
      }
      node = walker.nextNode();
    }
  }

  /** Attach to every current and future video, including inside shadow DOM. */
  function scan() {
    const videos = [];
    collectVideos(document, videos);
    for (const video of videos) {
      attach(video);
    }

    // Drop the button when its own video is no longer playing or visible.
    if (activeButton && activeVideo) {
      if (
        !activeVideo.isConnected ||
        activeVideo.paused ||
        !isUsableVideo(activeVideo) ||
        dismissedFor.has(playbackKey(activeVideo))
      ) {
        unmountButton();
      }
    }
  }

  /** Capture play events even for videos created after our scan. */
  // The app can switch language while this page stays open; re-read the
  // preference on visibility changes so the label follows without a reload.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState !== 'visible') return;
    try {
      chrome.runtime.sendMessage({ action: 'get_prefs' }, (reply) => {
        if (chrome.runtime.lastError || !reply || !reply.language || !window.ZDM_I18N) return;
        const changed = window.ZDM_I18N.activeLanguage() !== reply.language;
        window.ZDM_I18N.setAppLanguage(reply.language);
        if (changed && activeVideo) {
          const currentVideo = activeVideo;
          unmountButton();
          if (isUsableVideo(currentVideo) && !currentVideo.paused) {
            mountButton(currentVideo);
            reposition(currentVideo);
          }
        }
      });
    } catch {
      /* extension context may be gone after an update */
    }
  });

  document.addEventListener(
    'play',
    (event) => {
      const target = event.target;
      if (target instanceof HTMLVideoElement) {
        attach(target);
        if (!dismissedFor.has(playbackKey(target)) && isUsableVideo(target)) {
          mountButton(target);
          reposition(target);
        }
      }
    },
    true
  );

  document.addEventListener(
    'pause',
    (event) => {
      // Only react to the video our button represents. Pages autoplay ads and
      // previews, and unmounting on any of those hid the button while the real
      // video was still playing.
      if (event.target instanceof HTMLVideoElement && event.target === activeVideo) {
        unmountButton();
      }
    },
    true
  );

  // SPA route changes must reset the dismissed state: a new "video" in the same
  // tab deserves its own button.
  let lastHref = location.href;
  const onNavigate = () => {
    if (location.href === lastHref) return;
    lastHref = location.href;
    dismissedFor = new Set();
    unmountButton();
    setTimeout(scan, 400);
  };

  try {
    const pushState = history.pushState;
    history.pushState = function (...args) {
      pushState.apply(this, args);
      onNavigate();
    };
    const replaceState = history.replaceState;
    history.replaceState = function (...args) {
      replaceState.apply(this, args);
      onNavigate();
    };
    window.addEventListener('popstate', onNavigate);
  } catch {
    /* some pages freeze history; the MutationObserver still covers us */
  }

  const observer = new MutationObserver(() => {
    scan();
  });

  function boot() {
    observer.observe(document.documentElement, { childList: true, subtree: true });

    try {
      chrome.runtime.sendMessage({ action: 'get_prefs' }, (reply) => {
        if (chrome.runtime.lastError || !reply) return;

        // Follow the desktop app's language for the button label.
        //
        // `unmountButton()` clears `activeVideo`, so the video must be captured
        // *before* remounting — otherwise the condition is always false and the
        // label never updates (the "extension stays English" bug).
        if (reply.reachable && reply.language && window.ZDM_I18N) {
          window.ZDM_I18N.setAppLanguage(reply.language);

          const currentVideo = activeVideo;
          if (currentVideo) {
            unmountButton();
            if (isUsableVideo(currentVideo) && !currentVideo.paused) {
              mountButton(currentVideo);
              reposition(currentVideo);
            }
          }
        }

        if (reply.reachable && reply.sniffer_enabled === false) {
          snifferEnabled = false;
          unmountButton();
        }
      });
    } catch {
      /* keep the default */
    }

    scan();
    // Late-loading players (lazy hydration) are common; keep a light poll for a
    // short while instead of a permanent interval.
    let ticks = 0;
    const timer = setInterval(() => {
      scan();
      if (++ticks > 40) clearInterval(timer);
    }, 500);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot, { once: true });
  } else {
    boot();
  }
})();
