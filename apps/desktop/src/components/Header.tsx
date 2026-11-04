//! ZDM Header
//! Developed by Ramy Badraan
//!
//! Search, the primary action, and two compact pickers (language, theme). The
//! bar is deliberately quiet: one accent, one primary button, no decoration.

import React, { useState } from 'react';
import { Plus, Search, Film, X, Languages, Sun, Moon, Monitor, Pause } from 'lucide-react';
import { useI18n } from '../i18n';
import { LANGS, Lang } from '../i18n/dict';

interface HeaderProps {
  searchQuery: string;
  onSearch: (value: string) => void;
  onAddUrl: () => void;
  onVideo: () => void;
  onPauseAll?: () => void;
  theme?: string;
  onThemeChange?: (theme: string) => void;
}

export const Header: React.FC<HeaderProps> = ({
  searchQuery,
  onSearch,
  onAddUrl,
  onVideo,
  onPauseAll,
  theme = 'moon',
  onThemeChange,
}) => {
  const { t, lang, setLang } = useI18n();
  const [langOpen, setLangOpen] = useState(false);

  const current = LANGS.find((option) => option.code === lang);

  const THEMES: { id: string; icon: React.ElementType; label: string }[] = [
    { id: 'moon', icon: Moon, label: t('themeMoon') },
    { id: 'dawn', icon: Sun, label: t('themeDawn') },
    { id: 'system', icon: Monitor, label: t('themeSystem') },
  ];

  return (
    <header className="h-[54px] shrink-0 px-3.5 flex items-center gap-2 border-b border-line bg-canvas/80 glass">
      {/* Search */}
      <div className="relative flex-1 max-w-[420px]">
        <Search
          size={14}
          className="absolute top-1/2 -translate-y-1/2 start-3 text-inkMuted pointer-events-none"
        />
        <input
          type="search"
          value={searchQuery}
          onChange={(event) => onSearch(event.target.value)}
          placeholder={t('searchPlaceholder')}
          aria-label={t('searchPlaceholder')}
          className="field h-8 ps-8.5 pe-8 text-[12px]"
          style={{ paddingInlineStart: 32 }}
        />
        {searchQuery ? (
          <button
            type="button"
            onClick={() => onSearch('')}
            aria-label={t('close')}
            className="absolute top-1/2 -translate-y-1/2 end-2 text-inkMuted hover:text-ink"
          >
            <X size={13} />
          </button>
        ) : null}
      </div>

      <div className="flex-1" />

      {/* Theme switch: three states, always visible so the light mode is findable. */}
      {onThemeChange ? (
        <div className="segment" role="group" aria-label={t('themeLabel')}>
          {THEMES.map(({ id, icon: Icon, label }) => (
            <button
              key={id}
              type="button"
              onClick={() => onThemeChange(id)}
              aria-pressed={theme === id}
              title={label}
              aria-label={label}
              className="flex items-center justify-center gap-1 min-w-[30px]"
            >
              <Icon size={13} />
            </button>
          ))}
        </div>
      ) : null}

      {/* Language */}
      <div className="relative">
        <button
          type="button"
          onClick={() => setLangOpen((open) => !open)}
          aria-haspopup="listbox"
          aria-expanded={langOpen}
          aria-label={t('languageLabel')}
          className="btn-ghost h-8 px-2 flex items-center gap-1.5 text-[13px] font-bold"
        >
          <Languages size={13} />
          <span>{current?.short ?? lang.toUpperCase()}</span>
        </button>

        {langOpen ? (
          <>
            <div className="fixed inset-0 z-30" onClick={() => setLangOpen(false)} />
            <ul
              role="listbox"
              className="absolute end-0 mt-1.5 z-40 w-36 panel p-1 shadow-pop anim-rise"
            >
              {LANGS.map((option) => (
                <li key={option.code}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={lang === option.code}
                    onClick={() => {
                      setLang(option.code as Lang);
                      setLangOpen(false);
                    }}
                    className={`w-full text-start px-2.5 py-1.5 rounded-md text-[12px] font-semibold transition-colors ${
                      lang === option.code
                        ? 'bg-panelHover text-ink'
                        : 'text-inkSoft hover:bg-panelHover/60'
                    }`}
                  >
                    {option.native}
                  </button>
                </li>
              ))}
            </ul>
          </>
        ) : null}
      </div>

      {onPauseAll ? (
        <button
          type="button"
          onClick={onPauseAll}
          className="btn-ghost h-8 px-2.5 flex items-center gap-1.5 text-[13.5px] font-bold"
          title={t('pauseAll')}
          aria-label={t('pauseAll')}
        >
          <Pause size={13} />
          <span className="hidden xl:inline">{t('pauseAll')}</span>
        </button>
      ) : null}

      <button
        type="button"
        onClick={onVideo}
        className="btn-ghost h-8 px-2.5 flex items-center gap-1.5 text-[13.5px] font-bold"
        title={t('videoTitle')}
      >
        <Film size={13} />
        <span className="hidden lg:inline">{t('navVideo')}</span>
      </button>

      <button
        type="button"
        onClick={onAddUrl}
        className="btn-primary h-8 px-3 flex items-center gap-1.5 text-[12px]"
      >
        <Plus size={14} strokeWidth={3} />
        <span>{t('addUrl')}</span>
      </button>
    </header>
  );
};
