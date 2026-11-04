/**
 * ZDM i18n runtime.
 * Developed by Ramy Badraan
 *
 * A tiny context instead of a library: one provider, one hook, zero bundle cost.
 * `t()` is memoised per language so re-renders never rebuild strings.
 */

import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { DICTS, Dict, Key, Lang, detectLang, dirFor } from './dict';

interface I18nValue {
  lang: Lang;
  dir: 'rtl' | 'ltr';
  isRtl: boolean;
  t: (key: Key) => string;
  setLang: (lang: Lang) => void;
  /** Format bytes/speed numbers consistently for the active language. */
  formatNumber: (value: number, digits?: number) => string;
}

const I18nContext = createContext<I18nValue | null>(null);

export const I18nProvider: React.FC<{
  children: React.ReactNode;
  initial?: string;
  onChange?: (lang: Lang) => void;
}> = ({ children, initial, onChange }) => {
  const [lang, setLangState] = useState<Lang>(() => detectLang(initial));

  // Keep <html> in sync: direction drives every layout decision downstream.
  useEffect(() => {
    const dir = dirFor(lang);
    document.documentElement.setAttribute('dir', dir);
    document.documentElement.setAttribute('lang', lang);
  }, [lang]);

  useEffect(() => {
    if (initial && (initial === 'ar' || initial === 'en' || initial === 'fr') && initial !== lang) {
      setLangState(initial);
    }
    // Intentionally only reacts to the persisted value.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initial]);

  const setLang = useCallback(
    (next: Lang) => {
      setLangState(next);
      onChange?.(next);
    },
    [onChange]
  );

  const dict: Dict = DICTS[lang];

  const t = useCallback((key: Key) => dict[key] ?? DICTS.en[key] ?? key, [dict]);

  const formatNumber = useCallback(
    (value: number, digits = 0) => {
      // Western digits in every language: technical values read better and
      // Arabic-Indic digits would break speed/size scanning in a list.
      const locale = lang === 'fr' ? 'fr-FR' : lang === 'ar' ? 'en-US' : 'en-US';
      try {
        return new Intl.NumberFormat(locale, {
          minimumFractionDigits: digits,
          maximumFractionDigits: digits,
        }).format(value);
      } catch {
        return value.toFixed(digits);
      }
    },
    [lang]
  );

  const value = useMemo<I18nValue>(
    () => ({
      lang,
      dir: dirFor(lang),
      isRtl: lang === 'ar',
      t,
      setLang,
      formatNumber,
    }),
    [lang, t, setLang, formatNumber]
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
};

export function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error('useI18n must be used inside <I18nProvider>');
  return ctx;
}
