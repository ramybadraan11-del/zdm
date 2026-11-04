//! ZDM Queue panel
//! Developed by Ramy Badraan
//!
//! The scheduler, as requested: a list of files waiting their turn, with control
//! over how many run at once and what happens when the list empties.
//!
//! Layout follows the design guide: one card, rows with a fixed grid, Volt for the
//! primary action, text pills for state. Nothing here introduces a new colour or a
//! gradient — it reuses `.lane`, `.sched-*` and the shared tokens.

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  ListOrdered,
  Play,
  Pause,
  ArrowUp,
  ArrowDown,
  ArrowUpToLine,
  X,
  Loader2,
  Power,
  Moon,
  Ban,
  Layers,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { DownloadItem } from '../types';
import { formatBytes, hostOf } from '../utils/format';

type ParallelMode = 'sequential' | 'two' | 'three' | 'four' | 'auto';
type DrainAction = 'nothing' | 'sleep' | 'shut_down';

interface QueueConfig {
  enabled: boolean;
  parallel: ParallelMode;
  drain_action: DrainAction;
  paused: boolean;
}

interface QueueStatus {
  config: QueueConfig;
  order: string[];
  running: number;
  slots: number;
  drained: boolean;
}

const PARALLEL: { id: ParallelMode; key: string }[] = [
  { id: 'sequential', key: 'queueSeq' },
  { id: 'two', key: 'queueTwo' },
  { id: 'three', key: 'queueThree' },
  { id: 'four', key: 'queueFour' },
  { id: 'auto', key: 'queueAuto' },
];

const DRAIN: { id: DrainAction; key: string; icon: React.ElementType }[] = [
  { id: 'nothing', key: 'queueDrainNothing', icon: Ban },
  { id: 'sleep', key: 'queueDrainSleep', icon: Moon },
  { id: 'shut_down', key: 'queueDrainShutdown', icon: Power },
];

interface Props {
  downloads: DownloadItem[];
  onQueuedChange: () => void;
}

