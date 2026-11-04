//! ZDM Settings Screen
//! Developed by Ramy Badraan
//!
//! Every control here persists to the database and takes effect immediately.
//! The diagnostics card reports what the app actually detected, including the
//! honest "tray unavailable" case on minimal Linux sessions.

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  Cpu,
  Folder,
  HardDrive,
  RefreshCw as UpdateIcon,
  Globe,
  Film,
  Loader2,
  CheckCircle2,
  RefreshCw,
  ExternalLink,
  ShieldCheck,
  Activity,
  Bell,
  Gauge,
  Languages,
  Wrench,
} from 'lucide-react';
import { SectionCard, Switch } from './ui';
import { useI18n } from '../i18n';
import { BrowserStatus, CaptureStatus, DEFAULT_INTERCEPT_EXTENSIONS, Diagnostics, IntegrationReport, Settings, CATEGORY_KEYS, AppInfo } from '../types';
import type { Update } from '@tauri-apps/plugin-updater';
import { LANGS, Lang } from '../i18n/dict';

interface Props {
  settings: Settings;
  onChange: (next: Settings) => void;
}

export const SettingsView: React.FC<Props> = ({ settings, onChange }) => {
  const { t, lang, setLang } = useI18n();
  const [browsers, setBrowsers] = useState<BrowserStatus[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [interceptText, setInterceptText] = useState(() => settings.intercept_extensions.join(', '));
  const [capture, setCapture] = useState<CaptureStatus | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [updateBusy, setUpdateBusy] = useState<'idle' | 'checking' | 'installing'>('idle');
  const [updateAvailable, setUpdateAvailable] = useState<null | { version: string; update: Update }>(null);
  const [updateMessage, setUpdateMessage] = useState('');
  const [ytDlpPath, setYtDlpPath] = useState<string>('');
  const [ytDlpBusy, setYtDlpBusy] = useState(false);
  const [ffmpegBusy, setFfmpegBusy] = useState(false);
  const [report, setReport] = useState<IntegrationReport | null>(null);
  const [savedTick, setSavedTick] = useState(false);
  const [copiedPath, setCopiedPath] = useState(false);

  // The folder the user must hand to "Load unpacked": the one containing
  // manifest.json, which is <install dir>\extension — not the install dir itself.
  const extensionDir = diagnostics?.browser_report?.extension_dir || report?.extension_dir || '';

  // Reset the "Copied" label after a moment so the button stays reusable.
  useEffect(() => {
    if (!copiedPath) return;
    const timer = window.setTimeout(() => setCopiedPath(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copiedPath]);

  const copyExtensionPath = async () => {
    try {
      await navigator.clipboard.writeText(extensionDir);
      setCopiedPath(true);
    } catch {
      /* clipboard blocked: the path is still visible and selectable */
    }
  };

  // The list is edited as one text field; keep it in sync when the backend
  // returns a sanitized array after a save.
  useEffect(() => {
    setInterceptText(settings.intercept_extensions.join(', '));
  }, [settings.intercept_extensions]);

  // `get_diagnostics` is cached in Rust, so this is cheap — but it is still only
  // fetched once per mount, and never re-entered while a request is in flight.
  const loadingRef = useRef(false);

  const loadAll = useCallback(async () => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    try {
      const [list, diag, cap] = await Promise.all([
        invoke<BrowserStatus[]>('get_browsers'),
        invoke<Diagnostics>('get_diagnostics'),
        invoke<CaptureStatus>('capture_status'),
      ]);
      setBrowsers(list);
      setDiagnostics(diag);
      setCapture(cap);
    } catch (error) {
      console.error('diagnostics failed', error);
    } finally {
      loadingRef.current = false;
    }
  }, []);

  useEffect(() => {
    // Defer past the first paint: the panel appears immediately, the probes fill
    // in a frame later. This is what stopped the window looking frozen.
    const timer = window.setTimeout(() => void loadAll(), 60);
    return () => window.clearTimeout(timer);
  }, [loadAll]);

  const patch = (partial: Partial<Settings>) => {
    onChange({ ...settings, ...partial });
    setSavedTick(true);
    window.setTimeout(() => setSavedTick(false), 1600);
  };

  const pickFolder = async () => {
    try {
      const chosen = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: settings.download_dir,
      });
      if (typeof chosen === 'string') patch({ download_dir: chosen });
    } catch {
      /* keep the current path */
    }
  };

  const commitIntercept = () => {
    const list = interceptText
      .split(',')
      .map((entry) => entry.trim().replace(/^\./, '').toLowerCase())
      .filter(Boolean);
    patch({ intercept_extensions: list });
  };

  const resetIntercept = () => {
    setInterceptText(DEFAULT_INTERCEPT_EXTENSIONS.join(', '));
    patch({ intercept_extensions: DEFAULT_INTERCEPT_EXTENSIONS });
  };

  useEffect(() => {
    invoke<AppInfo>('get_app_info')
      .then(setAppInfo)
      .catch(() => undefined);
  }, []);

  const checkForUpdates = useCallback(async () => {
    if (updateBusy !== 'idle') return;
    setUpdateBusy('checking');
    setUpdateMessage('');
    setUpdateAvailable(null);
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (!update) {
        setUpdateMessage(t('updateUpToDate'));
        return;
      }
      setUpdateAvailable({ version: update.version, update });
    } catch (error) {
      setUpdateMessage(`${t('updateFailed')}: ${String(error)}`);
    } finally {
      setUpdateBusy('idle');
    }
  }, [updateBusy, t]);

  const installUpdate = useCallback(async () => {
    if (!updateAvailable || updateBusy !== 'idle') return;
    setUpdateBusy('installing');
    try {
      await updateAvailable.update.downloadAndInstall();
      setUpdateAvailable(null);
      setUpdateMessage(t('updateInstalledRestart'));
    } catch (error) {
      setUpdateMessage(`${t('updateFailed')}: ${String(error)}`);
    } finally {
      setUpdateBusy('idle');
    }
  }, [updateAvailable, updateBusy, t]);

  const pickCategoryFolder = async (category: string) => {
    try {
      const chosen = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: settings.category_dirs[category] || settings.download_dir || undefined,
      });
      if (typeof chosen === 'string') {
        patch({ category_dirs: { ...settings.category_dirs, [category]: chosen } });
      }
    } catch {
      /* keep the current path */
    }
  };

  const resetCategoryFolder = (category: string) => {
    const next = { ...settings.category_dirs };
    delete next[category];
    patch({ category_dirs: next });
  };

  const categoryLabel = (key: string) =>
    ({ Video: t('catVideo'), Audio: t('catAudio'), Compressed: t('catCompressed'), Documents: t('catDocuments'), Images: t('catImages'), Programs: t('catPrograms'), Others: t('catOthers') })[key] ?? key;

  const repairBrowsers = async () => {
    try {
      const result = await invoke<IntegrationReport>('register_all_browsers');
      setReport(result);
      await loadAll();
    } catch (error) {
      console.error(error);
    }
  };

  const updateYtDlp = async () => {
    setYtDlpBusy(true);
    try {
      const path = await invoke<string>('update_yt_dlp_engine');
      setYtDlpPath(path);
    } catch (error) {
      setYtDlpPath(String(error));
    } finally {
      setYtDlpBusy(false);
    }
  };

  const installFfmpeg = async () => {
    setFfmpegBusy(true);
    try {
      await invoke<string>('install_ffmpeg');
      const diag = await invoke<Diagnostics>('get_diagnostics');
      setDiagnostics(diag);
      setSavedTick(true);
      window.setTimeout(() => setSavedTick(false), 1600);
    } catch (error) {
      console.error(error);
    } finally {
      setFfmpegBusy(false);
    }
  };

  const changeLanguage = async (code: Lang) => {
    setLang(code);
    patch({ language: code });
  };

  const changeTheme = async (theme: string) => {
    patch({ theme });
  };

  const toggleAutostart = async (enabled: boolean) => {
    try {
      await invoke('set_autostart', { enabled });
      patch({ auto_start: enabled });
      const diag = await invoke<Diagnostics>('get_diagnostics');
      setDiagnostics(diag);
    } catch (error) {
      console.error(error);
    }
  };

  return (
    <div className="space-y-3 pb-6 max-w-3xl">
      {/* Saved tick */}
      {savedTick ? (
        <div className="fixed top-[70px] end-5 z-40 flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-emerald-400/15 border border-emerald-400/30 text-emerald-200 text-[13.5px] font-bold anim-rise">
          <CheckCircle2 size={13} />
          {t('saved')}
        </div>
      ) : null}

      {/* ---------------------------------------------------------- engine */}
      <SectionCard title={t('sectionEngine')} icon={<Cpu size={14} />}>
        <div className="space-y-4 min-h-[120px]">
          <div>
            <label className="label block mb-1.5">{t('downloadDirLabel')}</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={settings.download_dir}
                onChange={(event) => patch({ download_dir: event.target.value })}
                className="field always-ltr flex-1"
                dir="ltr"
                spellCheck={false}
              />
              <button
                type="button"
                onClick={pickFolder}
                className="btn-ghost h-10 px-3 flex items-center gap-1.5 text-[12px] font-bold shrink-0"
              >
                <Folder size={14} />
                {t('browse')}
              </button>
            </div>
            <p className="text-[12.5px] text-inkMuted mt-1">{t('downloadDirHint')}</p>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <label className="label">{t('maxConcurrentLabel')}</label>
                <span className="mono text-[12px] font-bold text-accent2">
                  {settings.max_concurrent_downloads}
                </span>
              </div>
              <input
                type="range"
                min={1}
                max={10}
                value={settings.max_concurrent_downloads}
                onChange={(event) =>
                  patch({ max_concurrent_downloads: Number(event.target.value) })
                }
                className="w-full accent-cyan-400"
              />
              <p className="text-[12.5px] text-inkMuted mt-1">{t('maxConcurrentHint')}</p>
            </div>

            <div>
              <div className="flex items-center justify-between mb-1.5">
                <label className="label">{t('defaultConnectionsLabel')}</label>
                <span className="mono text-[12px] font-bold text-accent2">
                  {settings.default_connections}
                </span>
              </div>
              <input
                type="range"
                min={1}
                max={32}
                value={settings.default_connections}
                onChange={(event) => patch({ default_connections: Number(event.target.value) })}
                className="w-full accent-cyan-400"
              />
              <p className="text-[12.5px] text-inkMuted mt-1">{t('connectionsHint')}</p>
            </div>
          </div>

          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label className="label flex items-center gap-1.5">
                <Gauge size={12} />
                {t('speedLimitLabel')}
              </label>
              <span className="mono text-[12px] font-bold text-accent2">
                {settings.speed_limit_kbps === 0 ? '∞' : settings.speed_limit_kbps}
              </span>
            </div>
            <input
              type="range"
              min={0}
              max={20000}
              step={250}
              value={settings.speed_limit_kbps}
              onChange={(event) => patch({ speed_limit_kbps: Number(event.target.value) })}
              className="w-full accent-cyan-400"
            />
            <p className="text-[12.5px] text-inkMuted mt-1">{t('speedLimitHint')}</p>
          </div>
        </div>
      </SectionCard>

      {/* --------------------------------------------- download locations */}
      <SectionCard title={t('sectionLocations')} icon={<HardDrive size={14} />}>
        <div className="space-y-1">
          <p className="text-[12px] text-inkMuted mb-2">{t('downloadLocationsHint')}</p>
          {CATEGORY_KEYS.map((category) => {
            const override = settings.category_dirs[category];
            return (
              <div
                key={category}
                className="flex items-center gap-2 py-1.5 border-b border-line last:border-0"
              >
                <span className="w-[92px] shrink-0 text-[12.5px] font-semibold text-inkSoft">
                  {categoryLabel(category)}
                </span>
                <span
                  className="flex-1 min-w-0 mono text-[11px] text-inkMuted truncate"
                  dir="ltr"
                  title={override ?? t('locationDefault')}
                >
                  {override ?? t('locationDefault')}
                </span>
                {override && (
                  <button
                    type="button"
                    onClick={() => resetCategoryFolder(category)}
                    className="h-7 px-2.5 rounded-md text-[11.5px] font-bold text-inkMuted hover:text-ink hover:bg-panel border border-line transition-colors shrink-0"
                  >
                    {t('resetLocation')}
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => void pickCategoryFolder(category)}
                  className="h-7 w-7 rounded-md flex items-center justify-center text-inkMuted hover:text-ink hover:bg-panel border border-line transition-colors shrink-0"
                  title={t('browse')}
                  aria-label={`${t('browse')} — ${categoryLabel(category)}`}
                >
                  <Folder size={13} />
                </button>
              </div>
            );
          })}
        </div>
      </SectionCard>

      {/* -------------------------------------------------------- general */}
      <SectionCard title={t('sectionGeneral')} icon={<Globe size={14} />}>
        <div className="space-y-3">
          {/* Language */}
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 text-[12.5px] font-semibold text-inkSoft">
              <Languages size={14} className="text-inkMuted" />
              {t('languageLabel')}
            </div>
            <div className="flex gap-1 p-0.5 rounded-xl bg-input border border-line">
              {LANGS.map((option) => (
                <button
                  key={option.code}
                  type="button"
                  onClick={() => void changeLanguage(option.code)}
                  className={`px-2.5 h-7 rounded-lg text-[13.5px] font-bold transition-colors ${
                    lang === option.code
                      ? 'bg-panelHover text-ink'
                      : 'text-inkMuted hover:text-inkSoft'
                  }`}
                >
                  {option.native}
                </button>
              ))}
            </div>
          </div>

          {/* Theme */}
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 text-[12.5px] font-semibold text-inkSoft">
              <ShieldCheck size={14} className="text-inkMuted" />
              {t('themeLabel')}
            </div>
            <div className="flex gap-1 p-0.5 rounded-xl bg-input border border-line">
              {[
                { id: 'moon', key: 'themeMoon' as const },
                { id: 'dawn', key: 'themeDawn' as const },
                { id: 'system', key: 'themeSystem' as const },
              ].map((option) => (
                <button
                  key={option.id}
                  type="button"
                  onClick={() => void changeTheme(option.id)}
                  className={`px-2.5 h-7 rounded-lg text-[13.5px] font-bold transition-colors ${
                    settings.theme === option.id
                      ? 'bg-panelHover text-ink'
                      : 'text-inkMuted hover:text-inkSoft'
                  }`}
                >
                  {t(option.key)}
                </button>
              ))}
            </div>
          </div>

          <Row
            icon={<RefreshCw size={14} />}
            title={t('autoStartLabel')}
            hint={t('autoStartHint')}
            control={
              <Switch
                checked={settings.auto_start}
                onChange={(next) => void toggleAutostart(next)}
                label={t('autoStartLabel')}
              />
            }
          />

          <Row
            icon={<Bell size={14} />}
            title={t('notificationsLabel')}
            control={
              <Switch
                checked={settings.notifications}
                onChange={(next) => patch({ notifications: next })}
                label={t('notificationsLabel')}
              />
            }
          />

          <Row
            icon={<Globe size={14} />}
            title={t('captureLabel')}
            control={
              <Switch
                checked={settings.capture_downloads}
                onChange={(next) => patch({ capture_downloads: next })}
                label={t('captureLabel')}
              />
            }
          />

          <Row
            icon={<Film size={14} />}
            title={t('snifferLabel')}
            control={
              <Switch
                checked={settings.sniffer_enabled}
                onChange={(next) => patch({ sniffer_enabled: next })}
                label={t('snifferLabel')}
              />
            }
          />

          {/* Intercept list: the browser keeps everything outside these types. */}
          <div>
            <div className="flex items-center justify-between gap-3 mb-1.5">
              <div className="flex items-center gap-2 text-[12.5px] font-semibold text-inkSoft">
                <ShieldCheck size={14} className="text-inkMuted" />
                {t('interceptExtensionsLabel')}
              </div>
              <button
                type="button"
                onClick={resetIntercept}
                className="text-[12.5px] font-bold text-accent2 hover:text-ink transition-colors"
              >
                {t('resetIntercept')}
              </button>
            </div>
            <input
              type="text"
              dir="ltr"
              spellCheck={false}
              value={interceptText}
              onChange={(event) => setInterceptText(event.target.value)}
              onBlur={commitIntercept}
              onKeyDown={(event) => {
                if (event.key === 'Enter') (event.target as HTMLInputElement).blur();
              }}
              className="field always-ltr w-full"
              placeholder="mp4, mkv, zip, exe, …"
            />
            <p className="text-[12.5px] text-inkMuted mt-1">{t('interceptExtensionsHint')}</p>
          </div>
        </div>
      </SectionCard>

      {/* --------------------------------------------------------- browsers */}
      <SectionCard
        title={t('sectionBrowsers')}
        icon={<Globe size={14} />}
        action={
          <button
            type="button"
            onClick={() => void repairBrowsers()}
            className="btn-ghost h-7 px-2.5 flex items-center gap-1.5 text-[13px] font-bold"
          >
            <Wrench size={12} />
            {t('repairBrowsers')}
          </button>
        }
      >
        <p className="text-[12.5px] text-inkMuted mb-2.5 leading-snug">{t('browsersHint')}</p>

        {/* Without this the user has to guess which folder to hand the browser,
            and the obvious choice — the install directory — is the wrong one:
            manifest.json sits one level down in extension\. */}
        {extensionDir ? (
          <div className="mb-3 rounded-lg bg-panelHover/40 border border-line p-2.5">
            <div className="flex items-center justify-between gap-2 mb-1.5">
              <span className="text-[12.5px] font-extrabold text-inkSoft">
                {t('extensionFolderLabel')}
              </span>
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => void copyExtensionPath()}
                  className="btn-ghost h-6 px-2 text-[12px] font-bold"
                >
                  {copiedPath ? t('pathCopied') : t('copyPath')}
                </button>
                <button
                  type="button"
                  onClick={() => void invoke('open_file_folder', { path: extensionDir })}
                  className="btn-ghost h-6 px-2 flex items-center gap-1 text-[12px] font-bold"
                >
                  <Folder size={11} />
                  {t('openExtensionFolder')}
                </button>
              </div>
            </div>
            {/* LTR + mono: this is a Windows path, and in an RTL layout it would
                otherwise render backslashes and the drive letter in the wrong
                visual order. */}
            <code
              dir="ltr"
              className="block text-[12.5px] font-mono text-accent2 break-all leading-snug text-left"
            >
              {extensionDir}
            </code>
            <p className="text-[12px] text-inkMuted mt-1.5 leading-snug">
              {t('extensionFolderHint')}
            </p>
          </div>
        ) : null}

        <div className="space-y-1.5">
          {browsers
            .filter((browser) => browser.is_installed)
            .map((browser) => (
              <div
                key={browser.id}
                className="flex items-center gap-2 px-2.5 py-2 rounded-lg bg-panelHover/40 border border-line"
              >
                <span className="text-[13.5px] font-bold text-ink flex-1 truncate">
                  {browser.name}
                </span>
                <span className="text-[11px] font-bold uppercase tracking-wide px-1.5 py-0.5 rounded bg-panel border border-line text-inkMuted">
                  {browser.family}
                </span>
                <span
                  className={`text-[12px] font-bold px-1.5 py-0.5 rounded ${
                    browser.native_host_ready
                      ? 'text-emerald-300 bg-emerald-400/10'
                      : 'text-amber-300 bg-amber-400/10'
                  }`}
                >
                  {browser.native_host_ready ? t('browserReady') : t('browserPending')}
                </span>
                <button
                  type="button"
                  onClick={() =>
                    void invoke('launch_browser_with_ext', { browserId: browser.id })
                  }
                  title={t('launchBrowser')}
                  className="btn-icon !w-7 !h-7"
                >
                  <ExternalLink size={12} />
                </button>
              </div>
            ))}

          {browsers.filter((browser) => browser.is_installed).length === 0 ? (
            <p className="text-[13px] text-inkMuted">{t('browserMissing')}</p>
          ) : null}
        </div>

        {report && report.warnings.length > 0 ? (
          <ul className="mt-2.5 space-y-1">
            {report.warnings.map((warning, index) => (
              <li key={index} className="text-[12.5px] text-amber-200/80 leading-snug">
                • {warning}
              </li>
            ))}
          </ul>
        ) : null}
      </SectionCard>

      {/* ---------------------------------------------------------- updates */}
      <SectionCard title={t('sectionUpdates')} icon={<UpdateIcon size={14} />}>
        <div className="space-y-2.5">
          <div className="flex items-center justify-between gap-3 flex-wrap">
            <div className="min-w-0">
              <div className="text-[14px] font-extrabold text-ink" dir="ltr">
                ZDM {appInfo?.version ?? '—'}
              </div>
              <p className="text-[12px] text-inkMuted mt-0.5 max-w-md">{t('updatesHint')}</p>
            </div>
            {updateAvailable ? (
              <button
                type="button"
                onClick={() => void installUpdate()}
                disabled={updateBusy === 'installing'}
                className="btn-primary h-9 px-3.5 text-[12.5px] font-bold flex items-center gap-1.5"
              >
                {updateBusy === 'installing' ? <Loader2 size={13} className="anim-spin" /> : null}
                {updateBusy === 'installing' ? t('installingUpdate') : t('installUpdate')}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => void checkForUpdates()}
                disabled={updateBusy === 'checking'}
                className="btn-ghost h-9 px-3.5 text-[12.5px] font-bold flex items-center gap-1.5"
              >
                {updateBusy === 'checking' ? <Loader2 size={13} className="anim-spin" /> : <UpdateIcon size={13} />}
                {updateBusy === 'checking' ? t('checkingUpdates') : t('checkForUpdates')}
              </button>
            )}
          </div>
          {updateAvailable && (
            <p className="text-[12.5px] font-semibold text-emerald-400">
              {t('updateAvailable').replace('{version}', updateAvailable.version)}
            </p>
          )}
          {!updateAvailable && updateMessage && (
            <p className="text-[12px] text-inkMuted">{updateMessage}</p>
          )}
          {updateAvailable && updateMessage && (
            <p className="text-[12px] text-amber-300">{updateMessage}</p>
          )}
          {appInfo?.platform === 'linux' && (
            <p className="text-[11px] text-inkMuted">{t('linuxUpdateHint')}</p>
          )}
        </div>
      </SectionCard>

      {/* ------------------------------------------------------------ video */}
      <SectionCard title={t('sectionVideo')} icon={<Film size={14} />}>
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[12.5px] font-semibold text-inkSoft">{t('ytDlpLabel')}</div>
            <div className="text-[12.5px] text-inkMuted truncate always-ltr mt-0.5" dir="ltr">
              {ytDlpPath || t('ytDlpReady')}
            </div>
          </div>
          <button
            type="button"
            onClick={() => void updateYtDlp()}
            disabled={ytDlpBusy}
            className="btn-ghost h-8 px-3 flex items-center gap-1.5 text-[13.5px] font-bold shrink-0"
          >
            {ytDlpBusy ? <Loader2 size={13} className="anim-spin" /> : <RefreshCw size={13} />}
            {ytDlpBusy ? t('ytDlpUpdating') : t('ytDlpUpdate')}
          </button>
        </div>

      </SectionCard>

      {/* ------------------------------------------------------------ ffmpeg */}
      <SectionCard title={t('sectionVideo')} icon={<Film size={14} />}>
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[12.5px] font-semibold text-inkSoft">{t('ffmpegLabel')}</div>
            <div
              className={`text-[12.5px] mt-0.5 leading-snug ${
                diagnostics?.ffmpeg_available ? 'text-inkMuted' : 'text-amber-300'
              }`}
            >
              {diagnostics?.ffmpeg_available ? t('ffmpegReady') : t('ffmpegMissing')}
            </div>
          </div>
          {!diagnostics?.ffmpeg_available ? (
            <button
              type="button"
              onClick={() => void installFfmpeg()}
              disabled={ffmpegBusy}
              className="btn-ghost h-8 px-3 flex items-center gap-1.5 text-[13.5px] font-bold shrink-0"
            >
              {ffmpegBusy ? <Loader2 size={13} className="anim-spin" /> : <RefreshCw size={13} />}
              {t('ffmpegInstall')}
            </button>
          ) : null}
        </div>
      </SectionCard>

      {/* ------------------------------------------------ browser capture */}
      <SectionCard title={t('captureHealth')} icon={<Globe size={14} />}>
        <p className="text-[12px] text-inkMuted mb-2">{t('captureHealthHint')}</p>
        <dl className="space-y-1.5">
          <DiagRow
            label={t('captureBridge')}
            value={capture?.bridge_running ? `${t('available')} (:${capture.bridge_port})` : t('unavailable')}
            ok={!!capture?.bridge_running}
            hint={capture && !capture.bridge_running ? t('captureBridgeDown') : undefined}
          />
          <DiagRow
            label={t('captureEnabledLabel')}
            value={capture?.capture_downloads ? t('enabled') : t('disabled')}
            ok={!!capture?.capture_downloads}
          />
          <DiagRow
            label={t('captureInterceptLabel')}
            value={
              capture && capture.intercept_extensions.length > 0
                ? capture.intercept_extensions.slice(0, 8).join(', ') + (capture.intercept_extensions.length > 8 ? ' …' : '')
                : t('captureNone')
            }
            ok={!!capture && capture.intercept_extensions.length > 0}
            mono
          />
        </dl>
        <p className="text-[11px] text-inkMuted mt-2">{t('captureImagesHint')}</p>
      </SectionCard>

      {/* ------------------------------------------------------ diagnostics */}
      <SectionCard title={t('sectionDiagnostics')} icon={<Activity size={14} />}>
        <p className="text-[12.5px] text-inkMuted mb-2.5">{t('diagnosticsHint')}</p>

        <dl className="space-y-1.5">
          <DiagRow
            label={t('trayStatus')}
            value={diagnostics?.tray_available ? t('available') : t('unavailable')}
            ok={!!diagnostics?.tray_available}
            hint={!diagnostics?.tray_available ? t('trayMissingHint') : undefined}
          />
          <DiagRow
            label={t('autostartStatus')}
            value={diagnostics?.autostart_enabled ? `${t('enabled')} · ${diagnostics.autostart_kind}` : t('disabled')}
            ok={!!diagnostics?.autostart_enabled}
            hint={!diagnostics?.autostart_enabled ? t('autostartMissingHint') : undefined}
          />
          <DiagRow label={t('desktopStatus')} value={diagnostics?.desktop || '—'} ok />
          <DiagRow label={t('dataDir')} value={diagnostics?.data_dir || '—'} ok mono />
          <DiagRow label={t('logFile')} value={diagnostics?.log_file || '—'} ok mono />
        </dl>
      </SectionCard>
    </div>
  );
};

