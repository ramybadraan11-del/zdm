//! ZDM Transfer Window
//! Developed by Ramy Badraan
//!
//! A small, self-contained window for one running download: the file, the
//! segment ribbon, speed/ETA, and the three controls. It renders only when the
//! webview URL carries `?window=transfer&id=<download>`.
//!
//! Design intent: this window is the "hero" of the app. The segment ribbon is the
//! signature element — one column per connection, each filling at its own rate, so
//! parallelism is visible rather than asserted.

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  Pause,
  Play,
  X,
  Pin,
  PinOff,
  FolderOpen,
  CheckCircle2,
  AlertTriangle,
  Loader2,
  ArrowDownToLine,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { DownloadItem, ProgressPayload } from '../types';
import { formatBytes, formatSpeed, formatDuration, hostOf } from '../utils/format';

interface TransferWindowProps {
  downloadId: string;
}

/** One column of the segment ribbon. */
const SegmentBar: React.FC<{
  index: number;
  ratio: number;
  active: boolean;
  done: boolean;
  total: number;
}> = ({ index, ratio, active, done, total }) => {
  const pct = Math.max(0, Math.min(100, ratio * 100));
  return (
    <div
      className="relative flex-1 rounded-[2px] overflow-hidden bg-input lane-rail-track"
      title={`${index + 1}/${total} — ${pct.toFixed(0)}%`}
      style={{ minWidth: 3 }}
    >
      {/* Fills bottom-up: a bank of parallel streams, not a stack of pills. */}
      <div
        className={`absolute inset-x-0 bottom-0 rounded-[2px] transition-[height] duration-300 ease-smooth ${
          done ? 'tw-seg-done' : active ? 'tw-seg-active' : 'tw-seg-idle'
        }`}
        style={{ height: `${Math.max(4, pct)}%` }}
      />
      {/* Travelling glint gives the column life without a permanent animation. */}
      {active && !done ? (
        <div
          className="absolute inset-x-0 h-1/3 opacity-50"
          style={{
            background:
              'linear-gradient(180deg, transparent, rgba(255,255,255,0.7), transparent)',
            animation: 'zdm-travel 1.9s linear infinite',
            animationDelay: `${index * 110}ms`,
          }}
        />
      ) : null}
    </div>
  );
};

