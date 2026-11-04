//! ZDM Stats Strip
//! Developed by Ramy Badraan
//!
//! Four honest numbers. No gauges, no sparklines: the value is the hierarchy, and
//! the label is a quiet caption above it.

import React, { memo } from 'react';
import { ArrowDownToLine, Activity, CheckCircle2, HardDrive } from 'lucide-react';
import { formatBytes, formatSpeed } from '../utils/format';
import { useI18n } from '../i18n';

interface Props {
  totalSpeed: number;
  activeCount: number;
  queuedCount: number;
  completedCount: number;
  diskFree: number;
}

const Stat: React.FC<{
  icon: React.ReactNode;
  label: string;
  value: string;
  hint?: string;
  tone: string;
  live?: boolean;
}> = ({ icon, label, value, hint, tone, live }) => (
  <div className="panel edge-light px-3 py-2.5 flex items-center gap-2.5 min-w-0">
    <div className={`w-8 h-8 rounded-lg grid place-items-center shrink-0 ${tone}`}>{icon}</div>
    <div className="min-w-0">
      <div className="text-[13px] font-bold uppercase tracking-[0.1em] text-inkMuted truncate">
        {label}
      </div>
      <div className="flex items-baseline gap-1 min-w-0">
        <span
          className={`mono text-[13.5px] font-extrabold text-ink leading-tight truncate ${
            live ? 'tabular-nums' : ''
          }`}
          dir="ltr"
        >
          {value}
        </span>
        {hint ? (
          <span className="text-[11px] font-semibold text-inkMuted shrink-0">{hint}</span>
        ) : null}
      </div>
    </div>
  </div>
);

export const StatsStrip: React.FC<Props> = memo(
  ({ totalSpeed, activeCount, queuedCount, completedCount, diskFree }) => {
    const { t } = useI18n();

    return (
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-2">
        <Stat
          icon={<Activity size={14} />}
          label={t('speed')}
          value={totalSpeed > 0 ? formatSpeed(totalSpeed) : '0 B/s'}
          tone={
            totalSpeed > 0
              ? 'bg-accent2/12 text-accent2'
              : 'bg-panelHover text-inkMuted'
          }
          live
        />
        <Stat
          icon={<ArrowDownToLine size={14} />}
          label={t('navActive')}
          value={String(activeCount)}
          hint={queuedCount > 0 ? `+${queuedCount}` : undefined}
          tone="bg-accent3/12 text-accent3"
        />
        <Stat
          icon={<CheckCircle2 size={14} />}
          label={t('navCompleted')}
          value={String(completedCount)}
          tone="bg-emerald-400/12 text-emerald-400"
        />
        <Stat
          icon={<HardDrive size={14} />}
          label={t('freeSpace')}
          value={diskFree > 0 ? formatBytes(diskFree, 0) : '—'}
          tone="bg-inkMuted/10 text-inkSoft"
        />
      </div>
    );
  }
);
