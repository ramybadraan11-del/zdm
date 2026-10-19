'use strict';

/**
 * محرك التنزيلات.
 *
 * يربط بين طبقة البيانات (الواجهة) والمُنزِّل الحقيقي.
 * السجلات المُضافة حديثًا تُنزَّل فعليًا عبر HTTP مع تقسيم الأجزاء،
 * بينما تبقى السجلات التجريبية (البذور) محاكاة حيّة لعرض الحالة.
 */

const { EventEmitter } = require('events');
const { SegmentDownload, expandHome, fileNameFromUrl } = require('./downloader');

const MINUTE = 60;
const HOUR = 60 * MINUTE;

function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes < 0) return '--';
  const units = ['بايت', 'ك.ب', 'م.ب', 'ج.ب', 'ت.ب'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const decimals = value >= 100 || unit === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(decimals)} ${units[unit]}`;
}

function formatDuration(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return '--:--:--';
  const total = Math.floor(seconds);
  const h = Math.floor(total / HOUR);
  const m = Math.floor((total % HOUR) / MINUTE);
  const s = total % MINUTE;
  const pad = (n) => String(n).padStart(2, '0');
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}

const SEED_DOWNLOADS = [
  {
    id: 'dl-1001',
    name: 'Setup_v4.2.exe',
    url: 'https://downloads.zyzoom.net/releases/zyzoom/Setup_v4.2.exe',
    size: 45.2 * 1024 * 1024 * 1024,
    downloaded: 12.4 * 1024 * 1024 * 1024,
    speed: 45.2 * 1024 * 1024,
    status: 'downloading',
    browser: 'Chrome',
    description: 'حزمة التثبيت الرسمية لـ Zyzoom 4.2',
    addedAt: '2026-10-19T09:14:00',
    connections: 16,
    resume: true,
    type: 'تطبيق',
    simulated: true,
  },
  {
    id: 'dl-1002',
    name: 'Game_Package_Data.zip',
    url: 'https://cdn.zyzoom.net/games/package-data.zip',
    size: 12.4 * 1024 * 1024 * 1024,
    downloaded: 9.6 * 1024 * 1024 * 1024,
    speed: 84.5 * 1024 * 1024,
    status: 'downloading',
    browser: 'Chrome',
    description: 'ملفات حزمة اللعبة - نسخة كاملة',
    addedAt: '2026-10-19T08:02:00',
    connections: 32,
    resume: true,
    type: 'أرشيف',
    simulated: true,
  },
  {
    id: 'dl-1003',
    name: 'video_capture.mp4',
    url: 'https://media.zyzoom.net/captures/video_capture.mp4',
    size: 55.6 * 1024 * 1024,
    downloaded: 39.6 * 1024 * 1024,
    speed: 45.2 * 1024 * 1024,
    status: 'paused',
    browser: 'Firefox',
    description: 'تسجيل شاشة - جودة 1080p',
    addedAt: '2026-10-18T21:40:00',
    connections: 8,
    resume: true,
    type: 'فيديو',
    simulated: true,
  },
  {
    id: 'dl-1004',
    name: 'video_capture_4k.mp4',
    url: 'https://media.zyzoom.net/captures/video_capture_4k.mp4',
    size: 28.5 * 1024 * 1024,
    downloaded: 38.5 * 1024 * 1024,
    speed: 44.3 * 1024 * 1024,
    status: 'downloading',
    browser: 'Edge',
    description: 'تسجيل شاشة - دقة 4K',
    addedAt: '2026-10-19T10:26:00',
    connections: 8,
    resume: true,
    type: 'فيديو',
    simulated: true,
  },
  {
    id: 'dl-1005',
    name: 'setup_v4.2.mp4',
    url: 'https://media.zyzoom.net/tutorials/setup_v4.2.mp4',
    size: 123.8 * 1024,
    downloaded: 0,
    speed: 0,
    status: 'scheduled',
    browser: 'Chrome',
    description: 'فيديو شرح التثبيت - مجدول',
    addedAt: '2026-10-19T11:02:00',
    connections: 1,
    resume: true,
    type: 'فيديو',
    simulated: true,
  },
  {
    id: 'dl-1006',
    name: 'zyzoom-manual.pdf',
    url: 'https://zyzoom.net/docs/zyzoom-manual.pdf',
    size: 8.7 * 1024 * 1024,
    downloaded: 8.7 * 1024 * 1024,
    speed: 0,
    status: 'completed',
    browser: 'Chrome',
    description: 'دليل المستخدم - PDF',
    addedAt: '2026-10-17T15:22:00',
    finishedAt: '2026-10-17T15:24:10',
    connections: 4,
    resume: true,
    type: 'مستند',
    simulated: true,
  },
  {
    id: 'dl-1007',
    name: 'theme-pack-dark.zip',
    url: 'https://cdn.zyzoom.net/themes/dark.zip',
    size: 320 * 1024 * 1024,
    downloaded: 41 * 1024 * 1024,
    speed: 0,
    status: 'failed',
    browser: 'Chrome',
    description: 'حزمة الثيم الداكن - فشل الاتصال',
    addedAt: '2026-10-18T07:11:00',
    connections: 2,
    resume: true,
    type: 'أرشيف',
    simulated: true,
  },
];

function clone(record) {
  return JSON.parse(JSON.stringify(record));
}

class DownloadEngine extends EventEmitter {
  constructor(options = {}) {
    super();
    this.downloads = SEED_DOWNLOADS.map(clone);
    this.captures = [];
    this.runtimes = new Map();
    this.downloadDir = options.dir || '~/Downloads';
    this.maxConnections = options.maxConnections || 16;
    this.timer = null;
    this.throttle = null;
  }

  /* ------------------------------------------------------- الالتقاط */

  listCaptures() {
    return this.captures.map((item) => clone(item));
  }

  /**
   * استقبال عنصر وسائط من امتداد المتصفح.
   * @returns {number} عدد العناصر التي أُضيفت فعليًا
   */
  addCapture(item) {
    if (!item || !item.url) return 0;
    const exists = this.captures.some((entry) => entry.url === item.url);
    if (exists) return 0;
    const record = {
      id: `cap-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      url: item.url,
      kind: item.kind || 'element',
      mime: item.mime || '',
      title: item.title || '',
      pageUrl: item.pageUrl || '',
      detectedAt: item.detectedAt || Date.now(),
      status: 'idle',
    };
    this.captures.unshift(record);
    if (this.captures.length > 300) this.captures.length = 300;
    this.emit('captures', this.listCaptures());
    this.emit('changed', this.list());
    return 1;
  }

  clearCaptures() {
    this.captures = [];
    this.emit('captures', this.listCaptures());
    this.emit('changed', this.list());
  }

  setOptions(options = {}) {
    if (options.dir) this.downloadDir = options.dir;
    if (options.maxConnections) this.maxConnections = options.maxConnections;
  }

  list() {
    return this.downloads.map((item) => {
      const view = clone(item);
      view.sizeText = formatBytes(view.size);
      view.downloadedText = formatBytes(view.downloaded);
      view.speedText = view.speed > 0 ? `${formatBytes(view.speed)}/ث` : '--';
      view.percent = view.size > 0 ? Math.min(100, (view.downloaded / view.size) * 100) : 0;
      view.timeLeftText = formatDuration(
        view.speed > 0 ? (view.size - view.downloaded) / view.speed : Number.POSITIVE_INFINITY,
      );
      view.addedAtText = item.addedAt ? item.addedAt.replace('T', ' ').slice(0, 16) : '--';
      view.finishedAtText = item.finishedAt ? item.finishedAt.replace('T', ' ').slice(0, 16) : '--';
      view.real = !item.simulated;
      return view;
    });
  }

  find(id) {
    return this.downloads.find((item) => item.id === id);
  }

  update(id, patch) {
    const item = this.find(id);
    if (!item) return null;
    Object.assign(item, patch);
    this.emit('changed', this.list());
    return item;
  }

  /**
   * إضافة مهمة. إذا كان الرابط http(s) يبدأ تنزيلًا حقيقيًا فورًا،
   * وإلا تُبقى المهمة في الطابور.
   */
  add(payload = {}) {
    const url = String(payload.url || '').trim();
    const record = {
      id: `dl-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      name: payload.name || fileNameFromUrl(url) || 'download.bin',
      url,
      size: Number(payload.size) || 0,
      downloaded: 0,
      speed: 0,
      status: 'queued',
      browser: payload.browser || 'Zyzoom',
      description: payload.description || '',
      addedAt: new Date().toISOString(),
      connections: Number(payload.connections) || Math.min(8, this.maxConnections),
      resume: true,
      type: payload.type || 'ملف',
      simulated: false,
      error: null,
    };
    this.downloads.unshift(record);
    this.emit('changed', this.list());

    if (/^https?:\/\//i.test(url)) this._startReal(record);
    return record;
  }

  _startReal(record) {
    const downloader = new SegmentDownload({
      url: record.url,
      dir: this.downloadDir,
      name: record.name,
      connections: record.connections || 8,
      onStatus: (status, extra) => {
        if (status === 'failed') {
          this.update(record.id, { status: 'failed', error: extra.error || null, speed: 0 });
        } else if (status === 'completed') {
          this.update(record.id, { status, finishedAt: new Date().toISOString(), speed: 0 });
        } else {
          this.update(record.id, { status });
        }
      },
      onProgress: (progress) => {
        this.update(record.id, {
          downloaded: progress.downloaded,
          size: progress.totalSize || record.size,
          speed: progress.speed,
          connections: progress.connections,
        });
      },
      onError: (err) => {
        this.update(record.id, { status: 'failed', error: err.message, speed: 0 });
      },
    });

    this.runtimes.set(record.id, downloader);
    this.update(record.id, { status: 'downloading' });
    downloader.start().catch(() => {});
  }

  remove(id) {
    const runtime = this.runtimes.get(id);
    if (runtime) {
      runtime.remove().catch(() => {});
      this.runtimes.delete(id);
    }
    this.downloads = this.downloads.filter((item) => item.id !== id);
    this.emit('changed', this.list());
  }

  setStatus(id, action) {
    const item = this.find(id);
    if (!item) return null;
    const runtime = this.runtimes.get(id);

    if (runtime) {
      if (action === 'pause') runtime.pause().catch(() => {});
      else if (action === 'start') runtime.start().catch(() => {});
      else if (action === 'stop') {
        runtime.pause().catch(() => {});
        this.update(id, { status: 'queued', speed: 0 });
      }
      return item;
    }

    const map = {
      start: 'downloading',
      pause: 'paused',
      stop: 'queued',
    };
    return this.update(id, { status: map[action] || action });
  }

  start() {
    if (this.timer) return;
    this.timer = setInterval(() => {
      let mutated = false;
      for (const item of this.downloads) {
        if (!item.simulated) continue;
        if (item.status !== 'downloading') continue;
        const jitter = 0.75 + Math.random() * 0.5;
        item.speed = Math.max(64 * 1024, item.speed * jitter);
        item.downloaded = Math.min(item.size, item.downloaded + item.speed * 1.2);
        if (item.downloaded >= item.size) {
          item.speed = 0;
          item.status = 'completed';
          item.finishedAt = new Date().toISOString();
        }
        mutated = true;
      }
      if (mutated) this.emit('changed', this.list());
    }, 1200);
    if (this.timer.unref) this.timer.unref();
  }

  stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    for (const runtime of this.runtimes.values()) runtime.cancel().catch(() => {});
  }
}

module.exports = { DownloadEngine, formatBytes, formatDuration, expandHome };