export const TransferWindow: React.FC<TransferWindowProps> = ({ downloadId }) => {
  const { t, isRtl } = useI18n();

  const [item, setItem] = useState<DownloadItem | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const [busy, setBusy] = useState(false);
  const [finished, setFinished] = useState(false);

  const windowLabel = useMemo(
    () => `transfer-${downloadId.replace(/[^a-zA-Z0-9]/g, '')}`,
    [downloadId]
  );

  // ---------------------------------------------------------------- data
  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      try {
        const items = await invoke<DownloadItem[]>('get_downloads');
        if (cancelled) return;
        const found = items.find((entry) => entry.id === downloadId) ?? null;
        setItem(found);
        if (found?.status === 'completed') setFinished(true);
      } catch (err) {
        if (!cancelled) setError(String(err));
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, [downloadId]);

  // Live progress: patch only this download, and only when something moved.
  const lastPaint = useRef(0);
  useEffect(() => {
    const off = listen<ProgressPayload>('download_progress', (event) => {
      if (event.payload.id !== downloadId) return;

      // The engine ticks at ~8 Hz; painting at 60 fps buys nothing and costs CPU.
      const now = performance.now();
      if (now - lastPaint.current < 60 && event.payload.progress_percent < 100) return;
      lastPaint.current = now;

      setItem((current) =>
        current
          ? {
              ...current,
              downloaded_size: event.payload.downloaded_bytes,
              total_size:
                event.payload.total_bytes > 0
                  ? event.payload.total_bytes
                  : current.total_size,
              speed_bps: event.payload.speed_bps,
              eta_seconds: event.payload.eta_seconds,
              progress_percent: event.payload.progress_percent,
              segments:
                event.payload.segments.length > 0
                  ? event.payload.segments
                  : current.segments,
              status: 'downloading',
            }
          : current
      );
    });

    return () => {
      off.then((fn) => fn()).catch(() => undefined);
    };
  }, [downloadId]);

  // Completion: stop the spinner and offer the file.
  useEffect(() => {
    const offs = [
      listen<{ id: string }>('download_completed', (event) => {
        if (event.payload?.id === downloadId) {
          setFinished(true);
          void invoke<DownloadItem[]>('get_downloads').then((items) => {
            const found = items.find((entry) => entry.id === downloadId);
            if (found) setItem(found);
          });
        }
      }),
      listen<{ id: string; error: string }>('download_error', (event) => {
        if (event.payload?.id === downloadId) setError(event.payload.error);
      }),
      listen<{ id: string }>('download_paused', (event) => {
        if (event.payload?.id === downloadId) {
          setItem((current) => (current ? { ...current, status: 'paused' } : current));
        }
      }),
    ];

    return () => {
      offs.forEach((p) => p.then((fn) => fn()).catch(() => undefined));
    };
  }, [downloadId]);

  // ------------------------------------------------------------ controls
  const pause = useCallback(async () => {
    setBusy(true);
    try {
      await invoke('pause_download', { id: downloadId });
      setItem((current) => (current ? { ...current, status: 'paused' } : current));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [downloadId]);

  const resume = useCallback(async () => {
    setBusy(true);
    try {
      await invoke('resume_download', { id: downloadId });
      setItem((current) => (current ? { ...current, status: 'downloading' } : current));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [downloadId]);

  const cancel = useCallback(async () => {
    setBusy(true);
    try {
      await invoke('cancel_download', { id: downloadId });
      await invoke('close_transfer_window', { id: downloadId });
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }, [downloadId]);

  const openFile = useCallback(async () => {
    if (!item) return;
    try {
      await invoke('open_file', { path: item.save_path });
    } catch (err) {
      setError(String(err));
    }
  }, [item]);

  const openFolder = useCallback(async () => {
    if (!item) return;
    try {
      await invoke('open_file_folder', { path: item.save_path });
    } catch (err) {
      setError(String(err));
    }
  }, [item]);

  const togglePin = useCallback(async () => {
    const next = !pinned;
    setPinned(next);
    try {
      await invoke('set_window_always_on_top', { label: windowLabel, enabled: next });
    } catch {
      /* pinning is a nicety, not a requirement */
    }
  }, [pinned, windowLabel]);

  // ----------------------------------------------------------------- render
  const percent = finished
    ? 100
    : Math.min(100, Math.max(0, item?.progress_percent ?? 0));
  const isActive = item?.status === 'downloading';
  const isPaused = item?.status === 'paused';
  const segments = item?.segments ?? [];
  const segmentCount = segments.length > 0 ? segments.length : item?.connections ?? 1;

  // Unknown length: show a sweeping indeterminate ribbon instead of a fake %.
  const indeterminate = (item?.total_size ?? 0) === 0 && isActive;

  return (
    <div className="dl-window tw select-none" dir={isRtl ? 'rtl' : 'ltr'}>
      {/* ── Header: identity + window controls ─────────────────────────── */}
      <header className="tw-head drag-region">
        <div className="tw-mark" aria-hidden="true">
          <ArrowDownToLine size={13} strokeWidth={3} />
        </div>
        <div className="tw-head-text">
          <span className="tw-name" dir="ltr">
            {item?.filename ?? t('transferTitle')}
          </span>
          <span className="tw-host" dir="ltr">
            {item ? hostOf(item.url) : ''}
          </span>
        </div>
        <button
          type="button"
          className="tw-icon no-drag"
          onClick={togglePin}
          title={t('transferMinimizeToTray')}
          aria-label={t('transferMinimizeToTray')}
        >
          {pinned ? <Pin size={12} /> : <PinOff size={12} />}
        </button>
        <button
          type="button"
          className="tw-icon no-drag tw-icon-close"
          onClick={() => void invoke('close_transfer_window', { id: downloadId })}
          title={t('close')}
          aria-label={t('close')}
        >
          <X size={13} />
        </button>
      </header>

      {/* ── Body: figure, ribbon, byte counts ──────────────────────────── */}
      <div className="tw-body">
        {error && (
          <div className="dl-error">
            <AlertTriangle size={12} />
            <span>{error}</span>
          </div>
        )}

        {/* The number is the headline; speed and ETA sit beside it. */}
        <div className="tw-figure">
          <div className="tw-percent" dir="ltr">
            {finished ? (
              <CheckCircle2 size={22} className="tw-done-icon" />
            ) : (
              <>
                <span className={indeterminate ? 'tw-num tw-num-dim' : 'tw-num'}>
                  {indeterminate ? '—' : percent.toFixed(percent >= 10 || finished ? 0 : 1)}
                </span>
                {!indeterminate && <span className="tw-num-unit">%</span>}
              </>
            )}
          </div>

          <div className="tw-stats">
            <span className={isActive ? 'tw-speed mono' : 'tw-speed tw-speed-idle'} dir="ltr">
              {isActive ? formatSpeed(item?.speed_bps ?? 0) : '0 B/s'}
            </span>
            <span className="tw-eta mono" dir="ltr">
              {finished
                ? t('transferDone')
                : isActive
                  ? `${t('transferEta')} ${formatDuration(item?.eta_seconds ?? 0)}`
                  : t('statusPaused')}
            </span>
          </div>
        </div>

        {/* Segment ribbon — the signature element. */}
        <div className="tw-ribbon">
          {indeterminate ? (
            <div className="tw-sweep">
              <div className="tw-sweep-bar" />
            </div>
          ) : segments.length > 1 ? (
            segments.map((segment) => {
              const span = segment.total_bytes || 1;
              return (
                <SegmentBar
                  key={segment.index}
                  index={segment.index}
                  ratio={segment.downloaded_bytes / span}
                  active={isActive && !segment.is_finished}
                  done={segment.is_finished}
                  total={segments.length}
                />
              );
            })
          ) : (
            <div className="tw-sweep">
              <div
                className={
                  finished ? 'tw-single-fill tw-single-fill-done' : 'tw-single-fill'
                }
                style={{ width: `${percent}%` }}
              />
            </div>
          )}
        </div>

        <div className="tw-foot-info">
          <span className="mono" dir="ltr">
            {formatBytes(item?.downloaded_size ?? 0)}
            {' / '}
            {(item?.total_size ?? 0) > 0 ? formatBytes(item?.total_size ?? 0) : '∞'}
          </span>
          <span className="mono" dir="ltr">
            {segmentCount} {t('transferSegments')}
          </span>
        </div>
      </div>

      {/* ── Actions ─────────────────────────────────────────────────────── */}
      <footer className="tw-actions">
        {finished ? (
          <>
            <button type="button" onClick={() => void openFile()} className="dl-btn dl-btn-primary tw-act-wide">
              <CheckCircle2 size={13} />
              {t('openFile')}
            </button>
            <button type="button" onClick={() => void openFolder()} className="dl-btn dl-btn-ghost">
              <FolderOpen size={13} />
              {t('showFolder')}
            </button>
          </>
        ) : (
          <>
            {isPaused ? (
              <button
                type="button"
                onClick={() => void resume()}
                disabled={busy}
                className="dl-btn dl-btn-primary tw-act-wide"
              >
                {busy ? <Loader2 size={13} className="anim-spin" /> : <Play size={13} />}
                {t('resume')}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => void pause()}
                disabled={busy || !isActive}
                className="dl-btn dl-btn-primary tw-act-wide"
              >
                {busy ? <Loader2 size={13} className="anim-spin" /> : <Pause size={13} />}
                {t('pause')}
              </button>
            )}

            <button
              type="button"
              onClick={() => void openFolder()}
              className="dl-btn dl-btn-ghost tw-act-icon"
              title={t('showFolder')}
              aria-label={t('showFolder')}
            >
              <FolderOpen size={13} />
            </button>

            <button type="button" onClick={() => void cancel()} disabled={busy} className="dl-btn dl-btn-danger" title={t('cancel')}>
              <X size={13} />
            </button>
          </>
        )}
      </footer>
    </div>
  );
};
