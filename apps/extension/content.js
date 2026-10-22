//! ZDM Extension Content Script (Smart Universal Video Sniffer & Floating Panel)
//! Developed by Ramy Badraan

(() => {
  // Prevent double injection
  if (window.__ZDM_CONTENT_INJECTED__) return;
  window.__ZDM_CONTENT_INJECTED__ = true;

  const isKnownVideoPlatform = () => {
    const host = window.location.hostname.toLowerCase();
    return (
      host.includes("youtube.com") ||
      host.includes("youtu.be") ||
      host.includes("facebook.com") ||
      host.includes("fb.watch") ||
      host.includes("twitter.com") ||
      host.includes("x.com") ||
      host.includes("instagram.com") ||
      host.includes("tiktok.com") ||
      host.includes("reddit.com") ||
      host.includes("vimeo.com") ||
      host.includes("dailymotion.com") ||
      host.includes("twitch.tv") ||
      host.includes("rumble.com") ||
      host.includes("bitchute.com") ||
      host.includes("streamable.com") ||
      host.includes("pinterest.com") ||
      host.includes("linkedin.com") ||
      host.includes("threads.net")
    );
  };

  function getCleanVideoUrl(mediaEl) {
    // 1. Direct src if http
    const directSrc = mediaEl.currentSrc || mediaEl.src;
    if (directSrc && directSrc.startsWith("http") && !directSrc.startsWith("blob:") && !directSrc.startsWith("data:")) {
      return directSrc;
    }

    // 2. Child <source> elements
    const sources = mediaEl.querySelectorAll("source");
    for (const s of sources) {
      if (s.src && s.src.startsWith("http") && !s.src.startsWith("blob:")) {
        return s.src;
      }
    }

    // 3. Performance resource entries (catches HLS .m3u8, MP4, DASH .mpd)
    try {
      const resources = performance.getEntriesByType("resource");
      const mediaEntries = resources.filter((r) => {
        const u = r.name.toLowerCase();
        return (
          u.includes(".mp4") ||
          u.includes(".m3u8") ||
          u.includes(".mpd") ||
          u.includes(".webm") ||
          u.includes(".m4v") ||
          u.includes("/video/") ||
          r.initiatorType === "video"
        );
      });
      if (mediaEntries.length > 0) {
        return mediaEntries[mediaEntries.length - 1].name;
      }
    } catch (e) {}

    // 4. For known video platforms, the page URL is best for yt-dlp
    if (isKnownVideoPlatform()) {
      return window.top?.location?.href || window.location.href;
    }

    // 5. Fallback to page URL
    return window.top?.location?.href || window.location.href;
  }

  function createFloatingBadge(mediaEl) {
    if (mediaEl.dataset.zdmAttached || mediaEl.dataset.zdmDismissed) return;
    mediaEl.dataset.zdmAttached = "true";

    const panel = document.createElement("div");
    panel.className = "zdm-sniffer-panel";
    panel.style.position = "fixed";
    panel.style.zIndex = "2147483647"; // Always on top
    panel.style.display = "none";
    panel.style.alignItems = "center";
    panel.style.gap = "6px";
    panel.style.padding = "4px 8px";
    panel.style.background = "rgba(11, 16, 32, 0.92)";
    panel.style.backdropFilter = "blur(10px)";
    panel.style.border = "1px solid rgba(0, 245, 255, 0.5)";
    panel.style.borderRadius = "24px";
    panel.style.boxShadow = "0 8px 24px rgba(0, 0, 0, 0.6), 0 0 14px rgba(0, 245, 255, 0.35)";
    panel.style.direction = "rtl";
    panel.style.transition = "opacity 0.2s ease, transform 0.2s ease";
    panel.style.cursor = "default";
    panel.style.userSelect = "none";

    // Download Button
    const dlBtn = document.createElement("button");
    dlBtn.innerHTML = `
      <span style="display:inline-flex;align-items:center;gap:6px;color:#00F5FF;font-weight:800;font-size:12px;font-family:system-ui,-apple-system,sans-serif;">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#00F5FF" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="7 10 12 15 17 10"></polyline>
          <line x1="12" y1="15" x2="12" y2="3"></line>
        </svg>
        تحميل الفيديو (ZDM)
      </span>
    `;
    dlBtn.style.background = "none";
    dlBtn.style.border = "none";
    dlBtn.style.padding = "4px 6px";
    dlBtn.style.cursor = "pointer";
    dlBtn.style.outline = "none";

    // Dismiss "X" Button
    const closeBtn = document.createElement("button");
    closeBtn.innerHTML = `
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#94A3B8" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <line x1="18" y1="6" x2="6" y2="18"></line>
        <line x1="6" y1="6" x2="18" y2="18"></line>
      </svg>
    `;
    closeBtn.title = "إخفاء الزر لهذا الفيديو";
    closeBtn.style.background = "rgba(255, 255, 255, 0.08)";
    closeBtn.style.border = "none";
    closeBtn.style.borderRadius = "50%";
    closeBtn.style.width = "20px";
    closeBtn.style.height = "20px";
    closeBtn.style.display = "inline-flex";
    closeBtn.style.alignItems = "center";
    closeBtn.style.justifyContent = "center";
    closeBtn.style.cursor = "pointer";
    closeBtn.style.outline = "none";
    closeBtn.style.transition = "background 0.2s";

    closeBtn.onmouseenter = () => {
      closeBtn.style.background = "rgba(244, 63, 94, 0.4)";
    };
    closeBtn.onmouseleave = () => {
      closeBtn.style.background = "rgba(255, 255, 255, 0.08)";
    };

    // Click handler for X (Dismiss)
    closeBtn.onclick = (e) => {
      e.preventDefault();
      e.stopPropagation();
      mediaEl.dataset.zdmDismissed = "true";
      panel.remove();
    };

    // Click handler for Download
    dlBtn.onclick = (e) => {
      e.preventDefault();
      e.stopPropagation();

      const videoUrl = getCleanVideoUrl(mediaEl);
      if (videoUrl) {
        chrome.runtime.sendMessage({
          action: "open_video_modal",
          url: videoUrl,
          title: document.title,
        });
      }

      // Hide and remove badge immediately after clicking
      panel.remove();
    };

    panel.appendChild(dlBtn);
    panel.appendChild(closeBtn);
    document.body.appendChild(panel);

    function updatePosition() {
      const rect = mediaEl.getBoundingClientRect();
      if (rect.width < 120 || rect.height < 70 || rect.bottom < 0 || rect.top > window.innerHeight) {
        panel.style.display = "none";
        return false;
      }
      const topPos = Math.max(10, rect.top + 10);
      const rightPos = Math.max(10, window.innerWidth - rect.right + 10);
      panel.style.top = `${topPos}px`;
      panel.style.right = `${rightPos}px`;
      return true;
    }

    let hideTimer = null;

    function showPanel() {
      if (mediaEl.dataset.zdmDismissed) return;
      if (updatePosition()) {
        panel.style.display = "inline-flex";
        panel.style.opacity = "1";
        clearTimeout(hideTimer);
      }
    }

    function scheduleHide() {
      clearTimeout(hideTimer);
      hideTimer = setTimeout(() => {
        panel.style.opacity = "0";
        setTimeout(() => {
          if (panel.style.opacity === "0") {
            panel.style.display = "none";
          }
        }, 200);
      }, 3500);
    }

    // Show when mouse enters media or when video plays
    mediaEl.addEventListener("mouseenter", showPanel);
    mediaEl.addEventListener("play", () => {
      showPanel();
      scheduleHide();
    });
    mediaEl.addEventListener("playing", () => {
      showPanel();
      scheduleHide();
    });
    mediaEl.addEventListener("mouseleave", scheduleHide);

    // Keep visible while mouse is hovering the badge itself
    panel.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    panel.addEventListener("mouseleave", scheduleHide);

    // Update position on scroll/resize
    window.addEventListener("scroll", () => {
      if (panel.style.display !== "none") updatePosition();
    }, { passive: true });
    window.addEventListener("resize", () => {
      if (panel.style.display !== "none") updatePosition();
    }, { passive: true });
  }

  // Find genuine videos (ignore hidden 0x0 trackers)
  function scanMedia() {
    const mediaElements = document.querySelectorAll("video");
    mediaElements.forEach((el) => {
      const rect = el.getBoundingClientRect();
      if (rect.width >= 120 && rect.height >= 70) {
        createFloatingBadge(el);
      }
    });
  }

  // Global event capture for play and playing events across any site
  document.addEventListener("play", (e) => {
    if (e.target && e.target.tagName === "VIDEO") {
      createFloatingBadge(e.target);
    }
  }, true);

  document.addEventListener("playing", (e) => {
    if (e.target && e.target.tagName === "VIDEO") {
      createFloatingBadge(e.target);
    }
  }, true);

  document.addEventListener("timeupdate", (e) => {
    if (e.target && e.target.tagName === "VIDEO" && !e.target.dataset.zdmAttached) {
      const rect = e.target.getBoundingClientRect();
      if (rect.width >= 120 && rect.height >= 70) {
        createFloatingBadge(e.target);
      }
    }
  }, true);

  // Scan initially and observe DOM changes
  scanMedia();
  const observer = new MutationObserver(() => scanMedia());
  if (document.body) {
    observer.observe(document.body, { childList: true, subtree: true });
  } else {
    document.addEventListener("DOMContentLoaded", () => {
      observer.observe(document.body, { childList: true, subtree: true });
    });
  }
})();
