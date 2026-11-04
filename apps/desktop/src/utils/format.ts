//! ZDM Formatting Helpers
//! Developed by Ramy Badraan
//!
//! Locale-aware but deliberately technical: sizes use binary units (KiB-based
//! math, familiar MB labels), speeds are always "MB/s", and durations read as
//! clock values so a list can be scanned quickly.

export function formatBytes(bytes: number, digits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  const precision = exponent === 0 ? 0 : value >= 100 ? 0 : digits;
  return `${value.toFixed(precision)} ${units[exponent]}`;
}

export function formatSpeed(bps: number): string {
  if (!Number.isFinite(bps) || bps <= 0) return '—';
  return `${formatBytes(bps, 1)}/s`;
}

/** Remaining time as h:mm:ss / m:ss, or an em dash when unknown. */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return '—';
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;

  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }
  return `${minutes}:${String(secs).padStart(2, '0')}`;
}

/** Media length (not ETA): "1:23:45" style, same rules. */
export function formatMediaLength(seconds?: number): string {
  if (!seconds || seconds <= 0) return '—';
  return formatDuration(seconds);
}

export function formatPercent(value: number): string {
  if (!Number.isFinite(value)) return '0%';
  return `${Math.min(100, Math.max(0, value)).toFixed(value >= 10 ? 0 : 1)}%`;
}

/** Extract a readable host, or the raw scheme when parsing fails. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return 'unknown';
  }
}

export function baseName(path: string): string {
  if (!path) return '';
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** Short, calm badge palette per category (no more than one accent each). */
export function categoryAccent(category: string): {
  ring: string;
  text: string;
  glow: string;
} {
  switch (category.toLowerCase()) {
    case 'video':
      return { ring: 'border-rose-400/25 bg-rose-400/10', text: 'text-rose-300', glow: 'shadow-rose-500/10' };
    case 'audio':
      return { ring: 'border-amber-400/25 bg-amber-400/10', text: 'text-amber-300', glow: 'shadow-amber-500/10' };
    case 'programs':
      return { ring: 'border-emerald-400/25 bg-emerald-400/10', text: 'text-emerald-300', glow: 'shadow-emerald-500/10' };
    case 'compressed':
      return { ring: 'border-violet-400/25 bg-violet-400/10', text: 'text-violet-300', glow: 'shadow-violet-500/10' };
    case 'documents':
      return { ring: 'border-sky-400/25 bg-sky-400/10', text: 'text-sky-300', glow: 'shadow-sky-500/10' };
    case 'images':
      return { ring: 'border-fuchsia-400/25 bg-fuchsia-400/10', text: 'text-fuchsia-300', glow: 'shadow-fuchsia-500/10' };
    default:
      return { ring: 'border-slate-400/20 bg-slate-400/10', text: 'text-slate-300', glow: 'shadow-slate-500/10' };
  }
}

/** Human label for a status, resolved through the dictionary by the caller. */
export function statusTone(status: string): { dot: string; text: string } {
  switch (status) {
    case 'downloading':
      return { dot: 'bg-cyan-400', text: 'text-cyan-300' };
    case 'completed':
      return { dot: 'bg-emerald-400', text: 'text-emerald-300' };
    case 'paused':
      return { dot: 'bg-amber-400', text: 'text-amber-300' };
    case 'error':
      return { dot: 'bg-rose-400', text: 'text-rose-300' };
    case 'queued':
      return { dot: 'bg-slate-400', text: 'text-slate-300' };
    default:
      return { dot: 'bg-slate-500', text: 'text-slate-400' };
  }
}
