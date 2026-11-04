//! ZDM Add Download Dialog
//! Developed by Ramy Badraan

import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { Link2, Folder, Loader2, CheckCircle2, AlertTriangle, Zap, RotateCcw } from 'lucide-react';
import { Modal } from './ui';
import { useI18n } from '../i18n';
import { NewDownloadRequest, Settings, UrlMetadata } from '../types';
import { formatBytes } from '../utils/format';

interface Props {
  open: boolean;
  onClose: () => void;
  onAdded: (id: string) => void;
  settings: Settings;
  /** Pre-filled URL, e.g. from the browser extension or the clipboard. */
  initialUrl?: string;
}

type ProbeState =
  | { kind: 'idle' }
  | { kind: 'probing' }
  | { kind: 'ok'; meta: UrlMetadata }
  | { kind: 'error'; message: string };

export const AddDownloadModal: React.FC<Props> = ({
  open,
  onClose,
  onAdded,
  settings,
  initialUrl,
}) => {
  const { t } = useI18n();
  const [url, setUrl] = useState(initialUrl ?? '');
  const [filename, setFilename] = useState('');
  const [folder, setFolder] = useState(''); // empty = auto: route by file type
  const [connections, setConnections] = useState(settings.default_connections);
  const [startNow, setStartNow] = useState(true);
  const [probe, setProbe] = useState<ProbeState>({ kind: 'idle' });
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Reset each time the dialog opens so stale values never leak between uses.
  useEffect(() => {
    if (!open) return;
    setUrl(initialUrl ?? '');
    setFilename('');
    setFolder('');
    setConnections(settings.default_connections);
    setStartNow(true);
    setProbe({ kind: 'idle' });
    setSubmitError(null);
    setSubmitting(false);
  }, [open, initialUrl, settings.download_dir, settings.default_connections]);

  // Probe automatically: showing the real size and range support before the
  // user commits prevents "why is this 0 bytes?" confusion.
  useEffect(() => {
    if (!open) return;
    const trimmed = url.trim();
    if (!/^https?:\/\/\S+$/i.test(trimmed)) {
      setProbe({ kind: 'idle' });
      return;
    }

    let cancelled = false;
    setProbe({ kind: 'probing' });

    const timer = window.setTimeout(async () => {
      try {
        const meta = await invoke<UrlMetadata>('probe_url', { url: trimmed });
        if (cancelled) return;
        setProbe({ kind: 'ok', meta });
        setFilename((current) => current || meta.filename);
      } catch (error) {
        if (cancelled) return;
        setProbe({ kind: 'error', message: String(error) });
      }
    }, 420); // debounce so typing does not hammer the server

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [url, open]);

  const pickFolder = async () => {
    try {
      const chosen = await openDialog({ directory: true, multiple: false, defaultPath: folder || undefined });
      if (typeof chosen === 'string') setFolder(chosen);
    } catch {
      /* the dialog plugin may be unavailable: keep the typed path */
    }
  };

  const submit = async () => {
    const trimmed = url.trim();
    if (!/^https?:\/\/\S+$/i.test(trimmed)) {
      setSubmitError(t('invalidUrl'));
      return;
    }

    setSubmitting(true);
    setSubmitError(null);

    const request: NewDownloadRequest = {
      url: trimmed,
      filename: filename.trim() ? filename.trim() : undefined,
      save_path: folder.trim() ? folder.trim() : undefined,
      connections,
      start_now: startNow,
    };

    try {
      const item = await invoke<{ id: string }>('add_download', { req: request });
      onAdded(item.id);
      onClose();
    } catch (error) {
      setSubmitError(String(error));
    } finally {
      setSubmitting(false);
    }
  };

  const canSubmit = probe.kind !== 'probing' && !submitting && url.trim().length > 8;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('newDownloadTitle')}
      subtitle={t('tagline')}
      icon={<Link2 size={17} />}
      locked={submitting}
      footer={
        <>
          <button type="button" onClick={onClose} disabled={submitting} className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold">
            {t('cancel')}
          </button>
          <button
            type="button"
            onClick={submit}
            disabled={!canSubmit}
            className="btn-primary h-9 px-4 text-[12.5px] flex items-center gap-1.5"
          >
            {submitting ? <Loader2 size={14} className="anim-spin" /> : <Zap size={14} />}
            {t('startDownload')}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        {/* URL */}
        <div>
          <label htmlFor="zdm-url" className="label block mb-1.5">
            {t('urlLabel')}
          </label>
          <input
            id="zdm-url"
            type="url"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && canSubmit) submit();
            }}
            placeholder={t('urlPlaceholder')}
            className="field always-ltr"
            autoComplete="off"
            spellCheck={false}
            dir="ltr"
          />

          {/* Probe result */}
          <div className="mt-2 min-h-[22px]">
            {probe.kind === 'probing' ? (
              <span className="flex items-center gap-1.5 text-[13px] text-inkMuted">
                <Loader2 size={12} className="anim-spin" />
                {t('probing')}
              </span>
            ) : probe.kind === 'ok' ? (
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]">
                <span className="flex items-center gap-1 text-emerald-300 font-semibold">
                  <CheckCircle2 size={12} />
                  {probe.meta.total_size > 0 ? formatBytes(probe.meta.total_size) : t('probeFailed')}
                </span>
                <span className="text-inkMuted">
                  {probe.meta.supports_range ? t('rangeSupported') : t('rangeUnsupported')}
                </span>
                <span className="text-inkMuted mono">{probe.meta.category}</span>
              </div>
            ) : probe.kind === 'error' ? (
              <span className="flex items-start gap-1.5 text-[13px] text-rose-300">
                <AlertTriangle size={12} className="mt-0.5 shrink-0" />
                <span className="leading-snug">{probe.message}</span>
              </span>
            ) : null}
          </div>
        </div>

        {/* Filename */}
        <div>
          <label htmlFor="zdm-filename" className="label block mb-1.5">
            {t('filenameLabel')}
          </label>
          <input
            id="zdm-filename"
            type="text"
            value={filename}
            onChange={(event) => setFilename(event.target.value)}
            className="field always-ltr"
            dir="ltr"
            spellCheck={false}
          />
        </div>

        {/* Folder */}
        <div>
          <label htmlFor="zdm-folder" className="label block mb-1.5">
            {t('folderLabel')}
          </label>
          <div className="flex gap-2">
            <input
              id="zdm-folder"
              type="text"
              value={folder}
              onChange={(event) => setFolder(event.target.value)}
              className="field always-ltr flex-1"
              dir="ltr"
              spellCheck={false}
              placeholder={t('folderAuto')}
            />
            {folder && (
              <button
                type="button"
                onClick={() => setFolder('')}
                className="btn-ghost h-10 px-3 flex items-center gap-1.5 text-[12px] font-bold shrink-0"
                title={t('folderAuto')}
              >
                <RotateCcw size={14} />
                {t('folderAuto')}
              </button>
            )}
            <button type="button" onClick={pickFolder} className="btn-ghost h-10 px-3 flex items-center gap-1.5 text-[12px] font-bold shrink-0">
              <Folder size={14} />
              {t('browse')}
            </button>
          </div>
          {!folder && <p className="text-[12.5px] text-inkMuted mt-1">{t('folderAutoHint')}</p>}
        </div>

        {/* Connections */}
        <div>
          <div className="flex items-center justify-between mb-1.5">
            <label htmlFor="zdm-conn" className="label">
              {t('connectionsLabel')}
            </label>
            <span className="mono text-[12px] font-bold text-accent2">{connections}</span>
          </div>
          <input
            id="zdm-conn"
            type="range"
            min={1}
            max={32}
            value={connections}
            onChange={(event) => setConnections(Number(event.target.value))}
            className="w-full accent-cyan-400"
          />
          <p className="text-[12.5px] text-inkMuted mt-1 leading-snug">{t('connectionsHint')}</p>
        </div>

        {/* Start now */}
        <label className="flex items-center gap-2.5 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={startNow}
            onChange={(event) => setStartNow(event.target.checked)}
            className="w-4 h-4 rounded border-line bg-input accent-cyan-400"
          />
          <span className="text-[12px] font-semibold text-inkSoft">{t('startNow')}</span>
        </label>

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
