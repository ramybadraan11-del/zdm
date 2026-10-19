/* ==========================================================================
   Zyzoom Media Sniffer — controller
   ========================================================================== */
(function () {
  'use strict';

  const { esc, bytes, icon } = window.UI;
  const api = window.zyzoom;

  const state = {
    media: [],
    checked: new Set(),
  };

  const el = {
    list: document.getElementById('media-list'),
    url: document.getElementById('sniff-url'),
    count: document.getElementById('detected-count'),
    detected: document.getElementById('sb-detected'),
    checkedCount: document.getElementById('sb-checked'),
  };

  const SAMPLE = [
    { title: 'مقطع开场 - جودة عالية', resolution: '1080p', format: 'MP4', size: 148 * 1024 * 1024, duration: '08:12' },
    { title: 'مقطع تعليمي - الدقة فائقة', resolution: '4K', format: 'MKV', size: 812 * 1024 * 1024, duration: '02:08' },
    { title: 'مقطع رياضي - جودة فائقة', resolution: '4K', format: 'MP4', size: 430 * 1024 * 1024, duration: '00:09' },
    { title: 'مقطع وثائقي - الدقة 4K', resolution: '4K', format: 'MKV', size: 1.4 * 1024 * 1024 * 1024, duration: '02:41' },
    { title: 'مقطع إخباري - جودة 1080p', resolution: '1080p', format: 'MKV', size: 690 * 1024 * 1024, duration: '05:56' },
  ];

  function render() {
    if (!state.media.length) {
      el.list.innerHTML = `<div class="empty">${icon('search', 46)}<span>أدخل رابط صفحة في شريط العنوان ثم اضغط زر الفحص</span></div>`;
    } else {
      el.list.innerHTML = state.media.map(renderItem).join('');
    }
    el.count.textContent = state.media.length;
    el.detected.textContent = state.media.length;
    el.checkedCount.textContent = state.checked.size;
  }

  function renderItem(item) {
    const done = item.status === 'queued' || item.status === 'downloading';
    return `
      <div class="media-item" data-id="${esc(item.id)}">
        <div class="media-thumb">
          ${icon('video')}
          <span class="badge">${esc(item.duration)}</span>
        </div>
        <div class="media-info">
          <div class="media-title" title="${esc(item.title)}">${esc(item.title)}</div>
          <div class="media-sub">
            <span><b>${esc(item.resolution)}</b> دقة</span>
            <span>امتداد <b>${esc(item.format)}</b></span>
            <span>${esc(bytes(item.size))}</span>
          </div>
        </div>
        <div class="media-actions">
          <button class="btn-download ${done ? 'done' : ''}" data-action="download" data-id="${esc(item.id)}">
            ${done ? 'في القائمة' : 'نزّل الآن'}
          </button>
        </div>
        <div class="media-check">
          <input type="checkbox" data-check="${esc(item.id)}" ${state.checked.has(item.id) ? 'checked' : ''} />
        </div>
      </div>`;
  }

  function scan() {
    const url = el.url.value.trim();
    if (!/^https?:\/\//i.test(url)) {
      el.url.focus();
      return;
    }
    state.media = SAMPLE.map((sample, index) => ({
      id: `media-${Date.now()}-${index}`,
      url: `${url.replace(/\/+$/, '')}/media-${index + 1}.${sample.format.toLowerCase()}`,
      status: 'idle',
      ...sample,
    }));
    state.checked.clear();
    render();
  }

  async function download(item) {
    if (item.status === 'queued' || item.status === 'downloading') return;
    item.status = 'queued';
    render();
    await api.downloads.add({
      url: item.url,
      name: `${item.title}.${item.format.toLowerCase()}`,
      size: item.size,
      description: `${item.resolution} — ${item.format}`,
      browser: 'Zyzoom Sniffer',
      type: 'فيديو',
    });
  }

  function bindEvents() {
    document.addEventListener('click', (event) => {
      const win = event.target.closest('[data-window]');
      if (win) {
        api.window.control(win.getAttribute('data-window'));
        return;
      }

      const action = event.target.closest('[data-action]');
      if (!action) return;
      const name = action.getAttribute('data-action');

      if (name === 'scan') {
        scan();
      } else if (name === 'download') {
        const item = state.media.find((m) => m.id === action.getAttribute('data-id'));
        if (item) download(item);
      } else if (name === 'clear') {
        state.media = [];
        state.checked.clear();
        render();
      } else if (name === 'open-folder') {
        api.shell.openExternal('file://' + (window.localStorage.getItem('zyzoom.settings') ? JSON.parse(window.localStorage.getItem('zyzoom.settings')).dir : '~/Downloads'));
      } else if (name === 'toggle-nav') {
        document.querySelector('.app-shell').classList.toggle('nav-collapsed');
      }
    });

    document.addEventListener('change', (event) => {
      const id = event.target.getAttribute('data-check');
      if (!id) return;
      if (event.target.checked) state.checked.add(id);
      else state.checked.delete(id);
      el.checkedCount.textContent = state.checked.size;
    });

    el.url.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') scan();
    });
  }

  bindEvents();
  render();
})();
