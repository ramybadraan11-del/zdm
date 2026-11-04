//! ZDM Video Download Dialog
//! Developed by Ramy Badraan

import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Film, Loader2, AlertTriangle, Search, Music, Video, Download } from 'lucide-react';
import { Modal } from './ui';
import { useI18n } from '../i18n';
import { PlaylistInfo, VideoFormat, VideoInfo } from '../types';
import { formatBytes, formatMediaLength } from '../utils/format';

interface Props {
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
  initialUrl?: string;
}

type Analysis =
  | { kind: 'idle' }
  | { kind: 'busy' }
  | { kind: 'ok'; info: VideoInfo }
  | { kind: 'playlist'; info: PlaylistInfo }
  | { kind: 'error'; message: string };

export const VideoDownloaderModal: React.FC<Props> = ({ open, onClose, onAdded, initialUrl }) => {
  const { t } = useI18n();
  const [url, setUrl] = useState(initialUrl ?? '');
  const [analysis, setAnalysis] = useState<Analysis>({ kind: 'idle' });
  const [selected, setSelected] = useState<string | null>(null);
  const [selectedHeight, setSelectedHeight] = useState<number | null>(null);
  const [busyIsPlaylist, setBusyIsPlaylist] = useState(false);
  const [customName, setCustomName] = useState('');
  const [overwrite, setOverwrite] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setUrl(initialUrl ?? '');
    setSelected(null);
    setSelectedHeight(null);
    setCustomName('');
    setOverwrite(false);
    setSubmitError(null);
    setSubmitting(false);

    // The extension usually hands us a URL: analyse straight away.
    if (initialUrl && /^https?:\/\//i.test(initialUrl)) {
      void analyse(initialUrl);
    } else {
      setAnalysis({ kind: 'idle' });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initialUrl]);

  async function analyse(target?: string) {
    const value = (target ?? url).trim();
    if (!/^https?:\/\/\S+$/i.test(value)) {
      setAnalysis({ kind: 'error', message: t('invalidUrl') });
      return;
    }

    setAnalysis({ kind: 'busy' });
    setBusyIsPlaylist(/[?&]list=/i.test(value));
    setSelected(null);
    setSelectedHeight(null);
    setSubmitError(null);

    try {
      // A `list=` parameter means the user wants the whole playlist, which is
      // also how yt-dlp reads the same URL.
      if (/[?&]list=/i.test(value)) {
        const info = await invoke<PlaylistInfo>('query_playlist_info', { url: value });
        setAnalysis({ kind: 'playlist', info });
        setSelectedHeight(info.heights[0] ?? null);
      } else {
        const info = await invoke<VideoInfo>('query_video_info', { url: value });
        setAnalysis({ kind: 'ok', info });
        setCustomName(info.title);
        const first = info.formats.find((format) => !format.is_audio) ?? info.formats[0];
        if (first) setSelected(first.format_id);
      }
    } catch (error) {
      setAnalysis({ kind: 'error', message: String(error) });
    }
  }

  const chosen: VideoFormat | undefined =
    analysis.kind === 'ok'
      ? analysis.info.formats.find((format) => format.format_id === selected)
      : undefined;

  const canSubmit =
    analysis.kind === 'ok'
      ? Boolean(chosen)
      : analysis.kind === 'playlist'
        ? selectedHeight != null
        : false;

  const submit = async () => {
    if (analysis.kind === 'playlist') {
      if (selectedHeight == null) return;
      setSubmitting(true);
      setSubmitError(null);

      try {
        await invoke('start_playlist_download', {
          entries: analysis.info.entries.map((entry) => ({ url: entry.url, title: entry.title })),
          height: selectedHeight,
          folder: null,
        });
        onAdded();
        onClose();
      } catch (error) {
        setSubmitError(String(error));
      } finally {
        setSubmitting(false);
      }
      return;
    }

    if (analysis.kind !== 'ok' || !chosen) return;
    setSubmitting(true);
    setSubmitError(null);

    try {
      await invoke('start_video_download', {
        url: analysis.info.webpage_url || url.trim(),
        formatId: chosen.format_id,
        title: analysis.info.title,
        ext: chosen.ext,
        isAudio: chosen.is_audio,
        estSize: chosen.filesize ?? null,
        customFilename: customName.trim() || null,
        forceOverwrite: overwrite,
        folder: null,
      });
      onAdded();
      onClose();
    } catch (error) {
      setSubmitError(String(error));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('videoTitle')}
      subtitle={t('navVideo')}
      icon={<Film size={17} />}
      width="lg"
      locked={submitting}
      footer={
        <>
          <button type="button" onClick={onClose} disabled={submitting} className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold">
            {t('cancel')}
          </button>
          <button
            type="button"
            onClick={submit}
            disabled={!canSubmit || submitting}
            className="btn-primary h-9 px-4 text-[12.5px] flex items-center gap-1.5"
          >
            {submitting ? <Loader2 size={14} className="anim-spin" /> : <Download size={14} />}
            {analysis.kind === 'playlist' ? t('videoPlaylistDownload') : t('videoDownload')}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        {/* URL + analyse */}
        <div className="flex gap-2">
          <input
            type="url"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void analyse();
            }}
            placeholder={t('videoPlaceholder')}
            className="field always-ltr flex-1"
            dir="ltr"
            spellCheck={false}
            autoComplete="off"
          />
          <button
            type="button"
            onClick={() => void analyse()}
            disabled={analysis.kind === 'busy'}
            className="btn-ghost h-10 px-3 flex items-center gap-1.5 text-[12px] font-bold shrink-0"
          >
            {analysis.kind === 'busy' ? <Loader2 size={14} className="anim-spin" /> : <Search size={14} />}
            {t('videoAnalyse')}
          </button>
        </div>

        {analysis.kind === 'busy' ? (
          <div className="flex items-center justify-center gap-2 py-10 text-[12px] text-inkMuted">
            <Loader2 size={16} className="anim-spin" />
            {busyIsPlaylist ? t('videoPlaylistAnalysing') : t('videoAnalysing')}
          </div>
        ) : null}

        {analysis.kind === 'error' ? (
          <div className="flex items-start gap-2 p-3 rounded-lg border border-rose-400/30 bg-rose-400/5 text-[13.5px] text-rose-200">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <span className="leading-snug">{analysis.message}</span>
          </div>
        ) : null}

        {analysis.kind === 'ok' ? (
          <>
            {/* Media identity */}
            <div className="flex gap-3 p-2.5 rounded-xl bg-panelHover/50 border border-line">
              {analysis.info.thumbnail ? (
                <img
                  src={analysis.info.thumbnail}
                  alt=""
                  className="w-28 h-16 object-cover rounded-lg shrink-0 bg-input"
                  loading="lazy"
                  referrerPolicy="no-referrer"
                />
              ) : (
                <div className="w-28 h-16 rounded-lg bg-input grid place-items-center shrink-0 text-inkMuted">
                  <Film size={20} />
                </div>
              )}
              <div className="min-w-0 flex-1">
                <p className="text-[12.5px] font-bold text-ink leading-snug line-clamp-2">
                  {analysis.info.title}
                </p>
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 mt-1.5 text-[12.5px] text-inkMuted">
                  {analysis.info.uploader ? <span className="truncate">{analysis.info.uploader}</span> : null}
                  {analysis.info.duration_seconds ? (
                    <span className="mono">{formatMediaLength(analysis.info.duration_seconds)}</span>
                  ) : null}
                  {analysis.info.extractor ? (
                    <span className="px-1.5 py-0.5 rounded bg-panel border border-line mono">
                      {analysis.info.extractor}
                    </span>
                  ) : null}
                </div>
              </div>
            </div>

            {/* Quality list */}
            <div>
              <span className="label block mb-2">{t('videoQuality')}</span>
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5 max-h-52 overflow-y-auto pe-1">
                {analysis.info.formats.map((format) => {
                  const active = selected === format.format_id;
                  return (
                    <button
                      key={format.format_id}
                      type="button"
                      onClick={() => setSelected(format.format_id)}
                      className={`flex items-center gap-2 px-2.5 py-2 rounded-xl border text-start transition-colors ${
                        active
                          ? 'border-accent2/60 bg-accent2/10'
                          : 'border-line bg-panel/60 hover:border-lineStrong'
                      }`}
                    >
                      <span className={active ? 'text-accent2' : 'text-inkMuted'}>
                        {format.is_audio ? <Music size={14} /> : <Video size={14} />}
                      </span>
                      <span className="min-w-0">
                        <span className="block text-[13.5px] font-bold text-ink truncate">
                          {format.is_audio ? t('videoAudioOnly') : format.resolution ?? format.format_id}
                        </span>
                        <span className="block text-[12px] text-inkMuted mono">
                          {format.ext.toUpperCase()}
                          {format.filesize ? ` · ${formatBytes(format.filesize)}` : ''}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
              {analysis.info.formats.length === 0 ? (
                <p className="text-[13.5px] text-inkMuted">{t('videoNoFormats')}</p>
              ) : null}
            </div>

            {/* Filename + overwrite */}
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <div>
                <label htmlFor="zdm-video-name" className="label block mb-1.5">
                  {t('videoSaveAs')}
                </label>
                <input
                  id="zdm-video-name"
                  type="text"
                  value={customName}
                  onChange={(event) => setCustomName(event.target.value)}
                  className="field"
                />
              </div>
              <label className="flex items-end gap-2 pb-2 cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={overwrite}
                  onChange={(event) => setOverwrite(event.target.checked)}
                  className="w-4 h-4 rounded border-line bg-input"
                />
                <span className="text-[13.5px] font-semibold text-inkSoft leading-snug">
                  {t('videoOverwrite')}
                </span>
              </label>
            </div>
          </>
        ) : null}

        {analysis.kind === 'playlist' ? (
          <>
            <div className="flex gap-3 p-2.5 rounded-xl bg-panelHover/50 border border-line">
              {analysis.info.thumbnail ? (
                <img
                  src={analysis.info.thumbnail}
                  alt=""
                  className="w-28 h-16 object-cover rounded-lg shrink-0 bg-input"
                  loading="lazy"
                  referrerPolicy="no-referrer"
                />
              ) : (
                <div className="w-28 h-16 rounded-lg bg-input grid place-items-center shrink-0 text-inkMuted">
                  <Film size={20} />
                </div>
              )}
              <div className="min-w-0 flex-1">
                <p className="text-[12.5px] font-bold text-ink leading-snug line-clamp-2">
                  {analysis.info.title}
                </p>
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 mt-1.5 text-[12.5px] text-inkMuted">
                  {analysis.info.uploader ? (
                    <span className="truncate">{analysis.info.uploader}</span>
                  ) : null}
                  <span className="mono">
                    {analysis.info.entries.length} {t('videoVideos')}
                  </span>
                  {analysis.info.total_duration_seconds ? (
                    <span className="mono">{formatMediaLength(analysis.info.total_duration_seconds)}</span>
                  ) : null}
                  <span className="px-1.5 py-0.5 rounded bg-panel border border-line mono">
                    {t('videoPlaylist')}
                  </span>
                </div>
              </div>
            </div>

            <div>
              <span className="label block mb-2">{t('videoQuality')}</span>
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
                {analysis.info.heights.map((height) => {
                  const active = selectedHeight === height;
                  const size = analysis.info.total_sizes[String(height)] ?? 0;
                  return (
                    <button
                      key={height}
                      type="button"
                      onClick={() => setSelectedHeight(height)}
                      className={`flex items-center gap-2 px-2.5 py-2 rounded-xl border text-start transition-colors ${
                        active
                          ? 'border-accent2/60 bg-accent2/10'
                          : 'border-line bg-panel/60 hover:border-lineStrong'
                      }`}
                    >
                      <span className={active ? 'text-accent2' : 'text-inkMuted'}>
                        <Video size={14} />
                      </span>
                      <span className="min-w-0">
                        <span className="block text-[13.5px] font-bold text-ink truncate">{height}p</span>
                        <span className="block text-[12px] text-inkMuted mono">
                          MP4{size ? ` · ${formatBytes(size)}` : ''}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
              <p className="text-[12.5px] text-inkMuted mt-2">{t('videoPlaylistQualityHint')}</p>
            </div>

            <div>
              <div className="flex items-center justify-between mb-2 gap-2">
                <span className="label">
                  {t('videoVideos')} ({analysis.info.entries.length})
                </span>
                {selectedHeight != null ? (
                  <span className="text-[12.5px] text-inkMuted mono">
                    {t('videoTotalSize')}:{' '}
                    {formatBytes(analysis.info.total_sizes[String(selectedHeight)] ?? 0)}
                  </span>
                ) : null}
              </div>
              <div className="max-h-48 overflow-y-auto rounded-xl border border-line divide-y divide-line/60">
                {analysis.info.entries.map((entry) => {
                  const size =
                    selectedHeight != null ? entry.sizes[String(selectedHeight)] ?? 0 : 0;
                  return (
                    <div key={`${entry.id}-${entry.index}`} className="flex items-center gap-2 px-2.5 py-1.5">
                      <span className="w-6 text-[12px] text-inkMuted mono shrink-0">
                        {entry.index + 1}
                      </span>
                      <span className="min-w-0 flex-1 text-[13px] text-ink truncate">{entry.title}</span>
                      {entry.duration_seconds ? (
                        <span className="text-[12px] text-inkMuted mono shrink-0">
                          {formatMediaLength(entry.duration_seconds)}
                        </span>
                      ) : null}
                      <span className="text-[12px] text-inkMuted mono shrink-0 w-16 text-end">
                        {size ? formatBytes(size) : '—'}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </>
        ) : null}

        {submitError ? (
          <div className="flex items-start gap-2 p-2.5 rounded-lg border border-rose-400/30 bg-rose-400/5 text-[13.5px] text-rose-200">
            <AlertTriangle size={13} className="mt-0.5 shrink-0" />
            <span className="leading-snug">{submitError}</span>
          </div>
        ) : null}
      </div>
    </Modal>
  );
};
