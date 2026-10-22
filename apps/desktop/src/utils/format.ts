//! Formatting Utilities
//! Developed by Ramy Badraan

export function formatBytes(bytes: number, decimals: number = 2): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  if (i < 0 || i >= sizes.length) return `${bytes} B`;
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

export function formatSpeed(bps: number): string {
  if (bps <= 0) return '0 KB/s';
  return `${formatBytes(bps)}/s`;
}

export function formatTime(seconds: number): string {
  if (seconds <= 0 || !isFinite(seconds)) return '--';
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  return `${hours}h ${remMins}m`;
}

export function getCategoryBadgeColor(category: string): { bg: string; text: string; border: string } {
  switch (category.toLowerCase()) {
    case 'video':
      return { bg: 'bg-rose-500/20', text: 'text-rose-400', border: 'border-rose-500/40' };
    case 'audio':
      return { bg: 'bg-amber-500/20', text: 'text-amber-400', border: 'border-amber-500/40' };
    case 'compressed':
      return { bg: 'bg-purple-500/20', text: 'text-purple-400', border: 'border-purple-500/40' };
    case 'documents':
      return { bg: 'bg-blue-500/20', text: 'text-blue-400', border: 'border-blue-500/40' };
    case 'programs':
      return { bg: 'bg-emerald-500/20', text: 'text-emerald-400', border: 'border-emerald-500/40' };
    default:
      return { bg: 'bg-cyan-500/20', text: 'text-cyan-400', border: 'border-cyan-500/40' };
  }
}
