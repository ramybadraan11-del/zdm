//! ZDM Download Row
//! Developed by Ramy Badraan
//!
//! One row per download, built for a long list: memoised, no blur, no permanent
//! animation. The signature element is the segment ribbon under the title — one
//! column per connection so parallelism is visible at a glance.

import React, { memo } from 'react';
import {
  Play,
  Pause,
  FolderOpen,
  Trash2,
  Film,
  Music,
  Archive,
  FileText,
  Package,
  Layers,
  CheckCircle2,
  AlertTriangle,
  Clock,
  RotateCw,
  Maximize2,
} from 'lucide-react';
import { DownloadItem } from '../types';
import { formatBytes, formatSpeed, formatDuration, hostOf, categoryAccent } from '../utils/format';
import { useI18n } from '../i18n';
import { Key } from '../i18n/dict';

interface Props {
  item: DownloadItem;
  index?: number;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onDelete: (id: string) => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
  onOpenTransfer?: (id: string) => void;
}

const STATUS_KEY: Record<string, Key> = {
  queued: 'statusQueued',
  downloading: 'statusDownloading',
  paused: 'statusPaused',
  completed: 'statusCompleted',
  error: 'statusError',
  cancelled: 'statusCancelled',
};

function CategoryIcon({ category }: { category: string }) {
  const size = 17;
  switch (category.toLowerCase()) {
    case 'video':
      return <Film size={size} />;
    case 'audio':
      return <Music size={size} />;
    case 'compressed':
      return <Archive size={size} />;
    case 'documents':
      return <FileText size={size} />;
    case 'programs':
      return <Package size={size} />;
    default:
      return <Layers size={size} />;
  }
}

