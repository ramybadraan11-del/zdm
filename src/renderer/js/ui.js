/* ==========================================================================
   Shared UI helpers (theme widgets, formatting, chart)
   ========================================================================== */
(function (global) {
  'use strict';

  const KB = 1024;
  const MB = KB * 1024;
  const GB = MB * 1024;
  const TB = GB * 1024;

  function esc(value) {
    return String(value == null ? '' : value)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  function bytes(value) {
    if (!Number.isFinite(value) || value <= 0) return '0 بايت';
    const units = ['بايت', 'ك.ب', 'م.ب', 'ج.ب', 'ت.ب'];
    let n = value;
    let i = 0;
    while (n >= KB && i < units.length - 1) {
      n /= KB;
      i += 1;
    }
    const decimals = n >= 100 || i === 0 ? 0 : n >= 10 ? 1 : 2;
    return `${n.toFixed(decimals)} ${units[i]}`;
  }

  function speed(value) {
    if (!Number.isFinite(value) || value <= 0) return '0 ب/ث';
    return `${bytes(value)}/ث`;
  }

  function duration(seconds) {
    if (!Number.isFinite(seconds) || seconds < 0) return '--:--:--';
    const total = Math.floor(seconds);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const pad = (n) => String(n).padStart(2, '0');
    return `${pad(h)}:${pad(m)}:${pad(s)}`;
  }

  const STATUS = {
    downloading: { label: 'قيد التنزيل', cls: 'downloading' },
    paused: { label: 'متوقف مؤقتاً', cls: 'paused' },
    queued: { label: 'في الطابور', cls: 'queued' },
    scheduled: { label: 'مجدول', cls: 'queued' },
    completed: { label: 'مكتمل', cls: 'completed' },
    failed: { label: 'فشل', cls: 'failed' },
  };

  function statusInfo(status) {
    return STATUS[status] || { label: status || '—', cls: 'queued' };
  }

  function ringSvg(percent, cls) {
    const p = Math.max(0, Math.min(100, Number(percent) || 0));
    const r = 20;
    const c = 2 * Math.PI * r;
    const offset = c - (c * p) / 100;
    return `<svg class="ring ${cls || ''}" viewBox="0 0 48 48">
      <circle class="track" cx="24" cy="24" r="${r}" />
      <circle class="fill" cx="24" cy="24" r="${r}"
        stroke-dasharray="${c.toFixed(2)}" stroke-dashoffset="${offset.toFixed(2)}" />
    </svg>`;
  }

  function icon(name, size) {
    return `<svg width="${size || 15}" height="${size || 15}"><use href="#i-${name}" /></svg>`;
  }

  /* ------------------------------------------------------------- chart */

  class SpeedChart {
    constructor(canvas, axisEl) {
      this.canvas = canvas;
      this.axisEl = axisEl;
      this.ctx = canvas.getContext('2d');
      this.real = [];
      this.expected = [];
      this.max = 0;
      this.resize();
      window.addEventListener('resize', () => this.resize());
    }

    resize() {
      const dpr = window.devicePixelRatio || 1;
      const rect = this.canvas.getBoundingClientRect();
      this.canvas.width = Math.max(1, Math.floor(rect.width * dpr));
      this.canvas.height = Math.max(1, Math.floor(rect.height * dpr));
      this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      this.width = rect.width;
      this.height = rect.height;
      this.render();
    }

    push(realValue, expectedValue) {
      this.real.push(realValue);
      this.expected.push(expectedValue);
      const maxPoints = Math.max(24, Math.floor(this.width / 8));
      while (this.real.length > maxPoints) this.real.shift();
      while (this.expected.length > maxPoints) this.expected.shift();
      this.render();
    }

    render() {
      const ctx = this.ctx;
      const w = this.width;
      const h = this.height;
      const padTop = 10;
      const padBottom = 8;
      const plotH = h - padTop - padBottom;

      ctx.clearRect(0, 0, w, h);

      const peak = Math.max(
        1,
        ...this.real,
        ...this.expected
      );
      const niceMax = Math.ceil(peak / (10 * MB)) * 10 * MB || 10 * MB;

      // grid
      ctx.strokeStyle = 'rgba(29, 48, 80, 0.85)';
      ctx.lineWidth = 1;
      for (let i = 0; i <= 4; i += 1) {
        const y = padTop + (plotH / 4) * i;
        ctx.beginPath();
        ctx.moveTo(0, y + 0.5);
        ctx.lineTo(w, y + 0.5);
        ctx.stroke();
      }

      // vertical grid
      ctx.strokeStyle = 'rgba(29, 48, 80, 0.5)';
      for (let i = 0; i <= 6; i += 1) {
        const x = (w / 6) * i;
        ctx.beginPath();
        ctx.moveTo(x + 0.5, padTop);
        ctx.lineTo(x + 0.5, padTop + plotH);
        ctx.stroke();
      }

      if (this.real.length > 1) {
        this.drawSeries(this.real, niceMax, '#22d3ee', true);
        this.drawSeries(this.expected, niceMax, '#f5a524', false);
      }

      if (this.axisEl) {
        const labels = [];
        for (let i = 0; i <= 4; i += 1) {
          const value = niceMax - (niceMax / 4) * i;
          labels.push(`<span>${bytes(value)}</span>`);
        }
        this.axisEl.innerHTML = labels.join('');
      }
    }

    drawSeries(points, max, color, fill) {
      const ctx = this.ctx;
      const w = this.width;
      const h = this.height;
      const padTop = 10;
      const padBottom = 8;
      const plotH = h - padTop - padBottom;
      const step = w / Math.max(1, points.length - 1);
      const yFor = (v) => padTop + plotH - (v / max) * plotH;

      if (fill) {
        const grad = ctx.createLinearGradient(0, padTop, 0, padTop + plotH);
        grad.addColorStop(0, 'rgba(34, 211, 238, 0.42)');
        grad.addColorStop(1, 'rgba(34, 211, 238, 0.01)');
        ctx.beginPath();
        ctx.moveTo(0, padTop + plotH);
        points.forEach((v, i) => ctx.lineTo(i * step, yFor(v)));
        ctx.lineTo((points.length - 1) * step, padTop + plotH);
        ctx.closePath();
        ctx.fillStyle = grad;
        ctx.fill();
      }

      ctx.beginPath();
      points.forEach((v, i) => {
        const x = i * step;
        const y = yFor(v);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.shadowColor = color;
      ctx.shadowBlur = 8;
      ctx.stroke();
      ctx.shadowBlur = 0;
    }
  }

  global.UI = {
    esc,
    bytes,
    speed,
    duration,
    statusInfo,
    ringSvg,
    icon,
    SpeedChart,
    KB,
    MB,
    GB,
    TB,
  };
})(window);
