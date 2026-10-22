//! DownloadProgressModal Component (IDM Style Floating Download Window)
//! Developed by Ramy Badraan

import React from 'react';
import {
  Download,
  Pause,
  Play,
  X,
  Minimize2,
  FolderOpen,
  CheckCircle2,
  Zap,
  Clock,
  HardDrive,
  Film,
  Music,
  FileArchive,
  FileText,
  File,
  ExternalLink,
} from 'lucide-react';
import { DownloadItem } from '../types';
import { formatBytes, formatSpeed, formatTime } from '../utils/format';

interface DownloadProgressModalProps {
  isOpen: boolean;
  onClose: () => void;
  download: DownloadItem | null;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
}

export const DownloadProgressModal: React.FC<DownloadProgressModalProps> = ({
  isOpen,
  onClose,
  download,
  onPause,
  onResume,
  onCancel,
  onOpenFile,
  onOpenFolder,
}) => {
  if (!isOpen || !download) return null;

  const isCompleted = download.status === 'completed';
  const isPaused = download.status === 'paused';
  const isDownloading = download.status === 'downloading';

  const isVideo =
    download.category === 'Video' ||
    download.filename.match(/\.(mp4|mkv|webm|avi|mov|flv|m4v)$/i);
  const isAudio =
    download.category === 'Audio' ||
    download.filename.match(/\.(mp3|m4a|wav|aac|flac|ogg)$/i);
  const isArchive =
    download.category === 'Compressed' ||
    download.filename.match(/\.(zip|rar|7z|tar|gz|bz2)$/i);
  const isDoc = download.filename.match(/\.(pdf|doc|docx|xls|xlsx|txt)$/i);

  const renderIcon = () => {
    if (isVideo) return <Film className="w-7 h-7 text-cyan-400" />;
    if (isAudio) return <Music className="w-7 h-7 text-amber-400" />;
    if (isArchive) return <FileArchive className="w-7 h-7 text-purple-400" />;
    if (isDoc) return <FileText className="w-7 h-7 text-emerald-400" />;
    return <File className="w-7 h-7 text-blue-400" />;
  };

  const displayPercent = isCompleted
    ? 100
    : Math.min(100, Math.max(0, download.progress_percent));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-md select-none animate-fadeIn">
      <div className="bg-[#101730] border border-cyan-500/40 rounded-3xl w-full max-w-xl p-6 shadow-2xl relative glow-cyan space-y-5">
        {/* Top Header Bar */}
        <div className="flex items-center justify-between border-b border-[#1E294D] pb-3.5">
          <div className="flex items-center gap-2.5">
            <div
              className={`w-9 h-9 rounded-xl flex items-center justify-center border shadow-md ${
                isCompleted
                  ? 'bg-emerald-500/20 border-emerald-500/40 text-emerald-400'
                  : 'bg-cyan-500/20 border-cyan-500/40 text-cyan-400 animate-pulse'
              }`}
            >
              {isCompleted ? <CheckCircle2 className="w-5 h-5" /> : <Download className="w-5 h-5" />}
            </div>
            <div>
              <h2 className="text-sm font-black text-white">
                {isCompleted ? 'اكتمل التنزيل بنجاح ✓' : 'ZDM Turbo Downloader — جارٍ التنزيل'}
              </h2>
              <span className="text-[11px] text-slate-400 font-mono">
                {isCompleted ? 'جاهز للفتح والاستخدام' : 'محرك التسريع الموازي قيد التشغيل'}
              </span>
            </div>
          </div>

          <div className="flex items-center gap-1.5">
            <button
              onClick={onClose}
              title="إخفاء في شريط المهام (متابعة التنزيل بالخلفية)"
              className="w-8 h-8 rounded-xl bg-[#1A2346] hover:bg-[#253262] text-slate-400 hover:text-white flex items-center justify-center transition-colors cursor-pointer"
            >
              <Minimize2 className="w-4 h-4" />
            </button>
            <button
              onClick={() => {
                if (!isCompleted) onCancel(download.id);
                onClose();
              }}
              title="إغلاق"
              className="w-8 h-8 rounded-xl bg-[#1A2346] hover:bg-rose-500/20 hover:text-rose-400 text-slate-400 flex items-center justify-center transition-colors cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* File Overview Card */}
        <div className="p-3.5 rounded-2xl bg-[#0B1020] border border-[#202C55] flex items-center gap-3.5">
          <div className="w-12 h-12 rounded-xl bg-[#141C38] border border-[#263566] flex items-center justify-center shrink-0">
            {renderIcon()}
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="text-xs font-bold text-white truncate mb-1" title={download.filename} dir="ltr">
              {download.filename}
            </h3>
            <div className="flex items-center gap-2 text-[11px] text-slate-400">
              <span className="font-mono text-cyan-400 font-bold">
                {formatBytes(isCompleted ? download.total_size : download.downloaded_size)} / {formatBytes(download.total_size)}
              </span>
              <span>•</span>
              <span className="truncate max-w-[200px]" dir="ltr">{download.url}</span>
            </div>
          </div>
          <div className="text-left shrink-0 font-mono text-xl font-black text-white">
            <span className={isCompleted ? 'text-emerald-400' : 'text-cyan-400'}>
              {displayPercent.toFixed(1)}%
            </span>
          </div>
        </div>

        {/* Live Multi-Segment Parallel Bar / Progress Visualizer */}
        <div className="space-y-2">
          {download.segments && download.segments.length > 1 ? (
            <div className="flex gap-1 h-4 bg-[#0A0E1C] p-0.5 rounded-xl border border-[#1A2449] overflow-hidden">
              {download.segments.map((seg) => {
                const segPct = seg.total_bytes > 0
                  ? Math.min(100, (seg.downloaded_bytes / seg.total_bytes) * 100)
                  : 0;
                const isFinished = seg.is_finished || segPct >= 100;

                return (
                  <div
                    key={seg.index}
                    className="flex-1 bg-[#141B34] rounded-lg relative overflow-hidden h-full"
                    title={`قناة ${seg.index + 1}: ${segPct.toFixed(0)}%`}
                  >
                    <div
                      className={`h-full transition-all duration-150 rounded-lg ${
                        isFinished
                          ? 'bg-gradient-to-r from-emerald-400 to-cyan-400'
                          : isDownloading
                          ? 'bg-gradient-to-r from-cyan-400 via-purple-500 to-pink-500 animate-pulse'
                          : 'bg-cyan-500/40'
                      }`}
                      style={{ width: `${segPct}%` }}
                    />
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="w-full h-3.5 bg-[#0A0E1C] rounded-full overflow-hidden border border-[#1A2449] p-0.5">
              <div
                className={`h-full rounded-full transition-all duration-200 ${
                  isCompleted
                    ? 'bg-emerald-400 shadow-lg glow-emerald'
                    : 'bg-gradient-to-r from-cyan-400 via-purple-500 to-pink-500 animate-pulse'
                }`}
                style={{ width: `${displayPercent}%` }}
              />
            </div>
          )}
        </div>

        {/* Live Metrics Grid */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
          <div className="p-3 rounded-2xl bg-[#0B1020] border border-[#202C55] flex flex-col justify-between">
            <div className="flex items-center gap-1.5 text-[11px] text-slate-400">
              <Zap className="w-3.5 h-3.5 text-cyan-400" />
              <span>السرعة اللحظية</span>
            </div>
            <span className="text-xs font-mono font-black text-cyan-400 mt-1">
              {isCompleted ? 'مكتمل ✓' : isPaused ? 'متوقف مؤقتاً' : formatSpeed(download.speed_bps)}
            </span>
          </div>

          <div className="p-3 rounded-2xl bg-[#0B1020] border border-[#202C55] flex flex-col justify-between">
            <div className="flex items-center gap-1.5 text-[11px] text-slate-400">
              <Clock className="w-3.5 h-3.5 text-purple-400" />
              <span>الوقت المتبقي</span>
            </div>
            <span className="text-xs font-mono font-bold text-slate-200 mt-1">
              {isCompleted ? '00:00' : isPaused ? '--:--' : formatTime(download.eta_seconds)}
            </span>
          </div>

          <div className="p-3 rounded-2xl bg-[#0B1020] border border-[#202C55] flex flex-col justify-between">
            <div className="flex items-center gap-1.5 text-[11px] text-slate-400">
              <HardDrive className="w-3.5 h-3.5 text-amber-400" />
              <span>الحجم الإجمالي</span>
            </div>
            <span className="text-xs font-mono font-bold text-slate-200 mt-1">
              {formatBytes(download.total_size)}
            </span>
          </div>

          <div className="p-3 rounded-2xl bg-[#0B1020] border border-[#202C55] flex flex-col justify-between">
            <div className="flex items-center gap-1.5 text-[11px] text-slate-400">
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
              <span>قنوات التوازي</span>
            </div>
            <span className="text-xs font-mono font-bold text-emerald-400 mt-1">
              {download.connections > 1 ? `${download.connections} Streams` : 'Direct Engine'}
            </span>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="pt-2">
          {isCompleted ? (
            <div className="grid grid-cols-2 gap-3">
              <button
                type="button"
                onClick={() => {
                  onOpenFile(download.save_path);
                  onClose();
                }}
                className="py-3 px-4 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 hover:opacity-95 text-slate-950 font-black text-xs flex items-center justify-center gap-2 glow-cyan transition-all active:scale-95 cursor-pointer shadow-lg"
              >
                <ExternalLink className="w-4 h-4 stroke-[2.5]" />
                <span>فتح الملف الآن</span>
              </button>

              <button
                type="button"
                onClick={() => {
                  onOpenFolder(download.save_path);
                  onClose();
                }}
                className="py-3 px-4 rounded-xl bg-[#1A2346] hover:bg-[#253262] text-slate-200 border border-[#2A3764] font-bold text-xs flex items-center justify-center gap-2 transition-all active:scale-95 cursor-pointer"
              >
                <FolderOpen className="w-4 h-4 text-cyan-400" />
                <span>فتح مجلد الحفظ</span>
              </button>
            </div>
          ) : (
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                {isDownloading ? (
                  <button
                    type="button"
                    onClick={() => onPause(download.id)}
                    className="py-2.5 px-4 rounded-xl bg-[#1A2346] hover:bg-amber-500/20 text-slate-200 hover:text-amber-300 border border-[#2A3764] font-bold text-xs flex items-center gap-1.5 transition-colors cursor-pointer"
                  >
                    <Pause className="w-3.5 h-3.5 fill-current" />
                    <span>إيقاف مؤقت</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => onResume(download.id)}
                    className="py-2.5 px-4 rounded-xl bg-[#1A2346] hover:bg-cyan-500/20 text-slate-200 hover:text-cyan-300 border border-[#2A3764] font-bold text-xs flex items-center gap-1.5 transition-colors cursor-pointer"
                  >
                    <Play className="w-3.5 h-3.5 fill-current" />
                    <span>استئناف التنزيل</span>
                  </button>
                )}

                <button
                  type="button"
                  onClick={() => {
                    onCancel(download.id);
                    onClose();
                  }}
                  className="py-2.5 px-4 rounded-xl bg-[#1A2346] hover:bg-rose-500/20 text-slate-300 hover:text-rose-300 border border-[#2A3764] font-bold text-xs flex items-center gap-1.5 transition-colors cursor-pointer"
                >
                  <X className="w-3.5 h-3.5" />
                  <span>إلغاء</span>
                </button>
              </div>

              <button
                type="button"
                onClick={onClose}
                className="py-2.5 px-4 rounded-xl bg-gradient-to-r from-cyan-500/20 to-purple-600/20 hover:from-cyan-500/30 text-cyan-300 border border-cyan-500/40 text-xs font-bold transition-all cursor-pointer"
              >
                <span>متابعة بالخلفية</span>
              </button>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-[#1A2449] pt-2.5 text-[11px] text-slate-500">
          <span>Developed by Ramy Badraan</span>
          <span className="font-mono text-slate-400 truncate max-w-[280px]" dir="ltr">
            {download.save_path}
          </span>
        </div>
      </div>
    </div>
  );
};
