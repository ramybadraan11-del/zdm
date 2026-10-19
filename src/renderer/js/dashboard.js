/* ==========================================================================
   Zyzoom Download Manager — Dashboard renderer & controller
   ========================================================================== */
(function () {
  'use strict';

  const { esc, bytes, speed, duration, statusInfo, ringSvg, icon, SpeedChart, MB } = window.UI;
  const api = window.zyzoom;

  const state = {
    downloads: [],
    captures: [],
    filter: 'all',
    viewMode: 'cards',
    tab: 'downloads',
    selected: new Set(),
    progressId: null,
    chart: null,
    settings: {
      dir: '~/Downloads',
      connections: 16,
      limit: 0,
      resume: true,
      tray: true,
      speed: true,
    },
  };

  const el = {
    nav: document.getElementById('nav'),
    chips: document.getElementById('chips'),
    cardsView: document.getElementById('cards-view'),
    tableView: document.getElementById('table-view'),
    tableBody: document.getElementById('table-body'),
    activeList: document.getElementById('active-list'),
    pausedList: document.getElementById('paused-list'),
    otherList: document.getElementById('other-list'),
    activeCount: document.getElementById('active-count'),
    pausedCount: document.getElementById('paused-count'),
    otherCount: document.getElementById('other-count'),
    viewTitle: document.getElementById('view-title'),
    viewSubtitle: document.getElementById('view-subtitle'),
    scroll: document.getElementById('scroll'),
    statusbar: {
      total: document.getElementById('sb-total'),
      speed: document.getElementById('sb-speed'),
      conn: document.getElementById('sb-conn'),
      path: document.getElementById('sb-path'),
    },
    progressModal: document.getElementById('progress-modal'),
    addModal: document.getElementById('add-modal'),
    settingsModal: document.getElementById('settings-modal'),
    bigRingFill: document.getElementById('big-ring-fill'),
    bigRingLabel: document.getElementById('big-ring-label'),
  };

  /* ------------------------------------------------------------ filtering */

  function matches(item, filter) {
    switch (filter) {
      case 'all':
        return true;
      case 'deferred':
        return item.status === 'queued' || item.status === 'scheduled';
      default:
        return item.status === filter;
    }
  }

  function visibleDownloads() {
    return state.downloads.filter((item) => matches(item, state.filter));
  }

  /* ------------------------------------------------------------ rendering */

  function cardMarkup(item) {
    const info = statusInfo(item.status);
    const selected = state.selected.has(item.id) ? ' selected' : '';
    const barCls = item.status === 'paused' ? 'paused' : item.status === 'completed' ? 'completed' : item.status === 'failed' ? 'failed' : '';
    return `
      <div class="dl-card${selected}" data-id="${esc(item.id)}">
        ${ringSvg(item.percent, info.cls)}
        <div class="dl-main">
          <div class="dl-name-row">
            <span class="dl-name" title="${esc(item.name)}">${esc(item.name)}</span>
            <span class="dl-size">${esc(item.sizeText || bytes(item.size))}</span>
          </div>
          <div class="dl-meta">
            <span>${esc(item.speedText || speed(item.speed))}</span>
            <span class="sep">•</span>
            <span>متبقٍ <b>${esc(item.timeLeftText || '--:--:--')}</b></span>
            <span class="sep">•</span>
            <span>${esc(item.browser || '-')}</span>
            <span class="sep">•</span>
            <span class="status-pill ${info.cls}">${esc(info.label)}</span>
          </div>
          <div class="bar ${barCls}"><i style="width:${item.percent.toFixed(1)}%"></i></div>
        </div>
        <div class="dl-actions">
          ${
            item.status === 'downloading'
              ? `<button class="mini-btn" title="إيقاف مؤقت" data-row-action="pause" data-id="${esc(item.id)}">${icon('pause')}</button>`
              : `<button class="mini-btn" title="بدء" data-row-action="start" data-id="${esc(item.id)}">${icon('play')}</button>`
          }
          <button class="mini-btn" title="تفاصيل" data-row-action="details" data-id="${esc(item.id)}">${icon('wave')}</button>
          <button class="mini-btn danger" title="حذف" data-row-action="remove" data-id="${esc(item.id)}">${icon('trash')}</button>
        </div>
      </div>`;
  }

  function rowMarkup(item) {
    const info = statusInfo(item.status);
    const barCls = item.status === 'paused' ? 'paused' : item.status === 'completed' ? 'completed' : item.status === 'failed' ? 'failed' : '';
    return `
      <tr data-id="${esc(item.id)}" class="${state.selected.has(item.id) ? 'selected' : ''}">
        <td class="check"><input type="checkbox" data-select="${esc(item.id)}" ${state.selected.has(item.id) ? 'checked' : ''} /></td>
        <td class="name-cell" title="${esc(item.name)}">${esc(item.name)}</td>
        <td class="mono">${esc(item.sizeText || bytes(item.size))}</td>
        <td class="mono">${esc(item.speedText || speed(item.speed))}</td>
        <td class="mono">${esc(item.timeLeftText || '--:--:--')}</td>
        <td>${esc(item.browser || '-')}</td>
        <td><span class="status-pill ${info.cls}">${esc(info.label)}</span></td>
        <td class="mono">${esc(item.downloadedText || bytes(item.downloaded))}</td>
        <td class="bar-cell">
          <div class="bar ${barCls}"><i style="width:${item.percent.toFixed(1)}%"></i></div>
          <span class="mono" style="font-size:11px">${item.percent.toFixed(0)}%</span>
        </td>
        <td class="desc-cell" title="${esc(item.description || '')}">${esc(item.description || '-')}</td>
        <td class="mono">${esc(item.addedAtText || '-')}</td>
        <td class="mono">${esc(item.finishedAtText || '-')}</td>
      </tr>`;
  }

  function renderCards(items) {
    const active = items.filter((i) => i.status === 'downloading');
    const paused = items.filter((i) => i.status === 'paused');
    const other = items.filter((i) => !['downloading', 'paused'].includes(i.status));

    el.activeList.innerHTML = active.map(cardMarkup).join('') || emptyMarkup('لا توجد تنزيلات جارية');
    el.pausedList.innerHTML = paused.map(cardMarkup).join('') || emptyMarkup('لا توجد تنزيلات متوقفة');
    el.otherList.innerHTML = other.map(cardMarkup).join('') || emptyMarkup('لا توجد مهام أخرى');

    el.activeCount.textContent = `(${active.length})`;
    el.pausedCount.textContent = `(${paused.length})`;
    el.otherCount.textContent = `(${other.length})`;
  }

  function emptyMarkup(text) {
    return `<div class="empty">${icon('download', 46)}<span>${esc(text)}</span></div>`;
  }

  function renderTable(items) {
    if (!items.length) {
      el.tableBody.innerHTML = `<tr><td colspan="12" style="text-align:center;padding:32px;color:var(--text-mute)">لا توجد نتائج مطابقة</td></tr>`;
      return;
    }
    el.tableBody.innerHTML = items.map(rowMarkup).join('');
  }

  function renderCounts() {
    const counts = {
      all: state.downloads.length,
      downloading: 0,
      completed: 0,
      queued: 0,
      scheduled: 0,
      failed: 0,
    };
    for (const item of state.downloads) {
      if (counts[item.status] !== undefined) counts[item.status] += 1;
    }
    document.querySelectorAll('[data-count]').forEach((node) => {
      const key = node.getAttribute('data-count');
      node.textContent = counts[key] !== undefined ? counts[key] : 0;
    });
  }

  function renderStatusbar() {
    let totalSpeed = 0;
    let connections = 0;
    for (const item of state.downloads) {
      if (item.status === 'downloading') {
        totalSpeed += item.speed;
        connections += item.connections || 1;
      }
    }
    el.statusbar.total.textContent = state.downloads.length;
    el.statusbar.speed.textContent = speed(totalSpeed);
    el.statusbar.conn.textContent = connections;
    el.statusbar.path.textContent = state.settings.dir;
  }

  function render() {
    const items = visibleDownloads();
    renderCounts();
    renderCards(items);
    renderTable(items);
    renderStatusbar();
    updateChart();
    if (state.progressId) renderProgressModal();
  }

  /* ---------------------------------------------------------------- chart */

  function updateChart() {
    if (!state.chart) return;
    const totalSpeed = state.downloads
      .filter((i) => i.status === 'downloading')
      .reduce((sum, i) => sum + i.speed, 0);
    const expected = totalSpeed * 1.08;
    const last = state.chart.real.length ? state.chart.real[state.chart.real.length - 1] : 0;
    state.chart.push(totalSpeed, expected);
    void last;
  }

  /* --------------------------------------------------------------- modals */

  function openModal(id) {
    const modal = document.getElementById(id);
    if (modal) modal.classList.add('open');
  }

  function closeModal(id) {
    const modal = document.getElementById(id);
    if (modal) modal.classList.remove('open');
  }

  function renderProgressModal() {
    const item = state.downloads.find((d) => d.id === state.progressId);
    if (!item) return;
    const c = 2 * Math.PI * 52;
    const offset = c - (c * item.percent) / 100;
    el.bigRingFill.setAttribute('stroke-dasharray', c.toFixed(2));
    el.bigRingFill.setAttribute('stroke-dashoffset', offset.toFixed(2));
    el.bigRingLabel.textContent = `${item.percent.toFixed(0)}%`;
    document.getElementById('pg-name').textContent = item.name;
    document.getElementById('pg-size').textContent = item.sizeText || bytes(item.size);
    document.getElementById('pg-done').textContent = item.downloadedText || bytes(item.downloaded);
    document.getElementById('pg-speed').textContent = item.speedText || speed(item.speed);
    document.getElementById('pg-time').textContent = item.timeLeftText || '--:--:--';
    const bar = document.getElementById('pg-bar');
    bar.style.width = `${item.percent.toFixed(1)}%`;
    const conn = document.getElementById('pg-connections');
    const active = Math.round((item.connections || 1) / 2);
    conn.innerHTML = Array.from({ length: 32 })
      .map((_, i) => `<i class="${i < active ? 'on' : ''}"></i>`)
      .join('');
  }

  /* ---------------------------------------------------------- captures */

  function kindLabel(kind) {
    switch (kind) {
      case 'video':
        return 'فيديو';
      case 'audio':
        return 'صوت';
      case 'network':
        return 'طلب شبكة';
      case 'resource':
        return 'مورد';
      case 'link':
        return 'رابط';
      default:
        return 'وسائط';
    }
  }

  function renderCaptures(items) {
    state.captures = items || [];
    const body = document.getElementById('browser-list-body');
    const count = document.getElementById('captures-count');
    if (count) count.textContent = state.captures.length;
    if (!body) return;
    if (!state.captures.length) {
      body.innerHTML = `<div class="empty">${icon('globe', 46)}<span>لم يلتقط الامتداد أي وسائط بعد</span></div>`;
      return;
    }
    body.innerHTML = state.captures
      .map((item) => {
        let host = '';
        try {
          host = new URL(item.url).hostname;
        } catch (err) {
          host = '';
        }
        const time = new Date(item.detectedAt || Date.now()).toLocaleTimeString('ar', {
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        });
        return `<div class="browser-row" data-id="${esc(item.id)}">
          <span class="icon" style="color: var(--accent)">${icon('film', 20)}</span>
          <div style="min-width:0">
            <div class="b-name" title="${esc(item.url)}">${esc(item.title || host || item.url)}</div>
            <div class="b-meta" dir="ltr" style="text-align:start">${esc(item.url)}</div>
          </div>
          <div class="b-meta">${esc(kindLabel(item.kind))} • ${esc(time)}</div>
          <div class="b-actions">
            <button class="btn-download" data-action="capture-download" data-id="${esc(item.id)}">تنزيل الآن</button>
          </div>
        </div>`;
      })
      .join('');
  }

  async function installNativeHost() {
    const extensionId = document.getElementById('ext-id').value.trim();
    const result = await api.bridge.installHost(extensionId);
    const info = document.getElementById('bridge-info');
    if (!result || !result.written || !result.written.length) {
      info.textContent = 'تعذّر تثبيت سجلّ المضيف. تأكد من صلاحيات الكتابة في مجلد المتصفح.';
      return;
    }
    info.innerHTML = `تم تثبيت سجلّ المضيف في:<br>${result.written.map((p) => `<span dir="ltr">${esc(p)}</span>`).join('<br>')}${
      extensionId ? '' : '<br>أدخل معرّف الامتداد لتفعيل الربط في Firefox.'
    }`;
  }

  async function loadBridgeInfo() {
    const info = await api.bridge.info();
    const target = document.getElementById('bridge-info');
    if (!target || !info) return;
    target.innerHTML = `جسر الالتقاط يعمل على <span dir="ltr">http://${esc(info.host)}:${esc(info.port)}</span> — ملف الإعدادات: <span dir="ltr">${esc(info.configPath)}</span>`;
  }

  /* -------------------------------------------------------------- actions */

  async function performAction(id, action) {
    await api.downloads.action(id, action);
  }

  function bulk(action) {
    const ids = state.selected.size
      ? Array.from(state.selected)
      : state.downloads.filter((d) => d.status === 'downloading').map((d) => d.id);
    if (!ids.length) return;
    const map = { start: 'start', pause: 'pause', resume: 'start', stop: 'stop' };
    api.downloads.bulkAction(ids, map[action] || action);
  }

  async function addDownload() {
    const url = document.getElementById('add-url').value.trim();
    const name = document.getElementById('add-name').value.trim();
    const size = Number(document.getElementById('add-size').value) || 0;
    const error = document.getElementById('add-error');
    if (!/^https?:\/\//i.test(url)) {
      error.textContent = 'الرابط غير صالح. يجب أن يبدأ بـ http:// أو https://';
      return;
    }
    error.textContent = '';
    await api.downloads.add({
      url,
      name: name || decodeURIComponent(url.split('/').pop() || 'download.bin'),
      size,
      description: document.getElementById('add-desc').value.trim(),
      browser: 'Zyzoom',
      type: 'ملف',
      connections: Number(document.getElementById('add-ref').value) || 8,
    });
    closeModal('add-modal');
    document.getElementById('add-url').value = '';
    document.getElementById('add-name').value = '';
    document.getElementById('add-size').value = '';
    document.getElementById('add-desc').value = '';
  }

  function loadSettings() {
    try {
      const raw = window.localStorage.getItem('zyzoom.settings');
      if (raw) Object.assign(state.settings, JSON.parse(raw));
    } catch (err) {
      /* ignore malformed storage */
    }
    document.getElementById('set-dir').value = state.settings.dir;
    document.getElementById('set-conn').value = state.settings.connections;
    document.getElementById('set-limit').value = state.settings.limit;
    document.getElementById('set-resume').checked = state.settings.resume;
    document.getElementById('set-tray').checked = state.settings.tray;
    document.getElementById('set-speed').checked = state.settings.speed;
  }

  function saveSettings() {
    state.settings.dir = document.getElementById('set-dir').value.trim() || '~/Downloads';
    state.settings.connections = Number(document.getElementById('set-conn').value) || 16;
    state.settings.limit = Number(document.getElementById('set-limit').value) || 0;
    state.settings.resume = document.getElementById('set-resume').checked;
    state.settings.tray = document.getElementById('set-tray').checked;
    state.settings.speed = document.getElementById('set-speed').checked;
    try {
      window.localStorage.setItem('zyzoom.settings', JSON.stringify(state.settings));
    } catch (err) {
      /* storage unavailable */
    }
    document.getElementById('auto-resume').checked = state.settings.resume;
    document.getElementById('auto-speed').checked = state.settings.speed;
    api.settings.update({
      dir: state.settings.dir,
      maxConnections: state.settings.connections,
    });
    closeModal('settings-modal');
    renderStatusbar();
  }

  /* --------------------------------------------------------------- events */

  function bindEvents() {
    document.addEventListener('click', (event) => {
      const target = event.target.closest('[data-action], [data-close], [data-window], [data-row-action], [data-select], [data-status], [data-filter], [data-tab], [data-viewmode]');
      if (!target) return;

      // window controls
      if (target.hasAttribute('data-window')) {
        api.window.control(target.getAttribute('data-window'));
        return;
      }

      // close buttons
      if (target.hasAttribute('data-close')) {
        closeModal(target.getAttribute('data-close'));
        return;
      }

      // overlay background click
      if (event.target.classList && event.target.classList.contains('overlay')) {
        closeModal(event.target.id);
        return;
      }

      // row action
      const rowAction = target.getAttribute('data-row-action');
      if (rowAction) {
        const id = target.getAttribute('data-id');
        if (rowAction === 'remove') {
          api.downloads.remove(id);
          state.selected.delete(id);
        } else if (rowAction === 'details') {
          state.progressId = id;
          renderProgressModal();
          openModal('progress-modal');
        } else {
          performAction(id, rowAction);
        }
        return;
      }

      // table checkbox
      const selectId = target.getAttribute('data-select');
      if (selectId) {
        if (target.checked) state.selected.add(selectId);
        else state.selected.delete(selectId);
        render();
        return;
      }

      // nav status
      const status = target.getAttribute('data-status');
      if (status) {
        state.filter = status;
        syncActiveStates();
        render();
        return;
      }

      // chip filter
      const filter = target.getAttribute('data-filter');
      if (filter) {
        state.filter = filter;
        syncActiveStates();
        render();
        return;
      }

      // tab
      const tab = target.getAttribute('data-tab');
      if (tab) {
        state.tab = tab;
        document.querySelectorAll('.tab').forEach((node) => node.classList.toggle('active', node.getAttribute('data-tab') === tab));
        applyTab();
        return;
      }

      // view mode
      const viewMode = target.getAttribute('data-viewmode');
      if (viewMode) {
        state.viewMode = viewMode;
        document.querySelectorAll('#view-switch .chip').forEach((node) => node.classList.toggle('active', node.getAttribute('data-viewmode') === viewMode));
        applyTab();
        return;
      }

      // toolbar / nav actions
      const action = target.getAttribute('data-action');
      if (!action) return;

      switch (action) {
        case 'add':
          openModal('add-modal');
          break;
        case 'add-confirm':
          addDownload();
          break;
        case 'settings':
          loadSettings();
          openModal('settings-modal');
          break;
        case 'settings-save':
          saveSettings();
          break;
        case 'sniffer':
          api.sniffer.open();
          break;
        case 'open-site':
          api.shell.openExternal('https://zyzoom.net');
          break;
        case 'toggle-nav':
          document.querySelector('.app-shell').classList.toggle('nav-collapsed');
          break;
        case 'bulk-start':
          bulk('start');
          break;
        case 'bulk-pause':
          bulk('pause');
          break;
        case 'bulk-resume':
          bulk('resume');
          break;
        case 'bulk-stop':
          bulk('stop');
          break;
        case 'bulk-delete': {
          const ids = Array.from(state.selected);
          ids.forEach((id) => api.downloads.remove(id));
          state.selected.clear();
          break;
        }
        case 'pg-pause':
          if (state.progressId) performAction(state.progressId, 'pause');
          break;
        case 'pg-cancel':
          if (state.progressId) api.downloads.remove(state.progressId);
          closeModal('progress-modal');
          state.progressId = null;
          break;
        case 'pg-limiter':
          openModal('settings-modal');
          break;
        case 'captures-clear':
          api.captures.clear();
          renderCaptures(state.captures);
          break;
        case 'capture-download': {
          const id = target.getAttribute('data-id');
          api.captures.download(id);
          break;
        }
        case 'install-host':
          installNativeHost();
          break;
        case 'open-extension':
          api.bridge.openExtension();
          break;
        default:
          break;
      }
    });

    document.getElementById('auto-resume').addEventListener('change', (event) => {
      state.settings.resume = event.target.checked;
    });
    document.getElementById('auto-speed').addEventListener('change', (event) => {
      state.settings.speed = event.target.checked;
    });

    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        document.querySelectorAll('.overlay.open').forEach((node) => node.classList.remove('open'));
      }
    });
  }

  function syncActiveStates() {
    document.querySelectorAll('#nav .nav-item[data-status]').forEach((node) => {
      node.classList.toggle('active', node.getAttribute('data-status') === state.filter);
    });
    document.querySelectorAll('#chips .chip').forEach((node) => {
      node.classList.toggle('active', node.getAttribute('data-filter') === state.filter);
    });
  }

  function applyTab() {
    const isDownloads = state.tab === 'downloads';
    el.cardsView.classList.toggle('hidden', !(isDownloads && state.viewMode === 'cards'));
    el.tableView.classList.toggle('hidden', !(isDownloads && state.viewMode === 'table'));
    document.getElementById('browser-list').classList.toggle('hidden', state.tab !== 'browser-list');
    document.getElementById('embedded-browser').classList.toggle('hidden', state.tab !== 'embedded-browser');
    document.getElementById('website').classList.toggle('hidden', state.tab !== 'website');
    document.getElementById('chart-panel').classList.toggle('hidden', !isDownloads);
    document.getElementById('filter-row').classList.toggle('hidden', !isDownloads);

    const titles = {
      downloads: ['لوحة التحكم الرئيسية', 'إدارة التنزيلات والنطاقات'],
      'browser-list': ['قائمة المتصفح', 'التنزيلات التي التقطها المتصفح'],
      'embedded-browser': ['المتصفح المدمج', 'تصفح وتصفّح داخل التطبيق'],
      website: ['الموقع الرسمي', 'Zyzoom'],
    };
    const [title, subtitle] = titles[state.tab] || titles.downloads;
    el.viewTitle.textContent = title;
    el.viewSubtitle.textContent = subtitle;
  }

  /* ----------------------------------------------------------------- init */

  async function init() {
    loadSettings();
    document.getElementById('auto-resume').checked = state.settings.resume;
    document.getElementById('auto-speed').checked = state.settings.speed;

    state.chart = new SpeedChart(document.getElementById('speed-chart'), document.getElementById('chart-axis'));
    bindEvents();
    applyTab();

    state.downloads = await api.downloads.list();
    state.captures = await api.captures.list();
    render();
    renderCaptures(state.captures);
    loadBridgeInfo();

    api.downloads.onChanged((list) => {
      state.downloads = list;
      render();
    });

    api.captures.onChanged((list) => {
      renderCaptures(list);
    });

    window.setInterval(updateChart, 2000);
  }

  init();
})();