const Row: React.FC<{
  icon: React.ReactNode;
  title: string;
  hint?: string;
  control: React.ReactNode;
}> = ({ icon, title, hint, control }) => (
  <div className="flex items-start justify-between gap-3">
    <div className="flex items-start gap-2 min-w-0">
      <span className="text-inkMuted mt-0.5">{icon}</span>
      <div className="min-w-0">
        <div className="text-[12.5px] font-semibold text-inkSoft">{title}</div>
        {hint ? <div className="text-[12.5px] text-inkMuted mt-0.5">{hint}</div> : null}
      </div>
    </div>
    {control}
  </div>
);

const DiagRow: React.FC<{
  label: string;
  value: string;
  ok?: boolean;
  mono?: boolean;
  hint?: string;
}> = ({ label, value, ok, mono, hint }) => (
  <div className="flex flex-col gap-0.5">
    <div className="flex items-center justify-between gap-3">
      <dt className="text-[13px] text-inkMuted shrink-0">{label}</dt>
      <dd
        className={`text-[13px] font-semibold truncate ${
          ok ? 'text-inkSoft' : 'text-amber-300'
        } ${mono ? 'mono always-ltr' : ''}`}
        dir={mono ? 'ltr' : undefined}
      >
        {value}
      </dd>
    </div>
    {hint ? <p className="text-[12px] text-amber-200/70 leading-snug">{hint}</p> : null}
  </div>
);
