//! ZDM Command Bar
//! Developed by Ramy Badraan
//!
//! The title bar *is* the command bar (design guide §03): brand on the left, one
//! big URL field in the middle, speed mode + window controls on the right. There
//! is no second toolbar and no menu strip.
//!
//! The field is the whole add-flow. Pasting a link shows a "Link detected" chip
//! with an Add button; Enter commits immediately. The full dialog only opens for
//! options (Shift+Enter), so the common case costs one keystroke.

import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  Search,
  Clipboard,
  Link2,
  Loader2,
  Settings as SettingsIcon,
  Minus,
  Square,
  X,
  Gauge,
  Sparkles,
  Leaf,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { LogoMark } from './Logo';

export type SpeedMode = 'quiet' | 'balanced' | 'turbo';

interface CommandBarProps {
  searchQuery: string;
  onSearch: (value: string) => void;
  onQuickAdd: (url: string) => Promise<void> | void;
  onOpenAddDialog: (url?: string) => void;
  speedMode: SpeedMode;
  onSpeedMode: (mode: SpeedMode) => void;
  onOpenSettings: () => void;
  settingsActive: boolean;
}

const MODES: { id: SpeedMode; icon: React.ElementType; key: string }[] = [
  { id: 'quiet', icon: Leaf, key: 'modeQuiet' },
  { id: 'balanced', icon: Gauge, key: 'modeBalanced' },
  { id: 'turbo', icon: Sparkles, key: 'modeTurbo' },
];

/** A magnet link or a .torrent URL — handled by the BitTorrent engine. */
function isTorrent(value: string): boolean {
  const trimmed = value.trim().toLowerCase();
  return trimmed.startsWith('magnet:') || trimmed.endsWith('.torrent');
}

/** A URL is "downloadable" if it is http(s), a magnet, or a .torrent. */
function looksLikeUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  if (isTorrent(trimmed) || (trimmed.length === 40 && /^[0-9a-f]+$/i.test(trimmed))) return true;
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

/** Last path segment, decoded — the guide's "Link detected · setup.exe". */
function guessName(url: string): string {
  const lower = url.trim().toLowerCase();
  if (lower.startsWith('magnet:')) {
    const match = url.match(/[?&]dn=([^&]+)/);
    return match ? decodeURIComponent(match[1]) : 'torrent';
  }
  if (lower.endsWith('.torrent')) {
    return url.split('/').pop() ?? 'torrent';
  }
  try {
    const last = decodeURIComponent(new URL(url).pathname.split('/').filter(Boolean).pop() || '');
    return last || url;
  } catch {
    return url;
  }
}

