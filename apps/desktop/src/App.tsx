//! Zyzoom Download Manager (ZDM) — Main Application
//! Developed by Ramy Badraan
//!
//! State model:
//!  * `downloads` is the single source of truth for the list.
//!  * Progress events patch exactly one row, and only when something visible
//!    changed, so a fast download cannot re-render the whole list.
//!  * Settings are persisted by the backend and mirrored here.

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Settings as SettingsIcon, Trash2, Loader2, Link2, LayoutGrid, List, Film } from 'lucide-react';

import { CommandBar, SpeedMode } from './components/CommandBar';

/** The tab set the shell understands. Kept local: the sidebar that used to own
 *  it is gone, and the rail maps onto these same values. */
type NavTab = 'all' | 'active' | 'completed' | 'video' | 'queue' | 'settings';
import { IconRail, RailTab, StatusBar } from './components/IconRail';
import { QueuePanel } from './components/QueuePanel';
import { Lane } from './components/Lane';
import { LaneTile } from './components/LaneTile';
import { AddDownloadModal } from './components/AddDownloadModal';
import { DownloadConfirmDialog } from './components/DownloadConfirmDialog';
import { VideoDownloaderModal } from './components/VideoDownloaderModal';
import { SettingsView } from './components/SettingsView';
import { AboutModal } from './components/AboutModal';
import { Modal, Toast, ToastKind, ToastStack } from './components/ui';
import { I18nProvider, useI18n } from './i18n';
import { Lang } from './i18n/dict';
import {
  CompletedPayload,
  Diagnostics,
  DownloadItem,
  EngineStats,
  ProgressPayload,
  Settings,
  DEFAULT_INTERCEPT_EXTENSIONS,
} from './types';
import { formatBytes } from './utils/format';

const FILTERS = [
  { id: 'all', key: 'filterAll' as const },
  { id: 'Video', key: 'filterVideo' as const },
  { id: 'Audio', key: 'filterAudio' as const },
  { id: 'Programs', key: 'filterPrograms' as const },
  { id: 'Compressed', key: 'filterCompressed' as const },
  { id: 'Torrent', key: 'filterTorrent' as const },
  { id: 'Documents', key: 'filterDocuments' as const },
  { id: 'Others', key: 'filterOther' as const },
];

const EMPTY_STATS: EngineStats = {
  total_speed_bps: 0,
  active_count: 0,
  queued_count: 0,
  completed_count: 0,
  disk_free_bytes: 0,
};

const DEFAULT_SETTINGS: Settings = {
  download_dir: '',
  max_concurrent_downloads: 3,
  default_connections: 8,
  language: '',
  theme: 'moon',
  auto_start: true,
  capture_downloads: true,
  notifications: true,
  sniffer_enabled: true,
  speed_limit_kbps: 0,
  keep_partial_on_cancel: false,
  intercept_extensions: DEFAULT_INTERCEPT_EXTENSIONS,
  category_dirs: {},
};

// Quiet = 4 connections, Balanced = 8, Turbo = 16 (the user's request).
const CONNECTIONS_BY_MODE: Record<SpeedMode, number> = { quiet: 4, balanced: 8, turbo: 16 };

const speedModeFor = (connections: number): SpeedMode => {
  if (connections <= 4) return 'quiet';
  if (connections <= 8) return 'balanced';
  return 'turbo';
};

