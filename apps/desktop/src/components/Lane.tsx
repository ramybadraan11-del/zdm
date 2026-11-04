//! ZDM Lane — the core card
//! Developed by Ramy Badraan
//!
//! Design guide §04: every download is a LANE, a wide horizontal card. Layout is
//! fixed — [file tile 40×40] [meta, flexible] [speed 90px] [actions] — so a list
//! of 10,000 stays calm and scannable.
//!
//! The SEGMENT RAIL is the product's signature: one segment per real connection
//! (1–32), each filling on its own. It is the visual proof that many connections
//! run at once, which is what makes ZDM fast. Rendered with CSS width transitions
//! only — no JS animation loop per segment.

import React, { memo, useEffect, useMemo, useRef, useState } from 'react';
import {
  Pause,
  Play,
  FolderOpen,
  Trash2,
  RotateCw,
  MoreHorizontal,
  AlertTriangle,
  Check,
  ExternalLink,
  FileDown,
  Link2,
  Copy,
  ListOrdered,
} from 'lucide-react';
import { DownloadItem } from '../types';
import { formatBytes, formatSpeed, formatDuration, hostOf } from '../utils/format';
import { useI18n } from '../i18n';

interface LaneProps {
  item: DownloadItem;
  selected?: boolean;
  onToggleSelect?: (id: string) => void;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onDelete: (id: string) => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
  onOpenDetails?: (id: string) => void;
  onRestart?: (id: string) => void;
  onUpdateUrl?: (id: string) => void;
  onToggleQueue?: (id: string) => void;
  isQueued?: boolean;
  onMenu?: (id: string, anchor: { x: number; y: number }) => void;
}

/**
 * Split a formatted speed into its number and unit.
 *
 * The guide makes the number the hero and the unit a small caption, so they must
 * be separate spans — "10.9 MB/s" in one span cannot be styled that way.
 */
function splitSpeed(bps: number): [string, string] {
  const formatted = formatSpeed(bps);
  const match = formatted.match(/^([\d.,]+)\s*(.+)$/);
  if (!match) return ['0', 'MB/s'];
  return [match[1], match[2]];
}

/** Extension tile: a rounded square with the file's extension, never a stock icon. */
export function FileTile({ item }: { item: DownloadItem }) {
  const ext = useMemo(() => {
    if (item.category === 'Torrent') return 'BT';
    const fromName = item.filename.split('.').pop() ?? '';
    const clean = fromName.toLowerCase().replace(/[^a-z0-9]/g, '');
    return (clean.length > 0 && clean.length <= 4 ? clean : 'file').toUpperCase();
  }, [item.filename]);

  // Tile colour follows file type: ISO/program=Volt, video=Violet,
  // archive=Amber, document/audio=Pulse.
  const kind = useMemo(() => {
    const category = (item.category || '').toLowerCase();
    const lower = item.filename.toLowerCase();
    if (category === 'torrent') return 'torrent';
    if (category === 'video' || /\.(mp4|mkv|webm|avi|mov|m4v|flv)$/.test(lower)) return 'video';
    if (/\.(zip|rar|7z|tar|gz|xz|bz2)$/.test(lower)) return 'archive';
    if (/\.(pdf|docx?|xlsx?|pptx?|txt|epub)$/.test(lower)) return 'doc';
    if (category === 'audio' || /\.(mp3|m4a|flac|wav|ogg|opus)$/.test(lower)) return 'audio';
    if (category === 'images' || /\.(jpe?g|png|webp|gif|bmp|svg|ico|avif|tiff?)$/.test(lower)) return 'images';
    return 'program';
  }, [item.category, item.filename]);

  return (
    <div className="lane-tile" data-kind={kind} aria-hidden="true">
      <span className="lane-tile-ext">{ext}</span>
    </div>
  );
}

/** The signature segment rail. */
const SegmentRail: React.FC<{ item: DownloadItem }> = ({ item }) => {
  const segments = item.segments ?? [];
  const count = segments.length > 0 ? segments.length : Math.max(1, item.connections || 1);
  const unknownLength = (item.total_size ?? 0) === 0 && item.status === 'downloading';

  // Uneven segments look like separate streams; equal ones look like one bar.
  const columns = useMemo(() => {
    if (segments.length > 0) {
      return segments.map((segment) => {
        const span = segment.total_bytes || 1;
        return Math.max(0, Math.min(1, segment.downloaded_bytes / span));
      });
    }
    // No per-segment data yet: distribute the known percentage evenly.
    const overall = Math.max(0, Math.min(1, (item.progress_percent ?? 0) / 100));
    return Array.from({ length: count }, () => overall);
  }, [segments, count, item.progress_percent]);

  return (
    <div
      className="lane-rail"
      data-status={item.status}
      data-indeterminate={unknownLength}
      aria-hidden="true"
    >
      {unknownLength ? (
        <span className="lane-rail-sweep" />
      ) : (
        columns.map((ratio, index) => (
          <span key={index} className="lane-rail-track">
            <span
              className="lane-rail-fill"
              style={{ width: `${Math.max(ratio * 100, ratio > 0 ? 3 : 0)}%` }}
            />
          </span>
        ))
      )}
    </div>
  );
};

