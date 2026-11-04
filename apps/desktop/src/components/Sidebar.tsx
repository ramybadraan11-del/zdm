//! ZDM Sidebar
//! Developed by Ramy Badraan
//!
//! Slim rail, one accent per state, and a live throughput readout at the bottom.
//! The nav item's active marker is a small vertical pill that grows in — cheap to
//! animate (transform only) and legible in both themes.

import React from 'react';
import {
  Download,
  PlayCircle,
  CheckCircle2,
  Film,
  Settings,
  Info,
  HardDrive,
  Activity,
} from 'lucide-react';
import { LogoWordmark } from './Logo';
import { useI18n } from '../i18n';
import { Key } from '../i18n/dict';
import { formatBytes, formatSpeed } from '../utils/format';

export type NavTab = 'all' | 'active' | 'completed' | 'video' | 'settings';

interface SidebarProps {
  currentTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  activeCount: number;
  completedCount: number;
  totalSpeed: number;
  diskFree: number;
  onOpenAbout: () => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentTab,
  onSelectTab,
  activeCount,
  completedCount,
  totalSpeed,
  diskFree,
  onOpenAbout,
}) => {
  const { t } = useI18n();

  const items: { id: NavTab; labelKey: Key; icon: React.ElementType; badge?: number }[] = [
    { id: 'all', labelKey: 'navAll', icon: Download },
    { id: 'active', labelKey: 'navActive', icon: PlayCircle, badge: activeCount || undefined },
    {
      id: 'completed',
      labelKey: 'navCompleted',
      icon: CheckCircle2,
      badge: completedCount || undefined,
    },
    { id: 'video', labelKey: 'navVideo', icon: Film },
    { id: 'settings', labelKey: 'navSettings', icon: Settings },
  ];

  return (
    <aside className="w-[228px] shrink-0 h-full flex flex-col bg-canvas border-e border-line">
      <div className="px-4 pt-4 pb-3">
        <LogoWordmark size={34} subtitle={t('tagline')} />
      </div>

      <nav className="px-2 flex-1 overflow-y-auto" aria-label={t('navAll')}>
        {items.map((item) => {
          const Icon = item.icon;
          const active = currentTab === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelectTab(item.id)}
              aria-current={active ? 'page' : undefined}
              className={`group relative w-full flex items-center gap-2.5 ps-3 pe-2 h-[38px] rounded-lg mb-0.5 text-[12.5px] font-bold transition-colors duration-150 ${
                active ? 'text-ink' : 'text-inkMuted hover:text-inkSoft'
              }`}
            >
              {/* Hover wash sits behind the label, not on it. */}
              <span
                className={`absolute inset-0 rounded-lg transition-opacity duration-150 ${
                  active ? 'opacity-100 bg-panelHover' : 'opacity-0 group-hover:opacity-100 bg-panelHover/60'
                }`}
              />
              {/* Active marker: scales in from the middle. */}
              <span
                className={`absolute start-0 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-full bg-accent2 transition-transform duration-200 ease-smooth ${
                  active ? 'scale-y-100' : 'scale-y-0'
                }`}
              />

              <Icon
                size={16}
                className={`relative shrink-0 transition-colors ${
                  active ? 'text-accent2' : 'text-inkMuted group-hover:text-inkSoft'
                }`}
              />
              <span className="relative flex-1 text-start truncate">{t(item.labelKey)}</span>
              {item.badge ? (
                <span className="relative mono text-[12px] font-bold px-1.5 py-0.5 rounded-md bg-panel text-inkMuted border border-line">
                  {item.badge}
                </span>
              ) : null}
            </button>
          );
        })}
      </nav>

      {/* Live engine readout */}
      <div className="px-2.5 pb-2.5 space-y-2">
        <div className="panel-inset px-2.5 py-2.5 space-y-2">
          <div className="flex items-center gap-2">
            <Activity
              size={13}
              className={totalSpeed > 0 ? 'text-accent2' : 'text-inkMuted'}
            />
            <span className="text-[11px] font-bold uppercase tracking-[0.09em] text-inkMuted flex-1 truncate">
              {t('speed')}
            </span>
            <span
              className={`mono text-[13.5px] font-bold ${
                totalSpeed > 0 ? 'text-accent2' : 'text-inkMuted'
              }`}
              dir="ltr"
            >
              {totalSpeed > 0 ? formatSpeed(totalSpeed) : '—'}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <HardDrive size={13} className="text-inkMuted shrink-0" />
            <span className="text-[11px] font-bold uppercase tracking-[0.09em] text-inkMuted flex-1 truncate">
              {t('freeSpace')}
            </span>
            <span className="mono text-[12.5px] font-semibold text-inkSoft" dir="ltr">
              {diskFree > 0 ? formatBytes(diskFree, 0) : '—'}
            </span>
          </div>
        </div>

        <button
          type="button"
          onClick={onOpenAbout}
          className="w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-[13px] text-inkMuted hover:text-inkSoft hover:bg-panelHover/60 transition-colors"
        >
          <Info size={12.5} />
          <span className="truncate">{t('navAbout')}</span>
        </button>
      </div>
    </aside>
  );
};