export const CommandBar: React.FC<CommandBarProps> = ({
  searchQuery,
  onSearch,
  onQuickAdd,
  onOpenAddDialog,
  speedMode,
  onSpeedMode,
  onOpenSettings,
  settingsActive,
}) => {
  const { t } = useI18n();
  const [value, setValue] = useState('');
  const [busy, setBusy] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const urlRef = useRef<HTMLInputElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const detected = looksLikeUrl(value);

  const commit = useCallback(
    async (openDialog: boolean) => {
      const url = value.trim();
      if (!looksLikeUrl(url)) return;

      if (openDialog) {
        onOpenAddDialog(url);
        setValue('');
        return;
      }

      setBusy(true);
      try {
        await onQuickAdd(url);
        setValue('');
      } finally {
        setBusy(false);
      }
    },
    [value, onQuickAdd, onOpenAddDialog]
  );

  // Ctrl+V anywhere focuses the URL field first, like the guide's shortcut list.
  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && /^(INPUT|TEXTAREA)$/.test(target.tagName)) return;

      const text = event.clipboardData?.getData('text') ?? '';
      if (!looksLikeUrl(text)) return;

      event.preventDefault();
      setValue(text.trim());
      urlRef.current?.focus();
    };

    window.addEventListener('paste', onPaste);
    return () => window.removeEventListener('paste', onPaste);
  }, []);

  // Ctrl+F opens the search field; Escape closes it.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault();
        setSearchOpen(true);
        window.setTimeout(() => searchRef.current?.focus(), 20);
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'l') {
        event.preventDefault();
        urlRef.current?.focus();
      }
      if (event.key === 'Escape' && searchOpen) {
        setSearchOpen(false);
        onSearch('');
      }
    };

    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [searchOpen, onSearch]);

  return (
    <header className="land-bar drag-region">
      {/* 1 — identity */}
      <div className="land-brand no-drag">
        <LogoMark size={20} />
        <span className="land-brand-name">ZDM</span>
      </div>

      {/* 2 — the URL field / search */}
      <div className="land-bar-center no-drag">
        {searchOpen ? (
          <div className="land-url">
            <Search size={13} className="land-url-icon" />
            <input
              ref={searchRef}
              value={searchQuery}
              onChange={(event) => onSearch(event.target.value)}
              placeholder={t('searchPlaceholder')}
              className="land-url-input"
              dir="auto"
            />
            <button
              type="button"
              className="land-url-clear"
              onClick={() => {
                setSearchOpen(false);
                onSearch('');
              }}
              aria-label={t('close')}
            >
              <X size={12} />
            </button>
          </div>
        ) : (
          <div className="land-url" data-detected={detected}>
            <Link2 size={13} className="land-url-icon" />
            <input
              ref={urlRef}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  void commit(event.shiftKey);
                }
                if (event.key === 'Escape') setValue('');
              }}
              placeholder={t('barPastePlaceholder')}
              className="land-url-input"
              dir="ltr"
              spellCheck={false}
            />

            {detected ? (
              <button
                type="button"
                className="land-url-add"
                onClick={() => void commit(false)}
                disabled={busy}
                title={t('barAddHint')}
              >
                {busy ? <Loader2 size={11} className="anim-spin" /> : <Clipboard size={11} />}
                <span className="land-url-name" dir="ltr">
                  {guessName(value)}
                </span>
                <span className="land-url-add-label">{t('barAdd')} ⏎</span>
              </button>
            ) : (
              <button
                type="button"
                className="land-url-search"
                onClick={() => {
                  setSearchOpen(true);
                  window.setTimeout(() => searchRef.current?.focus(), 20);
                }}
                title={t('searchPlaceholder')}
                aria-label={t('searchPlaceholder')}
              >
                <Search size={12} />
              </button>
            )}
          </div>
        )}
      </div>

      {/* 3 — speed mode, settings, window controls */}
      <div className="land-modes no-drag" role="group" aria-label={t('barSpeedMode')}>
        {MODES.map((mode) => {
          const Icon = mode.icon;
          const active = speedMode === mode.id;
          return (
            <button
              key={mode.id}
              type="button"
              onClick={() => onSpeedMode(mode.id)}
              className="land-mode"
              data-active={active}
              aria-pressed={active}
            >
              <Icon size={11} />
              <span>{t(mode.key as never)}</span>
            </button>
          );
        })}
      </div>

      <div className="land-win no-drag">
        <button
          type="button"
          className="land-win-btn"
          data-active={settingsActive}
          onClick={onOpenSettings}
          title={t('navSettings')}
          aria-label={t('navSettings')}
        >
          <SettingsIcon size={13} />
        </button>
        <span className="land-win-sep" aria-hidden="true" />
        <button
          type="button"
          className="land-win-btn"
          onClick={() => void import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().minimize())}
          title={t('minimize')}
          aria-label={t('minimize')}
        >
          <Minus size={13} />
        </button>
        <button
          type="button"
          className="land-win-btn"
          onClick={() =>
            void import('@tauri-apps/api/window').then(async (m) => {
              const w = m.getCurrentWindow();
              const max = await w.isMaximized();
              await (max ? w.unmaximize() : w.maximize());
            })
          }
          title={t('maximize')}
          aria-label={t('maximize')}
        >
          <Square size={11} />
        </button>
        <button
          type="button"
          className="land-win-btn land-win-close"
          onClick={() => void import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().close())}
          title={t('close')}
          aria-label={t('close')}
        >
          <X size={13} />
        </button>
      </div>
    </header>
  );
};
