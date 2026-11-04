import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { TransferWindow } from './components/TransferWindow';
import { DownloadPrompt } from './components/DownloadPrompt';
import { I18nProvider } from './i18n';
import { invoke } from '@tauri-apps/api/core';
import type { Settings } from './types';
import './styles/index.css';

/**
 * One bundle renders two windows:
 *   index.html                → the manager
 *   index.html?window=transfer&id=<uuid> → a single transfer
 *
 * Splitting into two HTML entry points would double the bundle and the build
 * time for a 12 KB component, so the route is a query string.
 */
const params = new URLSearchParams(window.location.search);
const windowKind = params.get('window');
const isTransfer = windowKind === 'transfer';
const isPrompt = windowKind === 'prompt';
const transferId = params.get('id') ?? '';

const Root: React.FC = () => {
  const [lang, setLang] = React.useState<string | undefined>(undefined);
  const [ready, setReady] = React.useState(false);
  const [theme, setTheme] = React.useState('moon');

  React.useEffect(() => {
    invoke<Settings>('get_settings')
      .then((settings) => {
        setLang(settings.language || undefined);
        setTheme(settings.theme || 'moon');
      })
      .catch(() => undefined)
      .finally(() => setReady(true));
  }, []);

  // Both windows must honour the theme.
  React.useEffect(() => {
    document.documentElement.setAttribute(
      'data-theme',
      ['moon', 'dawn', 'system'].includes(theme) ? theme : 'moon'
    );
  }, [theme]);

  if (!ready) {
    return (
      <div className="h-screen grid place-items-center bg-base">
        <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-accent1 to-accent3 opacity-70 animate-pulse" />
      </div>
    );
  }

  if (isPrompt) {
    return (
      <I18nProvider initial={lang}>
        <DownloadPrompt />
      </I18nProvider>
    );
  }

  if (isTransfer && transferId) {
    return (
      <I18nProvider initial={lang}>
        <TransferWindow downloadId={transferId} />
      </I18nProvider>
    );
  }

  return <App />;
};

const tree = import.meta.env.DEV ? (
  <React.StrictMode>
    <Root />
  </React.StrictMode>
) : (
  <Root />
);

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(tree);

// Remove the static boot splash as soon as React has painted its first frame.
requestAnimationFrame(() => {
  const boot = document.getElementById('boot');
  if (!boot) return;
  boot.classList.add('gone');
  window.setTimeout(() => boot.remove(), 260);
});
