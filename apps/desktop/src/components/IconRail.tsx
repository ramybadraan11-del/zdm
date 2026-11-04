//! ZDM Icon Rail + Status Bar
//! Developed by Ramy Badraan
//!
//! Design guide §03:
//!   • Icon rail — 56px wide, icons only, tooltips, active item is a Volt icon on
//!     a 14% Volt background. No tree sidebar, no category folders.
//!   • Status bar — 32px, a global 120×20 pulse line, total speed in cyan, then
//!     "N active · M queued", free disk space and bytes downloaded today.

import React, { useEffect, useRef } from 'react';
import {
  Download,
  Activity,
  ListOrdered,
  CheckCircle2,
  Settings as SettingsIcon,
  Info,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { formatBytes, formatSpeed } from '../utils/format';

export type RailTab = 'all' | 'active' | 'completed' | 'queue' | 'settings';

interface RailProps {
  current: RailTab;
  onSelect: (tab: RailTab) => void;
  counts: { all: number; active: number; queued: number; completed: number };
  onOpenAbout: () => void;
}

const ITEMS: { id: RailTab; icon: React.ElementType; key: string; countKey?: keyof RailProps['counts'] }[] = [
  { id: 'all', icon: Download, key: 'railAll', countKey: 'all' },
  { id: 'active', icon: Activity, key: 'railActive', countKey: 'active' },
  { id: 'completed', icon: CheckCircle2, key: 'railDone', countKey: 'completed' },
  { id: 'queue', icon: ListOrdered, key: 'railQueue', countKey: 'queued' },
];

export const IconRail: React.FC<RailProps> = ({ current, onSelect, counts, onOpenAbout }) => {
  const { t } = useI18n();

  return (
    <nav className="rail" aria-label={t('railAll')}>
      <div className="rail-group">
        {ITEMS.map((item) => {
          const Icon = item.icon;
          const active = current === item.id;
          const count = item.countKey ? counts[item.countKey] : 0;
          return (
            <button
              key={item.id}
              type="button"
              className="rail-btn"
              data-active={active}
              onClick={() => onSelect(item.id)}
              title={t(item.key as never)}
              aria-label={t(item.key as never)}
              aria-pressed={active}
            >
              <Icon size={17} strokeWidth={1.5} />
              {count > 0 && item.id === 'active' && <span className="rail-dot" />}
            </button>
          );
        })}
      </div>

      <div className="rail-group rail-bottom">
        <button
          type="button"
          className="rail-btn"
          data-active={current === 'settings'}
          onClick={() => onSelect('settings')}
          title={t('navSettings')}
          aria-label={t('navSettings')}
          aria-pressed={current === 'settings'}
        >
          <SettingsIcon size={17} strokeWidth={1.5} />
        </button>
        <button
          type="button"
          className="rail-btn"
          onClick={onOpenAbout}
          title={t('navAbout')}
          aria-label={t('navAbout')}
        >
          <Info size={17} strokeWidth={1.5} />
        </button>
      </div>
    </nav>
  );
};

interface StatusBarProps {
  speedBps: number;
  activeCount: number;
  queuedCount: number;
  diskFree: number;
  todayBytes?: number;
  /** Rolling speed samples; the guide asks for a 60s window. */
  history: number[];
}

/** 120×20 sparkline drawn on a canvas — cheap, and correct on WebKitGTK. */
const PulseLine: React.FC<{ values: number[] }> = ({ values }) => {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const width = 120;
    const height = 20;
    canvas.width = width * dpr;
    canvas.height = height * dpr;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const points = values.length > 1 ? values : [0, 0];
    const max = Math.max(...points, 1);
    const step = width / Math.max(1, points.length - 1);

    // Resolve the theme colour from CSS so the line follows light/dark.
    const stroke =
      getComputedStyle(document.documentElement).getPropertyValue('--accent-2').trim() || '#35d8ff';

    ctx.beginPath();
    points.forEach((value, index) => {
      const x = index * step;
      const y = height - 1 - (value / max) * (height - 3);
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1.5;
    ctx.lineJoin = 'round';
    ctx.stroke();

    // Soft fill under the line: reads as "live" without a glow.
    ctx.lineTo(width, height);
    ctx.lineTo(0, height);
    ctx.closePath();
    ctx.globalAlpha = 0.14;
    ctx.fillStyle = stroke;
    ctx.fill();
    ctx.globalAlpha = 1;
  }, [values]);

  return <canvas ref={ref} className="status-pulse" width={120} height={20} aria-hidden="true" />;
};

export const StatusBar: React.FC<StatusBarProps> = ({
  speedBps,
  activeCount,
  queuedCount,
  diskFree,
  todayBytes = 0,
  history,
}) => {
  const { t } = useI18n();

  return (
    <footer className="status">
      <PulseLine values={history} />

      <span className="status-speed mono" dir="ltr">
        {formatSpeed(speedBps)}
      </span>

      <span className="status-sep" aria-hidden="true" />

      <span className="status-item">
        {activeCount} {t('statusActive')} · {queuedCount} {t('statusQueued')}
      </span>

      <span className="status-spacer" />

      {todayBytes > 0 && (
        <span className="status-item">
          {t('statusToday')} {formatBytes(todayBytes)}
        </span>
      )}
      <span className="status-item">
        {t('statusFree')} {formatBytes(diskFree)}
      </span>
    </footer>
  );
};
