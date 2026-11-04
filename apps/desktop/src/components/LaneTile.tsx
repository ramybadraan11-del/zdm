//! ZDM Grid Lane — the compact card used in grid view.
//! Developed by Ramy Badraan
//!
//! Same data as the full lane, less chrome: icon + name + progress bar + the
//! one number that matters (speed or status). Actions stay reachable on hover
//! or via the context menu; nothing lives two clicks deeper than it should.

import { memo, useCallback } from 'react';
import { Pause, Play, FolderOpen, Trash2, Check } from 'lucide-react';
import { DownloadItem } from '../types';
import { formatSpeed, formatBytes } from '../utils/format';
import { useI18n } from '../i18n';
import { FileTile } from './Lane';

interface TileProps {
  item: DownloadItem;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onDelete: (id: string) => void;
  onOpenFolder: (path: string) => void;
  onOpenFile: (path: string) => void;
  onOpenDetails?: (id: string) => void;
}

export const LaneTile = memo(function LaneTile({
  item,
  onPause,
  onResume,
  onDelete,
  onOpenFolder,
  onOpenFile,
  onOpenDetails,
}: TileProps) {
  const { t } = useI18n();
  const active = item.status === 'downloading';
  const paused = item.status === 'paused';
  const done = item.status === 'completed';
  const percent = done
    ? 100
    : Math.max(0, Math.min(100, item.progress_percent ?? 0));

  const open = useCallback(() => {
    if (done) onOpenFile(item.save_path);
    else onOpenDetails?.(item.id);
  }, [done, item.save_path, item.id, onOpenFile, onOpenDetails]);

  return (
    <article
      className="tile-card"
      data-status={item.status}
      onDoubleClick={open}
      role="button"
      tabIndex={0}
      aria-label={item.filename}
      onKeyDown={(event) => {
        if (event.key === 'Enter') open();
      }}
    >
      <div className="tile-top">
        <FileTile item={item} />
        <div className="tile-text">
          <span className="tile-name" dir="auto" title={item.filename}>
            {item.filename}
          </span>
          <span className="tile-host mono" dir="ltr">
            {item.total_size ? formatBytes(item.total_size) : '—'}
          </span>
        </div>
      </div>

      <div className="tile-progress" aria-hidden="true">
        <div className="tile-progress-fill" style={{ width: `${percent}%` }} />
      </div>

      <div className="tile-foot">
        <span className="tile-speed mono" dir="ltr">
          {done ? (
            <Check size={11} strokeWidth={3.5} />
          ) : active ? (
            formatSpeed(item.speed_bps ?? 0)
          ) : (
            `${percent.toFixed(0)}%`
          )}
        </span>
        <span className="tile-actions">
          {active || paused ? (
            <button
              type="button"
              className="tile-action"
              onClick={() => (active ? onPause(item.id) : onResume(item.id))}
              title={active ? t('pause') : t('resume')}
              aria-label={active ? t('pause') : t('resume')}
            >
              {active ? <Pause size={11} /> : <Play size={11} />}
            </button>
          ) : null}
          <button
            type="button"
            className="tile-action"
            onClick={() => onOpenFolder(item.save_path)}
            title={t('showFolder')}
            aria-label={t('showFolder')}
          >
            <FolderOpen size={11} />
          </button>
          <button
            type="button"
            className="tile-action tile-action-danger"
            onClick={() => onDelete(item.id)}
            title={t('remove')}
            aria-label={t('remove')}
          >
            <Trash2 size={11} />
          </button>
        </span>
      </div>
    </article>
  );
});