const Shell: React.FC = () => {
  const { t, lang, setLang } = useI18n();

  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [speedMode, setSpeedMode] = useState<SpeedMode>(() =>
    speedModeFor(DEFAULT_SETTINGS.default_connections)
  );
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [stats, setStats] = useState<EngineStats>(EMPTY_STATS);
  const [tab, setTab] = useState<NavTab>('all');
  const [category, setCategory] = useState<string>('all');
  // List or grid: persisted per-machine, not per-row.
  const [viewMode, setViewMode] = useState<'list' | 'grid'>(() =>
    typeof window !== 'undefined' && window.localStorage?.getItem('zdm-view') === 'grid' ? 'grid' : 'list'
  );
  const setViewModePersisted = (next: 'list' | 'grid') => {
    setViewMode(next);
    try { window.localStorage?.setItem('zdm-view', next); } catch { /* private mode */ }
  };
  const [query, setQuery] = useState('');
  const [toasts, setToasts] = useState<Toast[]>([]);

  const [addOpen, setAddOpen] = useState(false);
  const [addInitialUrl, setAddInitialUrl] = useState<string | undefined>();
  const [confirmUrl, setConfirmUrl] = useState<string | null>(null);
  // Highlights the window while a file is being dragged over it.
  const [dragActive, setDragActive] = useState(false);
  const [videoOpen, setVideoOpen] = useState(false);
  const [videoInitialUrl, setVideoInitialUrl] = useState<string | undefined>();
  const [aboutOpen, setAboutOpen] = useState(false);


  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);

  const toastSeq = useRef(0);
  const pushToast = useCallback((message: string, kind: ToastKind = 'info') => {
    const id = ++toastSeq.current;
    setToasts((current) => [...current.slice(-2), { id, message, kind }]);
    window.setTimeout(() => {
      setToasts((current) => current.filter((toast) => toast.id !== id));
    }, 4200);
  }, []);

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  // ------------------------------------------------------------- data load
  const refresh = useCallback(async () => {
    try {
      const [items, engineStats] = await Promise.all([
        invoke<DownloadItem[]>('get_downloads'),
        invoke<EngineStats>('get_stats'),
      ]);
      setDownloads(items);
      setStats(engineStats);
    } catch (error) {
      console.error('failed to load downloads', error);
    }
  }, []);

  const refreshStats = useCallback(async () => {
    try {
      setStats(await invoke<EngineStats>('get_stats'));
    } catch {
      /* transient */
    }
  }, []);

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false);
  const [batchDeleteWithFiles, setBatchDeleteWithFiles] = useState(false);
  const [updateUrlItem, setUpdateUrlItem] = useState<DownloadItem | null>(null);
  const [updateUrlValue, setUpdateUrlValue] = useState('');

  // Initial boot: settings first (language + theme), then data.
  useEffect(() => {
    (async () => {
      try {
        const stored = await invoke<Settings>('get_settings');
        setSettings(stored);
        setSpeedMode(speedModeFor(stored.default_connections));
        if (stored.language) setLang(stored.language as Lang);
      } catch (error) {
        console.error('settings unavailable', error);
      } finally {
        setSettingsLoaded(true);
      }
      await refresh();
      try {
        setDiagnostics(await invoke<Diagnostics>('get_diagnostics'));
      } catch {
        /* diagnostics are optional */
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Theme attribute drives the CSS variable set.
  useEffect(() => {
    // `system` is resolved entirely in CSS via prefers-color-scheme, so the
    // attribute is set verbatim and the OS decides.
    const theme = ['moon', 'dawn', 'system'].includes(settings.theme)
      ? settings.theme
      : 'moon';
    document.documentElement.setAttribute('data-theme', theme);
  }, [settings.theme]);

  // ---------------------------------------------------------- event wiring
  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];

    unlisteners.push(
      listen<ProgressPayload>('download_progress', (event) => {
        const payload = event.payload;

        setDownloads((current) => {
          const index = current.findIndex((item) => item.id === payload.id);
          if (index === -1) return current;

          const item = current[index];
          // A pause the user just requested must win over a late in-flight tick.
          if (item.status === 'paused' || item.status === 'completed') return current;

          // Drop the update entirely when nothing visible changed.
          if (
            item.downloaded_size === payload.downloaded_bytes &&
            item.speed_bps === payload.speed_bps &&
            item.status === 'downloading'
          ) {
            return current;
          }

          const next = current.slice();
          next[index] = {
            ...item,
            downloaded_size: payload.downloaded_bytes,
            total_size: payload.total_bytes > 0 ? payload.total_bytes : item.total_size,
            speed_bps: payload.speed_bps,
            eta_seconds: payload.eta_seconds,
            progress_percent: payload.progress_percent,
            segments: payload.segments.length > 0 ? payload.segments : item.segments,
            status: 'downloading',
          };
          return next;
        });
      })
    );

    unlisteners.push(
      listen<{ id: string }>('download_added', () => {
        void refresh();
      })
    );

    unlisteners.push(
      listen<{ id: string }>('download_started', () => {
        void refresh();
      })
    );

    unlisteners.push(
      listen<{ id: string }>('download_paused', () => {
        void refresh();
        void refreshStats();
      })
    );

    unlisteners.push(
      listen<{ id: string }>('download_queued', () => {
        void refresh();
      })
    );

    unlisteners.push(
      listen<{ id: string; error: string }>('download_error', (event) => {
        void refresh();
        pushToast(event.payload?.error || t('errGeneric'), 'error');
      })
    );

    unlisteners.push(
      listen<{ id: string }>('download_cancelled', () => {
        void refresh();
        void refreshStats();
      })
    );

    unlisteners.push(
      listen<{ id: string }>('download_deleted', () => {
        void refresh();
      })
    );

    unlisteners.push(
      listen<CompletedPayload>('download_completed', async (event) => {
        await refresh();
        void refreshStats();
        const payload = event.payload;
        if (payload?.filename) {
          // Never open a modal by itself. A modal reappeared every time the user
          // returned to the window, because the last completion was still in
          // state — closing the floating prompt should end the interaction.
          // A quiet toast is the right weight for "this one finished".
          pushToast(`${payload.filename} — ${t('statusCompleted')}`, 'success');

          if (settings.notifications) {
            try {
              await invoke('plugin:notification|is_permission_granted').catch(() => false);
            } catch {
              /* notifications are best-effort */
            }
          }
        }
      })
    );

    unlisteners.push(
      listen<{ url: string }>('open_video_modal_with_url', (event) => {
        setVideoInitialUrl(event.payload?.url);
        setVideoOpen(true);
      })
    );

    unlisteners.push(
      listen<{ url: string }>('add_url_from_cli', (event) => {
        const url = event.payload?.url;
        if (!url) return;
        // Clicking a link in the browser opens the confirmation dialog: the user
        // chooses the folder and name before anything is written to disk.
        setConfirmUrl(url);
      })
    );

    unlisteners.push(
      listen<{ path: string }>('add_torrent_from_cli', (event) => {
        const path = event.payload?.path;
        if (!path) return;
        // Opened by double-clicking a .torrent: add it straight away.
        void invoke('add_torrent', { url: path, savePath: null, paused: false })
          .then(() => {
            void refresh();
            void refreshStats();
            pushToast(t('torrentAdded'), 'success');
          })
          .catch((error) => pushToast(String(error), 'error'));
      })
    );

    unlisteners.push(
      listen('pick_torrent_from_cli', () => {
        // Launched from the desktop action "Open a torrent file": ask for the file.
        void import('@tauri-apps/plugin-dialog').then(async ({ open }) => {
          try {
            const chosen = await open({
              multiple: false,
              filters: [{ name: 'Torrent', extensions: ['torrent'] }],
            });
            if (typeof chosen !== 'string') return;
            await invoke('add_torrent', { url: chosen, savePath: null, paused: false });
            await refresh();
            void refreshStats();
            pushToast(t('torrentAdded'), 'success');
          } catch (error) {
            pushToast(String(error), 'error');
          }
        });
      })
    );

    // ------------------------------------------------------ dropping files
    //
    // `dragDropEnabled: true` in tauri.conf.json makes Tauri capture the OS drag
    // and store the real filesystem paths. Because Tauri takes the drag, the DOM
    // never sees `drop`, so a `window.addEventListener('drop')` handler can never
    // fire — the Tauri 2 API for this is `onDragDropEvent`. The HTML listeners
    // below are kept only as a fallback for builds where Tauri does not intercept
    // the drag; they are harmless when it does.
    const addTorrentPaths = (paths: string[]) => {
      const torrents = paths.filter((path) => path.toLowerCase().endsWith('.torrent'));
      if (torrents.length === 0) return;

      // Every dropped .torrent is added; one failure must not hide the others.
      torrents.forEach((path) => {
        void invoke('add_torrent', { url: path, savePath: null, paused: false })
          .then(() => {
            void refresh();
            void refreshStats();
            pushToast(t('torrentAdded'), 'success');
          })
          .catch((error) => pushToast(String(error), 'error'));
      });
    };

    const onDroppedPaths = (paths: string[]) => {
      setDragActive(false);
      if (paths.length === 0) return;

      addTorrentPaths(paths);

      // A regular file cannot be downloaded — but its path often carries a link
      // the user wants. Offer the add dialog instead of failing silently.
      const other = paths.find((path) => !path.toLowerCase().endsWith('.torrent'));
      if (other) {
        setAddInitialUrl(other);
        setAddOpen(true);
      }
    };

    // The real path in Tauri 2: the OS drag is intercepted by the shell, which
    // reports the absolute paths of what was dropped.
    unlisteners.push(
      getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === 'enter') setDragActive(true);
        else if (event.payload.type === 'leave') setDragActive(false);
        else if (event.payload.type === 'drop') onDroppedPaths(event.payload.paths);
      })
    );

    // Fallback for the DOM events, used only when Tauri does not intercept.
    let dragDepth = 0;
    const onDragEnter = (event: DragEvent) => {
      event.preventDefault();
      dragDepth += 1;
      if (event.dataTransfer?.types.includes('Files')) setDragActive(true);
    };
    const onDragOver = (event: DragEvent) => {
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
    };
    const onDragLeave = (event: DragEvent) => {
      event.preventDefault();
      dragDepth = Math.max(0, dragDepth - 1);
      if (dragDepth === 0) setDragActive(false);
    };
    const onDrop = async (event: DragEvent) => {
      event.preventDefault();
      dragDepth = 0;
      setDragActive(false);

      // Tauri exposes the real filesystem path here; the File API only gives a
      // synthetic name, which is useless for reading the file.
      const files = Array.from(event.dataTransfer?.files ?? []);
      const paths = files
        .map((file) => (file as File & { path?: string }).path)
        .filter((path): path is string => Boolean(path));

      if (paths.length > 0) {
        onDroppedPaths(paths);
      } else {
        // Fallback when WebKit or the OS doesn't expose file.path: read binary directly
        for (const file of files) {
          if (file.name.toLowerCase().endsWith('.torrent')) {
            try {
              const buffer = await file.arrayBuffer();
              const bytes = Array.from(new Uint8Array(buffer));
              await invoke('add_torrent_bytes', {
                bytes,
                name: file.name,
                savePath: null,
                paused: false,
              });
              void refresh();
              void refreshStats();
              pushToast(t('torrentAdded'), 'success');
            } catch (error) {
              pushToast(String(error), 'error');
            }
          }
        }
      }
    };

    window.addEventListener('dragenter', onDragEnter);
    window.addEventListener('dragover', onDragOver);
    window.addEventListener('dragleave', onDragLeave);
    window.addEventListener('drop', onDrop);

    // A DOM removal is synchronous but the listener registry holds promises, so
    // it is wrapped to satisfy the same shape as every `listen()` entry.
    unlisteners.push(
      Promise.resolve(() => {
        window.removeEventListener('dragenter', onDragEnter);
        window.removeEventListener('dragover', onDragOver);
        window.removeEventListener('dragleave', onDragLeave);
        window.removeEventListener('drop', onDrop);
      })
    );

    unlisteners.push(
      listen<{ url: string }>('confirm_download_url', (event) => {
        const url = event.payload?.url;
        if (url) setConfirmUrl(url);
      })
    );

    unlisteners.push(
      listen('open_add_modal', () => {
        setAddInitialUrl(undefined);
        setAddOpen(true);
      })
    );

    unlisteners.push(
      listen('open_video_modal', () => {
        setVideoInitialUrl(undefined);
        setVideoOpen(true);
      })
    );

    unlisteners.push(
      listen('open_about_modal', () => {
        setAboutOpen(true);
      })
    );

    unlisteners.push(
      listen<string>('navigate', (event) => {
        const target = event.payload as NavTab;
        if (['all', 'active', 'completed', 'video', 'settings'].includes(target)) {
          setTab(target);
        }
      })
    );

    unlisteners.push(
      listen<{ reason: string }>('tray_unavailable', () => {
        pushToast(t('trayUnavailable'), 'info');
      })
    );

    // Initial check for any CLI payload captured on startup (e.g. double clicking .torrent in OS)
    void invoke<{ kind: 'Url' | 'Torrent' | 'Video' | 'PickTorrent'; payload?: string } | null>(
      'get_pending_cli'
    )
      .then((pending) => {
        if (!pending) return;
        if (pending.kind === 'Torrent' && pending.payload) {
          void invoke('add_torrent', { url: pending.payload, savePath: null, paused: false })
            .then(() => {
              void refresh();
              void refreshStats();
              pushToast(t('torrentAdded'), 'success');
            })
            .catch((err) => pushToast(String(err), 'error'));
        } else if (pending.kind === 'Url' && pending.payload) {
          setConfirmUrl(pending.payload);
        } else if (pending.kind === 'Video' && pending.payload) {
          setVideoInitialUrl(pending.payload);
          setVideoOpen(true);
        }
      })
      .catch(() => undefined);

    return () => {
      unlisteners.forEach((promise) => promise.then((off) => off()).catch(() => undefined));
    };
  }, [refresh, refreshStats, pushToast, t, settings.notifications]);

  // Poll stats slowly: speeds are already pushed by the engine, this only keeps
  // the disk-free figure and counters fresh without a busy loop.
  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshStats();
    }, 4000);
    return () => window.clearInterval(timer);
  }, [refreshStats]);

  // ------------------------------------------------------------- mutations
  const pause = useCallback(
    async (id: string) => {
      try {
        // A torrent is paused through its session, not the HTTP task table.
        await invoke(id.startsWith('torrent-') ? 'pause_torrent' : 'pause_download', { id });
        await refresh();
      } catch (error) {
        pushToast(`${t('errPauseFailed')}: ${error}`, 'error');
      }
    },
    [refresh, pushToast, t]
  );

  const resume = useCallback(
    async (id: string) => {
      try {
        await invoke(id.startsWith('torrent-') ? 'resume_torrent' : 'resume_download', { id });
        await refresh();
      } catch (error) {
        pushToast(String(error), 'error');
      }
    },
    [refresh, pushToast]
  );

  // Destructive actions go through an explicit confirmation: the row's ✕ used to
  // erase a completed file on a single hover-click, with no undo.
  const [pendingDelete, setPendingDelete] = useState<DownloadItem | null>(null);

  const requestRemove = useCallback((id: string) => {
    const item = downloads.find((entry) => entry.id === id) ?? null;
    setPendingDelete(item);
  }, [downloads]);

  const confirmRemove = useCallback(
    async (deleteFile: boolean) => {
      const target = pendingDelete;
      setPendingDelete(null);
      if (!target) return;
      try {
        if (target.status === 'downloading' || target.status === 'queued') {
          await invoke('cancel_download', { id: target.id });
        }
        await invoke('delete_download', { id: target.id, deleteFile });
        await refresh();
        void refreshStats();
      } catch (error) {
        pushToast(`${t('errDeleteFailed')}: ${error}`, 'error');
      }
    },
    [pendingDelete, refresh, refreshStats, pushToast, t]
  );

  const openFile = useCallback(
    async (path: string) => {
      try {
        await invoke('open_file', { path });
      } catch (error) {
        pushToast(String(error), 'error');
      }
    },
    [pushToast]
  );

  const openFolder = useCallback(
    async (path: string) => {
      try {
        await invoke('open_file_folder', { path });
      } catch (error) {
        pushToast(String(error), 'error');
      }
    },
    [pushToast]
  );

  const pauseAll = useCallback(async () => {
    try {
      await invoke('pause_all');
      await refresh();
      void refreshStats();
    } catch (error) {
      pushToast(String(error), 'error');
    }
  }, [refresh, refreshStats, pushToast]);

  const saveSettings = useCallback(
    async (next: Settings) => {
      setSettings(next);
      if (next.language && next.language !== lang) {
        setLang(next.language as Lang);
      }
      try {
        const persisted = await invoke<Settings>('save_settings', { settings: next });
        setSettings(persisted);
      } catch (error) {
        pushToast(String(error), 'error');
      }
    },
    [lang, setLang, pushToast]
  );

  const pickSpeedMode = useCallback((mode: SpeedMode) => {
    setSpeedMode(mode);
    const wanted = CONNECTIONS_BY_MODE[mode];
    setSettings((current) => {
      if (current.default_connections === wanted) return current;
      const next = { ...current, default_connections: wanted };
      void invoke<Settings>('save_settings', { settings: next })
        .then(setSettings)
        .catch(() => undefined);
      return next;
    });
  }, []);

  // ---------------------------------------------------------------- filters
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();

    return downloads.filter((item) => {
      if (tab === 'active' && item.status !== 'downloading' && item.status !== 'queued') return false;
      if (tab === 'completed' && item.status !== 'completed') return false;
      if (tab === 'video' && item.category !== 'Video' && item.category !== 'Audio') return false;
      if (category !== 'all' && item.category !== category) return false;
      if (!needle) return true;
      return (
        item.filename.toLowerCase().includes(needle) || item.url.toLowerCase().includes(needle)
      );
    });
  }, [downloads, tab, category, query]);

  const activeCount = useMemo(
    () => downloads.filter((item) => item.status === 'downloading').length,
    [downloads]
  );
  const completedCount = useMemo(
    () => downloads.filter((item) => item.status === 'completed').length,
    [downloads]
  );

  // ------------------------------------------------------------- bulk actions
  const toggleSelect = useCallback((id: string) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const allVisibleSelected =
    visible.length > 0 && visible.every((item) => selectedIds.has(item.id));

  const toggleSelectAll = useCallback(() => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (visible.every((item) => next.has(item.id))) {
        visible.forEach((item) => next.delete(item.id));
      } else {
        visible.forEach((item) => next.add(item.id));
      }
      return next;
    });
  }, [visible]);

  const confirmBatchDelete = useCallback(async () => {
    const ids = Array.from(selectedIds);
    setBatchDeleteOpen(false);
    if (ids.length === 0) return;
    try {
      await invoke('delete_downloads', { ids, deleteFiles: batchDeleteWithFiles });
      setSelectedIds(new Set());
      await refresh();
      void refreshStats();
    } catch (error) {
      pushToast(`${t('errDeleteFailed')}: ${error}`, 'error');
    }
  }, [selectedIds, batchDeleteWithFiles, refresh, refreshStats, pushToast, t]);

  const restart = useCallback(
    async (id: string) => {
      try {
        await invoke('restart_download', { id });
        await refresh();
        void refreshStats();
      } catch (error) {
        pushToast(String(error), 'error');
      }
    },
    [refresh, refreshStats, pushToast]
  );

  const requestUpdateUrl = useCallback(
    (id: string) => {
      const item = downloads.find((entry) => entry.id === id);
      if (!item) return;
      setUpdateUrlItem(item);
      setUpdateUrlValue(item.url);
    },
    [downloads]
  );

  const submitUpdateUrl = useCallback(async () => {
    const item = updateUrlItem;
    const trimmed = updateUrlValue.trim();
    if (!item || !trimmed) return;
    try {
      await invoke('update_download_url', { id: item.id, newUrl: trimmed });
      // Updating the link is only useful if the transfer picks it up again.
      if (item.status === 'error' || item.status === 'paused') {
        await invoke(item.id.startsWith('torrent-') ? 'resume_torrent' : 'resume_download', {
          id: item.id,
        });
      }
      setUpdateUrlItem(null);
      await refresh();
      pushToast(t('updateUrlSuccess'), 'success');
    } catch (error) {
      pushToast(String(error), 'error');
    }
  }, [updateUrlItem, updateUrlValue, refresh, pushToast, t]);

  const toggleQueue = useCallback(
    async (id: string) => {
      const item = downloads.find((entry) => entry.id === id);
      if (!item) return;
      try {
        await invoke(item.status === 'queued' ? 'unqueue_downloads' : 'queue_downloads', {
          ids: [id],
        });
        await refresh();
        void refreshStats();
      } catch (error) {
        pushToast(String(error), 'error');
      }
    },
    [downloads, refresh, refreshStats, pushToast]
  );

  // A selected row that disappears elsewhere must not stay in the selection.
  useEffect(() => {
    setSelectedIds((current) => {
      const live = new Set(downloads.map((item) => item.id));
      const next = new Set(Array.from(current).filter((id) => live.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [downloads]);

  // ------------------------------------------------------------------ render
  // Map the design-guide rail tabs onto the existing filter state, so the visual
  // shell changes without touching how downloads are selected.
  const railTab: RailTab =
    tab === 'settings'
      ? 'settings'
      : tab === 'queue'
        ? 'queue'
        : tab === 'completed'
          ? 'completed'
          : tab === 'active'
            ? 'active'
            : 'all';

  const queuedCount = useMemo(
    () => downloads.filter((item) => item.status === 'queued').length,
    [downloads]
  );

  const counts = {
    all: downloads.length,
    active: activeCount,
    queued: queuedCount,
    completed: completedCount,
  };

  // Speed history for the 60-second pulse line: sampled once a second.
  const [speedHistory, setSpeedHistory] = useState<number[]>([]);
  useEffect(() => {
    setSpeedHistory((current) => {
      const next = [...current, stats.total_speed_bps];
      return next.length > 60 ? next.slice(next.length - 60) : next;
    });
  }, [stats.total_speed_bps]);

  const quickAdd = useCallback(
    async (url: string) => {
      try {
        // A magnet link or .torrent file goes to the BitTorrent engine, which has
        // its own command: there are no byte ranges or connections to multiply.
        const lower = url.trim().toLowerCase();
        const isTorrent =
          lower.startsWith('magnet:') ||
          lower.endsWith('.torrent') ||
          (url.trim().length === 40 && /^[0-9a-f]+$/i.test(url.trim()));

        if (isTorrent) {
          await invoke('add_torrent', { url, savePath: null, paused: false });
          await refresh();
          void refreshStats();
          return;
        }

        await invoke('add_download', {
          // Tauri maps this key onto the Rust parameter name `req`. It used to say
          // `request`, so every paste-to-download failed with
          // "invalid args `req` for command `add_download`: command add_download
          // missing required key req". The other two call sites already used `req`.
          req: {
            url,
            filename: null,
            save_path: null,
            connections: settings.default_connections ?? null,
            category: null,
            start_now: true,
            referer: null,
          },
        });
        await refresh();
        void refreshStats();
      } catch (error) {
        pushToast(`${t('errAddFailed')}: ${error}`, 'error');
      }
    },
    [refresh, refreshStats, pushToast, t, settings.default_connections]
  );

  // Guide shortcut: Space pauses/resumes. With several lanes it acts on all of
  // them, which is the only sensible reading of a global key.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
      if (event.code !== 'Space') return;
      event.preventDefault();
      if (activeCount > 0) void pauseAll();
      else {
        const paused = downloads.filter((item) => item.status === 'paused');
        paused.forEach((item) => void resume(item.id));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [activeCount, downloads, pauseAll, resume]);

  const selectRail = useCallback((next: RailTab) => {
    setCategory('all');
    setTab(next);
  }, []);

  return (
    <div className="h-screen flex flex-col overflow-hidden bg-base text-ink">
      <CommandBar
        searchQuery={query}
        onSearch={setQuery}
        onQuickAdd={quickAdd}
        onOpenAddDialog={(url) => {
          setAddInitialUrl(url);
          setAddOpen(true);
        }}
        speedMode={speedMode}
        onSpeedMode={pickSpeedMode}
        onOpenSettings={() => setTab(tab === 'settings' ? 'all' : 'settings')}
        settingsActive={tab === 'settings'}
      />

      {/* Category chips + counts, under the command bar (guide §03 item 5) */}
      {tab !== 'settings' && (
        <div className="land-filters">
          {FILTERS.map((filter) => {
            const active = category === filter.id;
            return (
              <button
                key={filter.id}
                type="button"
                onClick={() => setCategory(filter.id)}
                className="land-chip"
                data-active={active}
                aria-pressed={active}
              >
                {t(filter.key)}
              </button>
            );
          })}
          <span className="land-filters-spacer" />
          <div className="land-view-toggle" role="group" aria-label={viewMode === 'grid' ? t('viewGrid') : t('viewList')}>
            <button
              type="button"
              aria-pressed={viewMode === 'list'}
              onClick={() => setViewModePersisted('list')}
              title={t('viewList')}
              aria-label={t('viewList')}
            >
              <List size={13} />
            </button>
            <button
              type="button"
              aria-pressed={viewMode === 'grid'}
              onClick={() => setViewModePersisted('grid')}
              title={t('viewGrid')}
              aria-label={t('viewGrid')}
            >
              <LayoutGrid size={13} />
            </button>
          </div>
        </div>
      )}

      {tab !== 'settings' && selectedIds.size > 0 && (
        <div className="bulk-bar">
          <span className="bulk-bar-count">
            {t('selectedCount').replace('{count}', String(selectedIds.size))}
          </span>
          <button
            type="button"
            onClick={toggleSelectAll}
            className="btn-ghost h-7 px-2.5 text-[13px] font-bold"
          >
            {allVisibleSelected ? t('deselectAll') : t('selectAll')}
          </button>
          <span className="flex-1" />
          <button
            type="button"
            onClick={() => setBatchDeleteOpen(true)}
            className="h-7 px-2.5 rounded text-[13px] font-bold bg-rose-500/90 hover:bg-rose-500 text-white transition-colors inline-flex items-center gap-1.5"
          >
            <Trash2 size={12} />
            {t('batchDelete')}
          </button>
          <button
            type="button"
            onClick={() => setSelectedIds(new Set())}
            className="btn-ghost h-7 px-2.5 text-[13px] font-bold"
          >
            {t('cancel')}
          </button>
        </div>
      )}

      <div className="land-content">
        <IconRail
          current={railTab}
          onSelect={selectRail}
          counts={counts}
          onOpenAbout={() => setAboutOpen(true)}
        />

        <div className="land-main">
          {tab === 'settings' ? (
            <main className="flex-1 overflow-y-auto px-4 py-3.5">
              {settingsLoaded ? (
                <SettingsView settings={settings} onChange={saveSettings} />
              ) : (
                <div className="flex items-center justify-center py-16 text-[12px] text-inkMuted">
                  <Loader2 size={16} className="anim-spin me-2" />
                  {t('probing')}
                </div>
              )}
            </main>
          ) : tab === 'queue' ? (
            <main className="flex-1 overflow-y-auto px-4 py-3.5">
              <QueuePanel downloads={downloads} onQueuedChange={refresh} />
            </main>
          ) : !settingsLoaded ? (
            <div className="flex-1" />
          ) : visible.length === 0 ? (
            <div className="land-empty">
              <div className="land-empty-box">
                <div className="land-empty-ghosts" aria-hidden="true">
                  {Array.from({ length: 6 }).map((_, index) => (
                    <span key={index} className="land-empty-ghost" />
                  ))}
                </div>
                <span className="land-empty-title">
                  {query || category !== 'all' ? t('emptyFiltered') : t('emptyTitle')}
                </span>
                <span className="land-empty-text">
                  {t('emptyHint')}{' '}
                  <kbd className="land-empty-kbd">Ctrl</kbd>{' '}
                  <kbd className="land-empty-kbd">V</kbd> {t('emptyHintEnd')}
                </span>
                {!query && category === 'all' && (
                  <div className="land-empty-actions">
                    <button
                      type="button"
                      className="land-empty-action"
                      onClick={() => {
                        setAddInitialUrl(undefined);
                        setAddOpen(true);
                      }}
                    >
                      <Link2 size={13} />
                      {t('emptyPasteUrl')}
                    </button>
                    <button
                      type="button"
                      className="land-empty-action"
                      onClick={() => {
                        setVideoInitialUrl(undefined);
                        setVideoOpen(true);
                      }}
                    >
                      <Film size={13} />
                      {t('emptyVideoDownload')}
                    </button>
                  </div>
                )}
              </div>
            </div>
          ) : viewMode === 'grid' ? (
            <div className="lane-grid">
              {visible.map((item) => (
                <LaneTile
                  key={item.id}
                  item={item}
                  onPause={pause}
                  onResume={resume}
                  onDelete={requestRemove}
                  onOpenFolder={openFolder}
                  onOpenFile={openFile}
                  onOpenDetails={(id) =>
                    void invoke('open_transfer_window', { id, alwaysOnTop: false }).catch(
                      () => undefined
                    )
                  }
                />
              ))}
            </div>
          ) : (
            <div className="lane-list">
              {visible.map((item) => (
                <Lane
                  key={item.id}
                  item={item}
                  selected={selectedIds.has(item.id)}
                  onToggleSelect={toggleSelect}
                  onPause={pause}
                  onResume={resume}
                  onDelete={requestRemove}
                  onOpenFile={openFile}
                  onOpenFolder={openFolder}
                  onRestart={restart}
                  onUpdateUrl={requestUpdateUrl}
                  onToggleQueue={toggleQueue}
                  isQueued={item.status === 'queued'}
                  onOpenDetails={(id) =>
                    void invoke('open_transfer_window', { id, alwaysOnTop: false }).catch(
                      () => undefined
                    )
                  }
                />
              ))}
            </div>
          )}
        </div>
      </div>

      <StatusBar
        speedBps={stats.total_speed_bps}
        activeCount={stats.active_count}
        queuedCount={stats.queued_count}
        diskFree={stats.disk_free_bytes}
        history={speedHistory}
      />

      {/* Dialogs */}
      <DownloadConfirmDialog
        open={!!confirmUrl}
        url={confirmUrl ?? ''}
        onCancel={() => setConfirmUrl(null)}
        onStarted={() => {
          setConfirmUrl(null);
          void refresh();
        }}
        settings={settings}
      />

      <AddDownloadModal
        open={addOpen}
        onClose={() => setAddOpen(false)}
        onAdded={() => void refresh()}
        settings={settings}
        initialUrl={addInitialUrl}
      />

      <VideoDownloaderModal
        open={videoOpen}
        onClose={() => setVideoOpen(false)}
        onAdded={() => void refresh()}
        initialUrl={videoInitialUrl}
      />

      <AboutModal open={aboutOpen} onClose={() => setAboutOpen(false)} />

      <Modal
        open={!!pendingDelete}
        onClose={() => setPendingDelete(null)}
        title={t('removeAndDelete')}
        icon={<Trash2 size={17} />}
        width="sm"
        footer={
          <>
            <button
              type="button"
              onClick={() => setPendingDelete(null)}
              className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold"
            >
              {t('cancel')}
            </button>
            <button
              type="button"
              onClick={() => void confirmRemove(false)}
              className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold"
            >
              {t('remove')}
            </button>
            <button
              type="button"
              onClick={() => void confirmRemove(true)}
              className="h-9 px-3.5 rounded text-[12.5px] font-bold bg-rose-500/90 hover:bg-rose-500 text-white transition-colors"
            >
              {t('removeAndDelete')}
            </button>
          </>
        }
      >
        <p className="text-[12px] text-inkSoft leading-relaxed">
          {pendingDelete ? (
            <>
              <span className="font-bold text-ink always-ltr block truncate" dir="ltr">
                {pendingDelete.filename}
              </span>
              <span className="block mt-2 text-inkMuted">
                {formatBytes(pendingDelete.total_size)}
              </span>
            </>
          ) : null}
        </p>
      </Modal>

      <Modal
        open={batchDeleteOpen}
        onClose={() => setBatchDeleteOpen(false)}
        title={t('batchDelete')}
        subtitle={t('batchDeleteConfirm').replace('{count}', String(selectedIds.size))}
        icon={<Trash2 size={17} />}
        width="sm"
        footer={
          <>
            <button
              type="button"
              onClick={() => setBatchDeleteOpen(false)}
              className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold"
            >
              {t('cancel')}
            </button>
            <button
              type="button"
              onClick={() => void confirmBatchDelete()}
              className="h-9 px-3.5 rounded text-[12.5px] font-bold bg-rose-500/90 hover:bg-rose-500 text-white transition-colors"
            >
              {t('batchDelete')}
            </button>
          </>
        }
      >
        <label className="flex items-center gap-2.5 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={batchDeleteWithFiles}
            onChange={(event) => setBatchDeleteWithFiles(event.target.checked)}
            className="w-4 h-4 rounded border border-line accent-rose-500"
          />
          <span className="text-[13.5px] font-semibold text-inkSoft">
            {t('batchDeleteWithFiles')}
          </span>
        </label>
      </Modal>

      <Modal
        open={!!updateUrlItem}
        onClose={() => setUpdateUrlItem(null)}
        title={t('updateUrlTitle')}
        subtitle={t('updateUrlHint')}
        icon={<Link2 size={17} />}
        width="sm"
        footer={
          <>
            <button
              type="button"
              onClick={() => setUpdateUrlItem(null)}
              className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold"
            >
              {t('cancel')}
            </button>
            <button
              type="button"
              disabled={!updateUrlValue.trim()}
              onClick={() => void submitUpdateUrl()}
              className="btn-primary h-9 px-4 text-[12.5px] font-bold disabled:opacity-40"
            >
              {t('updateUrl')}
            </button>
          </>
        }
      >
        <input
          type="text"
          value={updateUrlValue}
          onChange={(event) => setUpdateUrlValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') void submitUpdateUrl();
          }}
          className="field always-ltr"
          dir="ltr"
          spellCheck={false}
          aria-label={t('updateUrlTitle')}
        />
      </Modal>

      <ToastStack toasts={toasts} onDismiss={dismissToast} />

      {/* Drop target feedback. Without it a dragged file gives no hint that the
          window accepts it. */}
      {dragActive ? (
        <div className="drop-veil" aria-hidden="true">
          <div className="drop-card">
            <span className="drop-title">{t('dropTorrentTitle')}</span>
            <span className="drop-hint">{t('dropTorrentHint')}</span>
          </div>
        </div>
      ) : null}

      {/* Diagnostics stays mounted for the settings tab to reuse. */}
      <span className="hidden">{diagnostics?.desktop}</span>
    </div>
  );
};

export const App: React.FC = () => {
  const [initialLang, setInitialLang] = useState<string | undefined>(undefined);
  const [ready, setReady] = useState(false);

  // Read the persisted language before painting so there is no RTL flash.
  useEffect(() => {
    invoke<Settings>('get_settings')
      .then((settings) => setInitialLang(settings.language || undefined))
      .catch(() => setInitialLang(undefined))
      .finally(() => setReady(true));
  }, []);

  if (!ready) {
    return (
      <div className="h-screen grid place-items-center bg-base">
        <div className="flex flex-col items-center gap-3">
          <div className="w-10 h-10 rounded-2xl bg-panel border border-line grid place-items-center text-accent2">
            <SettingsIcon size={18} className="anim-spin" />
          </div>
          <span className="text-[13.5px] text-inkMuted">ZDM</span>
        </div>
      </div>
    );
  }

  return (
    <I18nProvider initial={initialLang}>
      <Shell />
    </I18nProvider>
  );
};
