//! ZDM Extension Content Script (Media Sniffer & Floating Button)
//! Developed by Ramy Badraan

(() => {
  // Sniff HTML5 video/audio elements
  function attachDownloadBadge(mediaElement) {
    if (mediaElement.dataset.zdmAttached) return;
    mediaElement.dataset.zdmAttached = "true";

    const container = document.createElement("div");
    container.style.position = "absolute";
    container.style.zIndex = "999999";
    container.style.direction = "rtl";

    const btn = document.createElement("button");
    btn.innerHTML = `
      <span style="display:inline-flex;align-items:center;gap:6px;padding:6px 12px;background:linear-gradient(135deg,#00F5FF,#7C4DFF);color:#0B1020;font-weight:800;font-size:12px;font-family:sans-serif;border:none;border-radius:20px;cursor:pointer;box-shadow:0 4px 14px rgba(0,245,255,0.4);">
        ⚡ تحميل مع ZDM
      </span>
    `;

    btn.onclick = (e) => {
      e.preventDefault();
      e.stopPropagation();
      const mediaUrl = mediaElement.currentSrc || mediaElement.src;
      if (mediaUrl) {
        chrome.runtime.sendMessage({
          action: "download_media",
          url: mediaUrl,
          filename: document.title.replace(/[\/\\?%*:|"<>]/g, '_'),
        });
      }
    };

    // Position relative to media
    const rect = mediaElement.getBoundingClientRect();
    container.style.top = `${window.scrollY + rect.top + 10}px`;
    container.style.right = `${window.innerWidth - (rect.right + window.scrollX) + 10}px`;
    container.appendChild(btn);

    document.body.appendChild(container);

    // Hide button after 5 seconds of inactivity
    let timeout;
    mediaElement.addEventListener("mouseenter", () => {
      container.style.display = "block";
      clearTimeout(timeout);
    });
    mediaElement.addEventListener("mouseleave", () => {
      timeout = setTimeout(() => {
        container.style.display = "none";
      }, 3000);
    });
  }

  // Periodic scan for dynamic video elements
  setInterval(() => {
    const videos = document.querySelectorAll("video, audio");
    videos.forEach((v) => {
      if (v.src || v.currentSrc) {
        attachDownloadBadge(v);
      }
    });
  }, 2000);
})();