export const QueuePanel: React.FC<Props> = ({ downloads, onQueuedChange }) => {
  const { t } = useI18n();

  const [status, setStatus] = useState<QueueStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setStatus(await invoke<QueueStatus>('queue_status'));
    } catch {
      /* leave the last known state */
    }
  }, []);

  useEffect(() => {
    void load();

    // Guarded: `listen` throws outside a Tauri webview, and an exception in an
    // effect unmounts the tree — which showed as a black window.
    let off: (() => void) | undefined;
    try {
      void listen<QueueStatus>('queue_changed', (event) => {
        if (event.payload) setStatus(event.payload);
      })
        .then((fn) => {
          off = fn;
        })
        .catch(() => undefined);
    } catch {
      /* no IPC bridge: the panel still works, it just polls on open */
    }

    return () => {
      off?.();
    };
  }, [load]);

  const patch = useCallback(
    async (partial: Partial<QueueConfig>) => {
      if (!status) return;
      setBusy(true);
      try {
        const next = await invoke<QueueStatus>('save_queue_config', {
          config: { ...status.config, ...partial },
        });
        setStatus(next);
      } finally {
        setBusy(false);
      }
    },
    [status]
  );

  // Ids in queue order, mapped to their live row.
  const queued = useMemo(() => {
    if (!status) return [];
    const byId = new Map(downloads.map((item) => [item.id, item]));
    return status.order
      .map((id) => byId.get(id))
      .filter((item): item is DownloadItem => Boolean(item));
  }, [status, downloads]);

  // Downloads that can still be added to the queue.
  const addable = useMemo(
    () =>
      downloads.filter(
        (item) =>
          item.status !== 'completed' &&
          item.status !== 'downloading' &&
          !status?.order.includes(item.id)
      ),
    [downloads, status]
  );

  const act = useCallback(
    async (command: string, args: Record<string, unknown> = {}) => {
      setBusy(true);
      try {
        await invoke(command, args);
        await load();
        onQueuedChange();
      } finally {
        setBusy(false);
      }
    },
    [load, onQueuedChange]
  );

  if (!status) {
    return (
      <div className="queue-loading">
        <Loader2 size={15} className="anim-spin" />
      </div>
    );
  }

  const config = status.config;
  const empty = queued.length === 0;

  return (
    <section className="queue">
      {/* ---- header: title, master switch, run controls ---------------- */}
      <header className="queue-head">
        <div className="queue-head-text">
          <span className="queue-title">
            <ListOrdered size={14} />
            {t('queueTitle')}
          </span>
          <span className="queue-sub">
            {config.enabled ? t('queueHintOn') : t('queueHintOff')}
          </span>
        </div>

        <label className="queue-master">
          <input
            type="checkbox"
            className="sched-switch"
            checked={config.enabled}
            onChange={(event) => void patch({ enabled: event.target.checked })}
          />
          <span>{config.enabled ? t('schedOn') : t('schedOff')}</span>
        </label>
      </header>

      {/* ---- the verdict line ------------------------------------------ */}
      <div className="queue-verdict" data-paused={config.paused} data-empty={empty}>
        <Layers size={11} />
        <span>
          {!config.enabled
            ? t('queueFree')
            : empty
              ? t('queueEmpty')
              : `${status.running} / ${status.slots} ${t('queueRunning')}`}
        </span>
        {!empty && (
          <div className="queue-run">
            <button
              type="button"
              className="queue-btn"
              onClick={() => {
                if (!config.enabled || config.paused) {
                  void act('start_queue');
                } else {
                  void act('pause_queue');
                }
              }}
              disabled={busy}
            >
              {config.paused || !config.enabled ? <Play size={11} /> : <Pause size={11} />}
              {config.paused || !config.enabled ? t('queueStart') : t('queuePause')}
            </button>
          </div>
        )}
      </div>

      {/* ---- how many at once ------------------------------------------ */}
      {config.enabled && (
        <div className="queue-section">
          <span className="dl-label">{t('queueParallel')}</span>
          <div className="queue-modes" role="radiogroup" aria-label={t('queueParallel')}>
            {PARALLEL.map((mode) => {
              const active = config.parallel === mode.id;
              return (
                <button
                  key={mode.id}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  className="queue-mode"
                  data-active={active}
                  onClick={() => void patch({ parallel: mode.id })}
                  disabled={busy}
                >
                  {t(mode.key as never)}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* ---- the list --------------------------------------------------- */}
      {queued.length > 0 && (
        <ol className="queue-list">
          {queued.map((item, index) => (
            <li key={item.id} className="queue-row" data-first={index === 0}>
              <span className="queue-pos mono" dir="ltr">
                {index + 1}
              </span>

              <div className="queue-row-text">
                <span className="queue-row-name" dir="auto">
                  {item.filename}
                </span>
                <span className="queue-row-meta" dir="auto">
                  {hostOf(item.url)}
                  {item.total_size > 0 ? ` · ${formatBytes(item.total_size)}` : ''}
                </span>
              </div>

              <div className="queue-row-actions">
                <button
                  type="button"
                  className="lane-btn"
                  onClick={() => void act('move_in_queue', { id: item.id, direction: 'top' })}
                  disabled={busy || index === 0}
                  title={t('queueTop')}
                  aria-label={t('queueTop')}
                >
                  <ArrowUpToLine size={12} />
                </button>
                <button
                  type="button"
                  className="lane-btn"
                  onClick={() => void act('move_in_queue', { id: item.id, direction: 'up' })}
                  disabled={busy || index === 0}
                  title={t('queueUp')}
                  aria-label={t('queueUp')}
                >
                  <ArrowUp size={12} />
                </button>
                <button
                  type="button"
                  className="lane-btn"
                  onClick={() => void act('move_in_queue', { id: item.id, direction: 'down' })}
                  disabled={busy || index === queued.length - 1}
                  title={t('queueDown')}
                  aria-label={t('queueDown')}
                >
                  <ArrowDown size={12} />
                </button>
                <button
                  type="button"
                  className="lane-btn"
                  onClick={() => void act('unqueue_downloads', { ids: [item.id] })}
                  disabled={busy}
                  title={t('queueRemove')}
                  aria-label={t('queueRemove')}
                >
                  <X size={12} />
                </button>
              </div>
            </li>
          ))}
        </ol>
      )}

      {/* ---- add more --------------------------------------------------- */}
      {addable.length > 0 && (
        <div className="queue-section">
          <span className="dl-label">{t('queueAdd')}</span>
          <div className="queue-add">
            {addable.map((item) => (
              <button
                key={item.id}
                type="button"
                className="queue-chip"
                onClick={() => void act('queue_downloads', { ids: [item.id] })}
                disabled={busy}
                title={`${item.filename}${item.total_size > 0 ? ` · ${formatBytes(item.total_size)}` : ''}`}
              >
                <span className="queue-chip-name" dir="auto">
                  {item.filename}
                </span>
                {item.status === 'paused' && (
                  <span className="queue-chip-state">{t('statusPaused')}</span>
                )}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* ---- what happens when it drains -------------------------------- */}
      {config.enabled && (
        <div className="queue-section">
          <span className="dl-label">{t('queueDrain')}</span>
          <div className="queue-drain" role="radiogroup" aria-label={t('queueDrain')}>
            {DRAIN.map((option) => {
              const Icon = option.icon;
              const active = config.drain_action === option.id;
              return (
                <button
                  key={option.id}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  className="queue-drain-btn"
                  data-active={active}
                  data-danger={option.id !== 'nothing'}
                  onClick={() => void patch({ drain_action: option.id })}
                  disabled={busy}
                >
                  <Icon size={11} />
                  {t(option.key as never)}
                </button>
              );
            })}
          </div>
          {config.drain_action !== 'nothing' && (
            <span className="queue-warn">{t('queueDrainWarn')}</span>
          )}
        </div>
      )}

      <footer className="queue-foot">
        <span>
          {queued.length} {t('queueQueued')} · {status.running}/{status.slots}
        </span>
      </footer>
    </section>
  );
};
