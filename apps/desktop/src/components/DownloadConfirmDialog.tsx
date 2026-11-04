//! ZDM Download Confirmation
//! Developed by Ramy Badraan
//!
//! Clicking a link must not start a transfer immediately. This dialog is the
//! "commit" step: it shows what the file is, how big it is, whether it can resume,
//! and lets the user change the folder or the name before anything touches the
//! disk. Pressing start opens a dedicated transfer window.

import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  ArrowDownToLine,
  Folder,
  Loader2,
  AlertTriangle,
  FileType2,
  Ruler,
  Repeat,
  Layers,
} from 'lucide-react';
import { Modal } from './ui';
import { useI18n } from '../i18n';
import { Settings, UrlMetadata } from '../types';
import { formatBytes } from '../utils/format';

interface Props {
  open: boolean;
  url: string;
  onCancel: () => void;
  onStarted: (id: string) => void;
  settings: Settings;
}

type Probe =
  | { kind: 'busy' }
  | { kind: 'ok'; meta: UrlMetadata }
  | { kind: 'error'; message: string };

export const DownloadConfirmDialog: React.FC<Props> = ({
  open,
  url,
  onCancel,
  onStarted,
  settings,
}) => {
  const { t } = useI18n();

  const [probe, setProbe] = useState<Probe>({ kind: 'busy' });
  const [filename, setFilename] = useState('');
  const [folder, setFolder] = useState(settings.download_dir);
  const [connections, setConnections] = useState(settings.default_connections);
  const [startOpenWindow, setStartOpenWindow] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Probe every time the URL changes: the dialog must show real facts.
  useEffect(() => {
    if (!open || !url) return;

    let cancelled = false;
    setProbe({ kind: 'busy' });
    setSubmitError(null);
    setSubmitting(false);
    setFolder(settings.download_dir);
    setConnections(settings.default_connections);
    setStartOpenWindow(true);

    (async () => {
      try {
        const meta = await invoke<UrlMetadata>('probe_url', { url });
        if (cancelled) return;
        setProbe({ kind: 'ok', meta });
        setFilename(meta.filename);
      } catch (error) {
        if (!cancelled) setProbe({ kind: 'error', message: String(error) });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [open, url, settings.download_dir, settings.default_connections]);

  const pickFolder = async () => {
    try {
      const chosen = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: folder,
      });
      if (typeof chosen === 'string') setFolder(chosen);
    } catch {
      /* keep the typed path */
    }
  };

  const start = async () => {
    setSubmitting(true);
    setSubmitError(null);

    try {
      // A torrent is not an HTTP transfer: `add_download` refuses it outright
      // ("This is a torrent — use Add torrent"), so a pasted `…/x.torrent` link
      // appeared to fail at the last step. The BitTorrent engine has its own
      // command, and it owns the row from then on.
      const isTorrent =
        /^magnet:/i.test(url.trim()) || /\.torrent$/i.test(url.trim().split(/[?#]/)[0]);

      if (isTorrent) {
        await invoke<{ id: string }>('add_torrent', {
          url: url.trim(),
          savePath: folder.trim() || null,
          paused: false,
        });
        onStarted('');
        return;
      }

      const item = await invoke<{ id: string }>('add_download', {
        req: {
          url,
          filename: filename.trim() || undefined,
          save_path: folder.trim() || undefined,
          connections,
          start_now: true,
        },
      });

      // Open the dedicated transfer window: this is the moment the user asked for
      // "a beautiful separate window with the segment ribbon".
      if (startOpenWindow) {
        await invoke('open_transfer_window', { id: item.id, alwaysOnTop: false }).catch(
          () => undefined
        );
      }

      onStarted(item.id);
    } catch (error) {
      setSubmitError(String(error));
      setSubmitting(false);
    }
  };

  const meta = probe.kind === 'ok' ? probe.meta : null;

  return (
    <Modal
      open={open}
      onClose={onCancel}
      title={t('confirmTitle')}
      subtitle={t('confirmSubtitle')}
      icon={<ArrowDownToLine size={17} />}
      locked={submitting}
      footer={
        <>
          <button
            type="button"
            onClick={onCancel}
            disabled={submitting}
            className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold"
          >
            {t('confirmCancel')}
          </button>
          <button
            type="button"
            onClick={() => void start()}
            disabled={probe.kind !== 'ok' || submitting}
            className="btn-primary h-9 px-4 text-[12.5px] flex items-center gap-1.5"
          >
            {submitting ? (
              <Loader2 size={14} className="anim-spin" />
            ) : (
              <ArrowDownToLine size={14} />
            )}
            {t('confirmStart')}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        {probe.kind === 'busy' ? (
          <div className="flex items-center justify-center gap-2 py-8 text-[12px] text-inkMuted">
            <Loader2 size={16} className="anim-spin" />
            {t('probing')}
          </div>
        ) : null}

        {probe.kind === 'error' ? (
          <div className="flex items-start gap-2 p-3 rounded-lg border border-danger/30 bg-danger/10 text-[13.5px] text-danger">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <span className="leading-snug">{probe.message}</span>
          </div>
        ) : null}

        {meta ? (
          <>
            {/* Fact strip: four honest values, no decoration. */}
            <div className="grid grid-cols-2 gap-2">
              <Fact
                icon={<Ruler size={13} />}
                label={t('confirmSize')}
                value={
                  meta.total_size > 0
                    ? formatBytes(meta.total_size)
                    : t('confirmUnknownSize')
                }
              />
              <Fact icon={<FileType2 size={13} />} label={t('confirmType')} value={meta.category} />
              <Fact
                icon={<Repeat size={13} />}
                label={t('confirmResumable')}
                value={meta.supports_range ? t('confirmYes') : t('confirmNo')}
                tone={meta.supports_range ? 'good' : 'warn'}
              />
              <Fact
                icon={<Layers size={13} />}
                label={t('confirmConnections')}
                value={String(connections)}
              />
            </div>

            {/* File name */}
            <div>
              <label htmlFor="zdm-confirm-name" className="label block mb-1.5">
                {t('confirmFileName')}
              </label>
              <input
                id="zdm-confirm-name"
                type="text"
                value={filename}
                onChange={(event) => setFilename(event.target.value)}
                className="field always-ltr"
                dir="ltr"
                spellCheck={false}
              />
            </div>

            {/* Destination */}
            <div>
              <label htmlFor="zdm-confirm-folder" className="label block mb-1.5">
                {t('confirmSaveTo')}
              </label>
              <div className="flex gap-2">
                <input
                  id="zdm-confirm-folder"
                  type="text"
                  value={folder}
                  onChange={(event) => setFolder(event.target.value)}
                  className="field always-ltr flex-1"
                  dir="ltr"
                  spellCheck={false}
                />
                <button
                  type="button"
                  onClick={() => void pickFolder()}
                  className="btn-ghost h-[38px] px-3 flex items-center gap-1.5 text-[12px] font-bold shrink-0"
                >
                  <Folder size={14} />
                  {t('confirmChange')}
                </button>
              </div>
            </div>

            {/* Connections */}
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <span className="label">{t('confirmConnections')}</span>
                <span className="mono text-[12px] font-bold text-accent2">{connections}</span>
              </div>
              <input
                type="range"
                min={1}
                max={32}
                value={connections}
                onChange={(event) => setConnections(Number(event.target.value))}
                className="w-full accent-cyan-500"
                aria-label={t('confirmConnections')}
              />
              <p className="text-[12.5px] text-inkMuted mt-1 leading-snug">
                {t('connectionsHint')}
              </p>
            </div>

            {/* Open the transfer window */}
            <label className="flex items-center gap-2.5 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={startOpenWindow}
                onChange={(event) => setStartOpenWindow(event.target.checked)}
                className="w-4 h-4 rounded border-line accent-cyan-500"
              />
              <span className="text-[13.5px] font-semibold text-inkSoft">
                {t('transferTitle')}
              </span>
            </label>
          </>
        ) : null}

        {submitError ? (
          <div className="flex items-start gap-2 p-2.5 rounded-lg border border-danger/30 bg-danger/10 text-[13.5px] text-danger">
            <AlertTriangle size={13} className="mt-0.5 shrink-0" />
            <span className="leading-snug">{submitError}</span>
          </div>
        ) : null}
      </div>
    </Modal>
  );
};

const Fact: React.FC<{
  icon: React.ReactNode;
  label: string;
  value: string;
  tone?: 'good' | 'warn';
}> = ({ icon, label, value, tone }) => (
  <div className="panel-inset px-2.5 py-2">
    <div className="flex items-center gap-1.5 text-inkMuted mb-0.5">
      {icon}
      <span className="text-[11px] font-bold uppercase tracking-[0.08em]">{label}</span>
    </div>
    <div
      className={`text-[12px] font-bold truncate ${
        tone === 'good' ? 'text-emerald-400' : tone === 'warn' ? 'text-amber-400' : 'text-ink'
      }`}
    >
      {value}
    </div>
  </div>
);
