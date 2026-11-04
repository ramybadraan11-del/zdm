//! ZDM Floating Download Prompt
//! Developed by Ramy Badraan
//!
//! One frameless window per decision. A direct file and a video page are shown
//! differently because the user is answering a different question:
//!
//!   file  → "how many connections, where do I save it?"
//!   video → "which quality?"
//!
//! Design intent: solid header strip with the brand mark and the host, a body
//! that changes shape with the mode, and a footer with exactly one primary
//! action. Nothing floats without a container, nothing is truncated mid-word, and
//! the window height is derived from the content so no mode ever shows a scrollbar.

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  ArrowDownToLine,
  Folder,
  X,
  Clock,
  Loader2,
  AlertTriangle,
  Repeat,
  Layers,
  Music,
  Video,
  Film,
  FileDown,
  Network,
  RotateCcw,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { Settings, UrlMetadata, VideoInfo } from '../types';
import { formatBytes, hostOf } from '../utils/format';

/** A magnet's display name comes from its `dn` parameter. */
function guessTorrentName(url: string): string {
  const match = url.match(/[?&]dn=([^&]+)/);
  if (match) {
    try {
      return decodeURIComponent(match[1]);
    } catch {
      return match[1];
    }
  }
  if (url.toLowerCase().endsWith('.torrent')) return url.split('/').pop() ?? 'torrent';
  const hash = url.split(':').pop() ?? url;
  return `${hash.slice(0, 16)}.torrent`;
}

/** A torrent has no known size and no ranges; say so rather than fake numbers. */
function torrentAsMeta(url: string): UrlMetadata {
  return {
    url,
    filename: guessTorrentName(url),
    total_size: 0,
    supports_range: false,
    mime_type: 'application/x-bittorrent',
    category: 'Torrent',
  };
}

type Probe =
  | { kind: 'busy' }
  | { kind: 'ok'; meta: UrlMetadata }
  | { kind: 'error'; message: string };

/** Wrap a video's metadata in the file shape used by the fact strip. */
function videoAsMeta(info: VideoInfo): UrlMetadata {
  const total = info.formats
    .filter((f) => !f.is_audio)
    .map((f) => f.filesize ?? 0)
    .reduce((best, size) => Math.max(best, size), 0);

  return {
    url: info.webpage_url,
    filename: info.title,
    total_size: total,
    supports_range: true,
    mime_type: 'video/*',
    category: 'Video',
  };
}

