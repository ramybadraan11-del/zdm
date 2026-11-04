//! ZDM Shared UI Primitives
//! Developed by Ramy Badraan
//!
//! Small, boring, accessible building blocks. Keeping them here means the
//! screens stay readable and every modal/panel shares identical behaviour.

import React, { useEffect, useRef } from 'react';
import { X, Check, AlertTriangle, Info } from 'lucide-react';
import { useI18n } from '../i18n';

// --------------------------------------------------------------------- Modal

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
  footer?: React.ReactNode;
  width?: 'sm' | 'md' | 'lg';
  /** Disable closing on backdrop/Esc while a task is in flight. */
  locked?: boolean;
}

const WIDTHS: Record<NonNullable<ModalProps['width']>, string> = {
  sm: 'max-w-md',
  md: 'max-w-xl',
  lg: 'max-w-3xl',
};

export const Modal: React.FC<ModalProps> = ({
  open,
  onClose,
  title,
  subtitle,
  icon,
  children,
  footer,
  width = 'md',
  locked = false,
}) => {
  const { t } = useI18n();
  const panelRef = useRef<HTMLDivElement>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;

    previouslyFocused.current = document.activeElement as HTMLElement | null;

    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !locked) {
        event.preventDefault();
        onClose();
      }
      if (event.key !== 'Tab' || !panelRef.current) return;

      // Focus trap: keep keyboard users inside the dialog.
      const focusables = panelRef.current.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), textarea, input, select, [tabindex]:not([tabindex="-1"])'
      );
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener('keydown', onKey);
    const timer = window.setTimeout(() => {
      const target = panelRef.current?.querySelector<HTMLElement>(
        'input, textarea, button:not([data-close])'
      );
      target?.focus();
    }, 60);

    return () => {
      document.removeEventListener('keydown', onKey);
      window.clearTimeout(timer);
      previouslyFocused.current?.focus?.();
    };
  }, [open, onClose, locked]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-5 anim-fade"
      style={{
        // Scrim follows the theme: rgba of --bg-scrim keeps contrast in light mode.
        background: 'rgb(var(--bg-scrim) / 0.62)',
        backdropFilter: 'blur(10px)',
      }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !locked) onClose();
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={`panel edge-light w-full ${WIDTHS[width]} anim-rise shadow-pop max-h-[88vh] flex flex-col`}
      >
        <header className="flex items-start gap-3 px-5 pt-5 pb-4 border-b border-line">
          {icon ? (
            <div className="w-10 h-10 rounded-xl grid place-items-center bg-panelHover text-accent2 shrink-0">
              {icon}
            </div>
          ) : null}
          <div className="min-w-0 flex-1">
            <h2 className="text-[15px] font-extrabold text-ink leading-tight">{title}</h2>
            {subtitle ? (
              <p className="text-[13.5px] text-inkMuted mt-0.5 leading-snug">{subtitle}</p>
            ) : null}
          </div>
          <button
            type="button"
            data-close
            onClick={onClose}
            disabled={locked}
            aria-label={t('close')}
            className="btn-icon shrink-0 disabled:opacity-30"
          >
            <X size={16} />
          </button>
        </header>

        <div className="px-5 py-4 overflow-y-auto flex-1">{children}</div>

        {footer ? (
          <footer className="px-5 py-4 border-t border-line flex items-center justify-end gap-2">
            {footer}
          </footer>
        ) : null}
      </div>
    </div>
  );
};

// ------------------------------------------------------------------- Toasts

export type ToastKind = 'success' | 'error' | 'info';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

const TOAST_ICON: Record<ToastKind, React.ReactNode> = {
  success: <Check size={14} />,
  error: <AlertTriangle size={14} />,
  info: <Info size={14} />,
};

const TOAST_TONE: Record<ToastKind, string> = {
  success: 'border-emerald-400/35 text-emerald-400',
  error: 'border-danger/35 text-danger',
  info: 'border-accent2/35 text-accent2',
};

export const ToastStack: React.FC<{ toasts: Toast[]; onDismiss: (id: number) => void }> = ({
  toasts,
  onDismiss,
}) => (
  <div className="fixed bottom-5 left-1/2 -translate-x-1/2 z-[60] flex flex-col gap-2 items-center pointer-events-none">
    {toasts.map((toast) => (
      <div
        key={toast.id}
        role="status"
        className={`pointer-events-auto flex items-center gap-2 px-3.5 py-2 rounded-xl border glass shadow-pop anim-rise text-[12px] font-semibold ${TOAST_TONE[toast.kind]}`}
        onClick={() => onDismiss(toast.id)}
      >
        {TOAST_ICON[toast.kind]}
        <span className="max-w-[380px]">{toast.message}</span>
      </div>
    ))}
  </div>
);

// ------------------------------------------------------------------ Switch

export const Switch: React.FC<{
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
}> = ({ checked, onChange, disabled, label }) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    disabled={disabled}
    onClick={() => onChange(!checked)}
    className={`ui-switch relative w-10 h-[22px] rounded-full shrink-0 disabled:opacity-40 ${
      checked ? 'ui-switch-on' : ''
    }`}
  >
    <span className="ui-switch-knob" />
  </button>
);

// ------------------------------------------------------------------ Layout

export const SectionCard: React.FC<{
  title: string;
  icon?: React.ReactNode;
  action?: React.ReactNode;
  children: React.ReactNode;
}> = ({ title, icon, action, children }) => (
  <section className="panel p-4">
    <div className="flex items-center justify-between gap-3 pb-3 mb-3 border-b border-line">
      <div className="flex items-center gap-2 text-[12px] font-extrabold tracking-wide text-inkSoft">
        {icon ? <span className="text-accent2">{icon}</span> : null}
        <span>{title}</span>
      </div>
      {action}
    </div>
    {children}
  </section>
);

export const StatPill: React.FC<{
  label: string;
  value: string;
  tone?: 'accent' | 'success' | 'warning';
}> = ({ label, value, tone = 'accent' }) => {
  const toneClass =
    tone === 'success'
      ? 'text-emerald-300'
      : tone === 'warning'
      ? 'text-amber-300'
      : 'text-accent2';
  return (
    <div className="flex flex-col">
      <span className="text-[12px] font-bold uppercase tracking-[0.12em] text-inkMuted">{label}</span>
      <span className={`mono text-[13px] font-bold ${toneClass}`}>{value}</span>
    </div>
  );
};

/** Empty state used by every list screen. */
export const EmptyState: React.FC<{
  icon: React.ReactNode;
  title: string;
  body: string;
  action?: React.ReactNode;
}> = ({ icon, title, body, action }) => (
  <div className="flex flex-col items-center justify-center text-center py-16 px-6 anim-fade">
    <div className="w-16 h-16 rounded-2xl grid place-items-center bg-panel border border-line text-inkMuted mb-4">
      {icon}
    </div>
    <h3 className="text-[14px] font-extrabold text-ink">{title}</h3>
    <p className="text-[12px] text-inkMuted mt-1.5 max-w-sm leading-relaxed">{body}</p>
    {action ? <div className="mt-5">{action}</div> : null}
  </div>
);