const DownloadRowBase: React.FC<Props> = ({
  item,
  index = 0,
  onPause,
  onResume,
  onDelete,
  onOpenFile,
  onOpenFolder,
  onOpenTransfer,
}) => {
  const { t } = useI18n();
  const accent = categoryAccent(item.category);

  const isActive = item.status === 'downloading';
  const isPaused = item.status === 'paused';
  const isQueued = item.status === 'queued';
  const isDone = item.status === 'completed';
  const isError = item.status === 'error';

  const percent = isDone ? 100 : Math.min(100, Math.max(0, item.progress_percent));
  const hasSegments = item.segments.length > 1;
  const sizeLabel = item.total_size > 0 ? formatBytes(item.total_size) : '—';

  return (
    <article
      className="group stagger card px-3.5 py-3"
      data-state={item.status}
      style={{ ['--i' as string]: index }}
    >
      <div className="flex items-start gap-3">
        {/* Category chip */}
        <div
          className={`w-9 h-9 shrink-0 rounded-lg grid place-items-center border ${accent.ring} ${accent.text}`}
        >
          <CategoryIcon category={item.category} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => isDone && onOpenFile(item.save_path)}
              disabled={!isDone}
              className={`text-[12.5px] font-bold truncate text-start always-ltr min-w-0 ${
                isDone ? 'text-ink hover:text-accent2 cursor-pointer' : 'text-ink cursor-default'
              }`}
              title={item.filename}
            >
              {item.filename}
            </button>

            <span
              className={`shrink-0 text-[13px] font-extrabold uppercase tracking-[0.07em] px-1.5 py-[3px] rounded-md border ${
                isDone
                  ? 'border-emerald-400/25 text-emerald-400 bg-emerald-400/10'
                  : isError
                  ? 'border-danger/25 text-danger bg-danger/10'
                  : isPaused
                  ? 'border-warning/25 text-warning bg-warning/10'
                  : isQueued
                  ? 'border-line text-inkMuted bg-panelHover/60'
                  : 'border-accent2/25 text-accent2 bg-accent2/10'
              }`}
            >
              {t(STATUS_KEY[item.status] ?? 'statusQueued')}
            </span>
          </div>

          <div className="flex items-center gap-1.5 mt-1 text-[12px] text-inkMuted min-w-0">
            <span className="mono shrink-0 text-inkSoft" dir="ltr">
              {sizeLabel}
            </span>
            <span className="opacity-30">·</span>
            <span className="truncate always-ltr">{hostOf(item.url)}</span>
            {item.connections > 1 ? (
              <>
                <span className="opacity-30">·</span>
                <span className="shrink-0 mono" dir="ltr">
                  {item.connections}
                </span>
              </>
            ) : null}
            {!item.supports_range && !isDone ? (
              <>
                <span className="opacity-30">·</span>
                <span className="shrink-0 text-warning/80">{t('rangeUnsupported')}</span>
              </>
            ) : null}
          </div>
        </div>

        {/* Actions: one primary, the destructive one appears on hover. */}
        <div className="flex items-center gap-0.5 shrink-0">
          {isActive ? (
            <button
              type="button"
              onClick={() => onPause(item.id)}
              title={t('pause')}
              aria-label={t('pause')}
              className="btn-icon hover:text-warning"
            >
              <Pause size={15} />
            </button>
          ) : null}

          {isPaused || isError ? (
            <button
              type="button"
              onClick={() => onResume(item.id)}
              title={isError ? t('retry') : t('resume')}
              aria-label={isError ? t('retry') : t('resume')}
              className="btn-icon hover:text-emerald-400"
            >
              {isError ? <RotateCw size={15} /> : <Play size={15} />}
            </button>
          ) : null}

          {isQueued ? (
            <span className="btn-icon cursor-default" title={t('queuedNotice')}>
              <Clock size={15} />
            </span>
          ) : null}

          {isDone ? (
            <>
              <button
                type="button"
                onClick={() => onOpenFile(item.save_path)}
                title={t('openFile')}
                aria-label={t('openFile')}
                className="btn-icon hover:text-emerald-400"
              >
                <CheckCircle2 size={15} />
              </button>
              <button
                type="button"
                onClick={() => onOpenFolder(item.save_path)}
                title={t('openFolder')}
                aria-label={t('openFolder')}
                className="btn-icon"
              >
                <FolderOpen size={15} />
              </button>
            </>
          ) : null}

          {/* A live transfer can be opened in its own window. */}
          {onOpenTransfer && (isActive || isPaused) ? (
            <button
              type="button"
              onClick={() => onOpenTransfer(item.id)}
              title={t('transferTitle')}
              aria-label={t('transferTitle')}
              className="btn-icon hover:text-accent2"
            >
              <Maximize2 size={14} />
            </button>
          ) : null}

          {/* Always visible. It used to appear only on `group-hover`, which
              left it permanently invisible on touch screens, on Windows when
              the pointer never enters the row, and for keyboard users who tab
              through the list — the delete action appeared to be missing. */}
          <button
            type="button"
            onClick={() => onDelete(item.id)}
            title={t('removeAndDelete')}
            aria-label={t('removeAndDelete')}
            className="btn-icon hover:text-danger hover:border-danger/40"
          >
            <Trash2 size={15} />
          </button>
        </div>
      </div>

      {/* ------------------------------------------------- segment ribbon */}
      {!isDone ? (
        <div className="mt-2.5">
          {hasSegments && item.total_size > 0 ? (
            <div className="ribbon h-3">
              {item.segments.map((segment) => {
                const span = segment.total_bytes || 1;
                const ratio = Math.max(
                  0,
                  Math.min(100, (segment.downloaded_bytes / span) * 100)
                );
                return (
                  <span
                    key={segment.index}
                    data-done={segment.is_finished}
                    title={`${segment.index + 1}/${item.segments.length} — ${ratio.toFixed(0)}%`}
                  >
                    <i style={{ width: `${ratio}%` }} />
                  </span>
                );
              })}
            </div>
          ) : (
            <div className="relative h-2.5 rounded-full bg-input overflow-hidden">
              <div
                className={`absolute inset-y-0 start-0 rounded-full transition-[width] duration-200 ease-smooth ${
                  isActive
                    ? 'bg-gradient-to-r from-accent1 via-accent2 to-accent3'
                    : isError
                    ? 'bg-danger/70'
                    : 'bg-inkMuted/40'
                }`}
                style={{ width: `${percent}%` }}
              />
            </div>
          )}

          <div className="flex items-center justify-between mt-1.5 text-[12px]">
            <span
              className={`mono font-bold ${isError ? 'text-danger' : 'text-inkSoft'}`}
              dir="ltr"
            >
              {percent.toFixed(percent >= 10 ? 0 : 1)}%
            </span>
            <span className="flex items-center gap-2 text-inkMuted mono" dir="ltr">
              {isActive ? (
                <>
                  <span className="text-accent2">{formatSpeed(item.speed_bps)}</span>
                  <span className="opacity-30">·</span>
                  <span>{formatDuration(item.eta_seconds)}</span>
                </>
              ) : isError ? (
                <span className="text-danger/90 truncate max-w-[380px]" dir="auto">
                  {item.error_message || t('errGeneric')}
                </span>
              ) : isPaused ? (
                <span className="text-warning/80">
                  {formatBytes(item.downloaded_size)} / {sizeLabel}
                </span>
              ) : (
                <span>{t('statusQueued')}</span>
              )}
            </span>
          </div>
        </div>
      ) : (
        <div className="mt-2 flex items-center gap-1.5 text-[12px] text-emerald-400/85">
          <CheckCircle2 size={11} />
          <span className="mono" dir="ltr">
            {formatBytes(item.total_size)}
          </span>
        </div>
      )}

      {isError && item.error_message && item.total_size === 0 ? (
        <div className="mt-1.5 flex items-start gap-1.5 text-[12px] text-danger/90">
          <AlertTriangle size={11} className="mt-0.5 shrink-0" />
          <span className="truncate">{item.error_message}</span>
        </div>
      ) : null}
    </article>
  );
};

/** Only re-render a row when a value it actually paints has changed. */
export const DownloadRow = memo(DownloadRowBase, (prev, next) => {
  const a = prev.item;
  const b = next.item;
  return (
    a.id === b.id &&
    a.status === b.status &&
    a.downloaded_size === b.downloaded_size &&
    a.total_size === b.total_size &&
    a.progress_percent === b.progress_percent &&
    a.speed_bps === b.speed_bps &&
    a.eta_seconds === b.eta_seconds &&
    a.error_message === b.error_message &&
    prev.index === next.index &&
    a.segments.length === b.segments.length &&
    a.segments.every((segment, i) => {
      const other = b.segments[i];
      return (
        other !== undefined &&
        segment.downloaded_bytes === other.downloaded_bytes &&
        segment.is_finished === other.is_finished
      );
    })
  );
});

/** Backwards-compatible alias: the rest of the app imports DownloadCard. */
export const DownloadCard = DownloadRow;