/** Status pill — every state carries text, never colour alone. */
const StatusPill: React.FC<{ item: DownloadItem }> = ({ item }) => {
  const { t } = useI18n();

  if (item.status === 'downloading') return null; // the rail already says it

  const label =
    item.status === 'paused'
      ? t('statusPaused')
      : item.status === 'queued'
        ? t('statusQueued')
        : item.status === 'completed'
          ? t('statusCompleted')
          : item.status === 'error'
            ? t('statusError')
            : t('statusCancelled');

  return (
    <span className="lane-pill" data-status={item.status}>
      {item.status === 'completed' && <Check size={9} strokeWidth={3.5} />}
      {item.status === 'error' && <AlertTriangle size={9} strokeWidth={3} />}
      {label}
    </span>
  );
};

export const Lane = memo(function Lane({
  item,
  selected,
  onToggleSelect,
  onPause,
  onResume,
  onDelete,
  onOpenFile,
  onOpenFolder,
  onOpenDetails,
  onRestart,
  onUpdateUrl,
  onToggleQueue,
  isQueued,
}: LaneProps) {
  const { t } = useI18n();

  // The three-dot menu and right-click context menu.
  const [menuAt, setMenuAt] = useState<{ top: number; left: number } | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!menuAt) return;

    const close = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuAt(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setMenuAt(null);
    };
    const dismiss = () => setMenuAt(null);

    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', escape);
    window.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('keydown', escape);
      window.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
    };
  }, [menuAt]);

  const toggleMenu = (event: React.MouseEvent<HTMLElement>) => {
    if (menuAt) {
      setMenuAt(null);
      return;
    }

    const rect = event.currentTarget.getBoundingClientRect();
    const width = 188;
    const rtl = getComputedStyle(document.documentElement).direction === 'rtl';
    const left = rtl
      ? Math.max(8, rect.right - width)
      : Math.min(rect.left, window.innerWidth - width - 8);

    setMenuAt({ top: rect.bottom + 4, left });
  };

  const handleContextMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    const width = 188;
    const rtl = getComputedStyle(document.documentElement).direction === 'rtl';
    const left = rtl
      ? Math.max(8, event.clientX - width)
      : Math.min(event.clientX, window.innerWidth - width - 8);
    const top = Math.min(event.clientY, Math.max(8, window.innerHeight - 300));
    setMenuAt({ top, left });
  };

  const run = (action: () => void) => {
    setMenuAt(null);
    action();
  };

  const active = item.status === 'downloading';
  const paused = item.status === 'paused';
  const done = item.status === 'completed';
  const failed = item.status === 'error';

  const percent = done ? 100 : Math.max(0, Math.min(100, item.progress_percent ?? 0));

  // Line 3 of the meta column: host · size · time · connections.
  const facts: string[] = [hostOf(item.url)];
  if ((item.total_size ?? 0) > 0) facts.push(formatBytes(item.total_size));
  if (active && item.eta_seconds > 0) facts.push(`${formatDuration(item.eta_seconds)} ${t('laneLeft')}`);
  if (done && item.finished_at) facts.push(t('laneCompleted'));
  if (paused) facts.push(`${t('lanePausedAt')} ${percent.toFixed(0)}%`);
  if (!done && !paused && item.connections > 0) {
    facts.push(`${item.connections} ${t('laneConn')}`);
  }

  return (
    <article
      className="lane"
      data-status={item.status}
      data-selected={selected}
      data-selectable={!!onToggleSelect}
      onDoubleClick={() => onOpenDetails?.(item.id)}
      onContextMenu={handleContextMenu}
      tabIndex={0}
      role="button"
      aria-label={item.filename}
    >
      {onToggleSelect && (
        <div
          className="flex items-center justify-center me-1.5 shrink-0 select-none"
          onClick={(e) => {
            e.stopPropagation();
            onToggleSelect(item.id);
          }}
        >
          <input
            type="checkbox"
            checked={!!selected}
            onChange={() => {}}
            className="w-4 h-4 rounded border border-line bg-input cursor-pointer accent-accent1"
            title={selected ? t('deselectAll') : t('selectAll')}
          />
        </div>
      )}

      <FileTile item={item} />

      <div className="lane-meta">
        <div className="lane-head">
          <span className="lane-name" dir="auto" title={item.filename}>
            {item.filename}
          </span>
          <span className="lane-pct mono" dir="ltr">
            {done ? '' : `${percent.toFixed(percent >= 10 ? 0 : 1)}%`}
          </span>
        </div>

        <SegmentRail item={item} />

        <div className="lane-facts">
          <span className="lane-fact-text" dir="auto">
            {failed && item.error_message ? item.error_message : facts.join(' · ')}
          </span>
          <StatusPill item={item} />
        </div>
      </div>

      <div className="lane-speed">
        {done ? (
          <span className="lane-speed-done mono">✓</span>
        ) : active ? (
          (() => {
            const [value, unit] = splitSpeed(item.speed_bps ?? 0);
            return (
              <>
                <span className="lane-speed-num mono" dir="ltr">
                  {value}
                </span>
                <span className="lane-speed-unit">{unit}</span>
              </>
            );
          })()
        ) : (
          <span className="lane-speed-idle">—</span>
        )}
      </div>

      <div className="lane-actions">
        {done ? (
          <>
            {/* Two different verbs, two different icons. They were both
                FolderOpen, so "open file" and "show folder" looked identical. */}
            <button
              type="button"
              className="lane-btn"
              onClick={() => onOpenFile(item.save_path)}
              title={t('openFile')}
              aria-label={t('openFile')}
            >
              <ExternalLink size={13} />
            </button>
            <button
              type="button"
              className="lane-btn"
              onClick={() => onOpenFolder(item.save_path)}
              title={t('showFolder')}
              aria-label={t('showFolder')}
            >
              <FolderOpen size={13} />
            </button>
          </>
        ) : failed ? (
          <button
            type="button"
            className="lane-btn lane-btn-volt"
            onClick={() => onResume(item.id)}
            title={t('retry')}
            aria-label={t('retry')}
          >
            <RotateCw size={13} />
          </button>
        ) : (
          <button
            type="button"
            className="lane-btn"
            onClick={() => (paused ? onResume(item.id) : onPause(item.id))}
            title={paused ? t('resume') : t('pause')}
            aria-label={paused ? t('resume') : t('pause')}
          >
            {paused ? <Play size={13} /> : <Pause size={13} />}
          </button>
        )}

        {/* The menu trigger belongs to every state. It used to be inside the
            "not done and not failed" branch, so right-clicking a completed or
            failed row set the position but rendered nothing. */}
        <button
          type="button"
          className="lane-btn"
          onClick={toggleMenu}
          title={t('more')}
          aria-label={t('more')}
          aria-haspopup="menu"
          aria-expanded={menuAt !== null}
        >
          <MoreHorizontal size={13} />
        </button>
      </div>

      {menuAt ? (
        <div
          className="lane-menu"
          ref={menuRef}
          role="menu"
          style={{ top: menuAt.top, left: menuAt.left }}
        >
          {done ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onOpenFile(item.save_path))}
            >
              <ExternalLink size={12} />
              {t('openFile')}
            </button>
          ) : null}

          {done ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onOpenFolder(item.save_path))}
            >
              <FolderOpen size={12} />
              {t('showFolder')}
            </button>
          ) : null}

          {item.status === 'paused' || item.status === 'error' ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onResume(item.id))}
            >
              <Play size={12} />
              {item.status === 'error' ? t('retry') : t('resume')}
            </button>
          ) : null}

          {item.status === 'downloading' ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onPause(item.id))}
            >
              <Pause size={12} />
              {t('pause')}
            </button>
          ) : null}

          {!done ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onOpenDetails?.(item.id))}
            >
              <FileDown size={12} />
              {t('transferTitle')}
            </button>
          ) : null}

          {onRestart ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onRestart(item.id))}
            >
              <RotateCw size={12} />
              {t('redownload')}
            </button>
          ) : null}

          {onUpdateUrl ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onUpdateUrl(item.id))}
            >
              <Link2 size={12} />
              {t('updateUrl')}
            </button>
          ) : null}

          {onToggleQueue && !done ? (
            <button
              type="button"
              role="menuitem"
              onClick={() => run(() => onToggleQueue(item.id))}
            >
              <ListOrdered size={12} />
              {isQueued ? t('removeFromQueue') : t('addToQueue')}
            </button>
          ) : null}

          <button
            type="button"
            role="menuitem"
            onClick={() => run(() => void navigator.clipboard.writeText(item.url))}
          >
            <Copy size={12} />
            {t('copyLink')}
          </button>

          <button
            type="button"
            role="menuitem"
            className="lane-menu-danger"
            onClick={() => run(() => onDelete(item.id))}
          >
            <Trash2 size={12} />
            {t('removeAndDelete')}
          </button>
        </div>
      ) : null}
    </article>
  );
});