export const DownloadPrompt: React.FC = () => {
  const { t } = useI18n();

  const params = useMemo(() => new URLSearchParams(window.location.search), []);
  const url = params.get('url') ?? '';
  const referer = params.get('referer') ?? undefined;
  // Classified by the Rust side before the window opens: a video page needs a
  // quality list, a direct file needs size and resume facts.
  const mode = params.get('mode') ?? 'file';
  const isVideo = mode === 'video';
  const isTorrent = mode === 'torrent';

  const [probe, setProbe] = useState<Probe>({ kind: 'busy' });
  const [filename, setFilename] = useState('');
  const [folder, setFolder] = useState('');
  const [connections, setConnections] = useState(16);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<false | 'now' | 'later'>(false);

  // Video mode only.
  const [video, setVideo] = useState<VideoInfo | null>(null);
  const [formatId, setFormatId] = useState<string | null>(null);

  const host = useMemo(() => (url ? hostOf(url) : ''), [url]);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const settings = await invoke<Settings>('get_settings');
        if (cancelled) return;
        // Folder starts empty: the backend routes by file type unless the user
        // explicitly picks one.
        setConnections(settings.default_connections ?? 16);
      } catch {
        /* defaults are fine */
      }

      // A magnet link has nothing to probe: skip straight to a ready state.
      if (isTorrent) {
        setFilename(guessTorrentName(url));
        setProbe({ kind: 'ok', meta: torrentAsMeta(url) });
        return;
      }

      // A video page has no Content-Length to probe: it needs the analyser.
      if (isVideo) {
        try {
          const info = await invoke<VideoInfo>('query_video_info', { url });
          if (cancelled) return;
          setVideo(info);
          setFilename(info.title);
          const best = info.formats.find((f) => !f.is_audio) ?? info.formats[0];
          if (best) setFormatId(best.format_id);
          setProbe({ kind: 'ok', meta: videoAsMeta(info) });
        } catch (err) {
          if (!cancelled) setProbe({ kind: 'error', message: String(err) });
        }
        return;
      }

      try {
        const meta = await invoke<UrlMetadata>('probe_url', { url });
        if (cancelled) return;
        setProbe({ kind: 'ok', meta });
        setFilename(meta.filename);
      } catch (err) {
        if (!cancelled) setProbe({ kind: 'error', message: String(err) });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [url, isVideo]);

  const cancel = useCallback(async () => {
    try {
      await invoke('close_prompt');
    } catch {
      window.close();
    }
  }, []);

  const pickFolder = async () => {
    try {
      const chosen = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: folder || undefined,
      });
      if (typeof chosen === 'string') setFolder(chosen);
    } catch (err) {
      // Surface the failure instead of swallowing it: a denied dialog (e.g. a
      // missing window capability) used to make this button look dead.
      setError(String(err));
    }
  };

  const submit = async (when: 'now' | 'later') => {
    setBusy(when);
    setError(null);

    try {
      if (isTorrent) {
        await invoke('add_torrent', {
          url,
          savePath: folder.trim() || null,
          paused: when === 'later',
        });
        await invoke('close_prompt').catch(() => undefined);
        return;
      }

      if (isVideo) {
        const chosen = video?.formats.find((f) => f.format_id === formatId);
        if (!chosen) throw new Error(t('videoNoFormats'));

        // The transfer window is opened by the backend and survives the prompt:
        // the download continues where the user started it.
        await invoke('start_video_download', {
          url: video?.webpage_url || url,
          formatId: chosen.format_id,
          title: video?.title ?? filename,
          ext: chosen.ext,
          isAudio: chosen.is_audio,
          estSize: chosen.filesize ?? null,
          customFilename: filename.trim() || null,
          forceOverwrite: false,
          openWindow: true,
          folder: folder.trim() || null,
        });

        await invoke('close_prompt').catch(() => undefined);
        return;
      }

      await invoke(when === 'now' ? 'start_download_from_prompt' : 'queue_download_from_prompt', {
        url,
        filename: filename.trim() || null,
        folder: folder.trim() || null,
        connections,
        referer: referer ?? null,
      });
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  const meta = probe.kind === 'ok' ? probe.meta : null;
  const total = meta?.total_size ?? 0;
  const picking = probe.kind === 'busy';

  return (
    <div className="dl-window" data-mode={mode}>
      {/* ── Header: identity + what was captured ───────────────────────── */}
      <header className="dl-head">
        <div className="dl-mark" aria-hidden="true">
          {isVideo ? <Film size={14} /> : isTorrent ? <Network size={14} /> : <FileDown size={14} />}
        </div>
        <div className="dl-head-text">
          <span className="dl-title">
            {isVideo ? t('promptVideoTitle') : isTorrent ? t('promptTorrentTitle') : t('promptFileTitle')}
          </span>
          <span className="dl-host mono" dir="ltr">
            {host}
          </span>
        </div>
        <button
          type="button"
          onClick={cancel}
          className="dl-close"
          title={t('cancel')}
          aria-label={t('cancel')}
        >
          <X size={13} />
        </button>
      </header>

      {/* ── Body ───────────────────────────────────────────────────────── */}
      <div className="dl-body">
        {isVideo && (
          <div className="dl-thumb" data-empty={!video?.thumbnail}>
            {video?.thumbnail ? (
              <img
                src={video.thumbnail}
                alt=""
                className="dl-thumb-img"
                loading="eager"
                onError={(event) => {
                  // A dead CDN link must not leave a broken-image frame.
                  (event.currentTarget as HTMLImageElement).style.display = 'none';
                  const parent = (event.currentTarget as HTMLImageElement).parentElement;
                  if (parent) parent.dataset.empty = 'true';
                }}
              />
            ) : (
              <span className="dl-thumb-fallback">
                <Film size={18} />
              </span>
            )}
            {video?.duration_seconds ? (
              <span className="dl-thumb-time mono" dir="ltr">
                {formatClock(video.duration_seconds)}
              </span>
            ) : null}
          </div>
        )}
        {probe.kind === 'error' && (
          <div className="dl-error">
            <AlertTriangle size={13} />
            <span>{probe.message}</span>
          </div>
        )}

        {/* Name — editable in both modes, sized to fit the window. */}
        <label className="dl-field">
          <span className="dl-label">{t('promptName')}</span>
          <input
            type="text"
            value={filename}
            onChange={(event) => setFilename(event.target.value)}
            placeholder={picking ? t('promptAnalysing') : t('promptName')}
            className="dl-input"
            dir="auto"
            disabled={picking}
          />
        </label>

        {/* Facts: one row of small chips, never more than one line. */}
        <div className="dl-facts">
          {picking ? (
            <span className="dl-chip dl-chip-muted">
              <Loader2 size={10} className="animate-spin" />
              {t('promptAnalysing')}
            </span>
          ) : (
            <>
              {total > 0 && <span className="dl-chip mono">{formatBytes(total)}</span>}
              {isVideo && video?.duration_seconds ? (
                <span className="dl-chip mono">{formatClock(video.duration_seconds)}</span>
              ) : null}
              {!isVideo && meta?.supports_range && (
                <span className="dl-chip">
                  <Repeat size={9} />
                  {t('promptResumable')}
                </span>
              )}
              {!isVideo && meta && !meta.supports_range && (
                <span className="dl-chip dl-chip-warning">
                  <AlertTriangle size={9} />
                  {t('promptNotResumable')}
                </span>
              )}
              {!isVideo && meta?.category && (
                <span className="dl-chip dl-chip-muted">{meta.category}</span>
              )}
              {isVideo && video?.uploader && (
                <span className="dl-chip dl-chip-muted dl-chip-clip">{video.uploader}</span>
              )}
            </>
          )}
        </div>

        {/* Mode-specific control. */}
        {isVideo ? (
          <div className="dl-section">
            <span className="dl-label">{t('videoQuality')}</span>
            <div className="dl-qualities" role="radiogroup" aria-label={t('videoQuality')}>
              {picking && !video ? (
                <span className="dl-quality-empty">
                  <Loader2 size={12} className="animate-spin" />
                  {t('promptAnalysing')}
                </span>
              ) : (
                (video?.formats ?? []).map((format) => {
                  const active = format.format_id === formatId;
                  return (
                    <button
                      key={format.format_id}
                      type="button"
                      role="radio"
                      aria-checked={active}
                      onClick={() => setFormatId(format.format_id)}
                      className="dl-quality"
                      data-active={active}
                    >
                      {format.is_audio ? (
                        <Music size={11} className="dl-q-icon" />
                      ) : (
                        <Video size={11} className="dl-q-icon" />
                      )}
                      <span className="dl-q-main">
                        {format.is_audio
                          ? t('videoAudioOnly')
                          : (format.resolution ?? format.format_id)}
                      </span>
                      <span className="dl-q-side mono" dir="ltr">
                        {format.ext.toUpperCase()}
                        {format.filesize ? ` · ${formatBytes(format.filesize)}` : ''}
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </div>
        ) : (
          <div className="dl-section">
            <div className="dl-row">
              <span className="dl-label">
                <Layers size={10} />
                {t('confirmConnections')}
              </span>
              <input
                type="range"
                min={1}
                max={32}
                value={connections}
                onChange={(event) => setConnections(Number(event.target.value))}
                className="dl-range"
                aria-label={t('confirmConnections')}
              />
              <span className="dl-range-value mono" dir="ltr">
                {connections}
              </span>
            </div>
          </div>
        )}

        {/* Destination — shared by both modes. */}
        <div className="dl-section">
          <span className="dl-label">
            <Folder size={10} />
            {t('promptFolder')}
          </span>
          <div className="dl-folder">
            <span className="dl-folder-path mono" dir="ltr" title={folder || t('folderAuto')}>
              {folder || t('folderAuto')}
            </span>
            {folder && (
              <button
                type="button"
                onClick={() => setFolder('')}
                className="dl-folder-btn"
                title={t('folderAuto')}
                aria-label={t('folderAuto')}
              >
                <RotateCcw size={11} />
              </button>
            )}
            <button type="button" onClick={pickFolder} className="dl-folder-btn">
              {t('promptChange')}
            </button>
          </div>
          {!folder && <div className="dl-hint">{t('folderAutoHint')}</div>}
        </div>

        {error && (
          <div className="dl-error">
            <AlertTriangle size={13} />
            <span>{error}</span>
          </div>
        )}
      </div>

      {/* ── Footer: one primary action ─────────────────────────────────── */}
      <footer className="dl-foot">
        <button type="button" onClick={cancel} className="dl-btn dl-btn-ghost">
          {t('cancel')}
        </button>
        <button
          type="button"
          onClick={() => void submit('later')}
          disabled={picking || busy !== false}
          className="dl-btn dl-btn-ghost"
        >
          {busy === 'later' ? (
            <Loader2 size={12} className="animate-spin" />
          ) : (
            <Clock size={12} />
          )}
          {t('confirmLater')}
        </button>
        <button
          type="button"
          onClick={() => void submit('now')}
          disabled={picking || busy !== false || (isVideo && !formatId)}
          className="dl-btn dl-btn-primary"
        >
          {busy === 'now' ? (
            <Loader2 size={12} className="animate-spin" />
          ) : (
            <ArrowDownToLine size={12} />
          )}
          {t('confirmStart')}
        </button>
      </footer>
    </div>
  );
};

/** Seconds as m:ss (or h:mm:ss), matching the transfer window. */
function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const pad = (value: number) => value.toString().padStart(2, '0');
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(rest)}` : `${minutes}:${pad(rest)}`;
}
