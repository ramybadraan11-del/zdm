//! DownloadCard Component with Multi-Segment Visualizer
//! Developed by Ramy Badraan

import React from 'react';
import {
  Play,
  Pause,
  FolderOpen,
  FileCheck,
  Trash2,
  Film,
  Music,
  Archive,
  FileText,
  Package,
  Layers,
  Maximize2,
} from 'lucide-react';
import { DownloadItem } from '../types';
import { formatBytes, formatSpeed, formatTime, getCategoryBadgeColor } from '../utils/format';

interface DownloadCardProps {
  item: DownloadItem;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onDelete: (id: string) => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
  onOpenProgressModal?: (id: string) => void;
}

export const DownloadCard: React.FC<DownloadCardProps> = ({
  item,
  onPause,
  onResume,
  onDelete,
  onOpenFile,
  onOpenFolder,
  onOpenProgressModal,
}) => {
  const isDownloading = item.status === 'downloading';
  const isCompleted = item.status === 'completed';
  const isPaused = item.status === 'paused';
  const badgeColors = getCategoryBadgeColor(item.category);

  // Extract host domain from URL
  let domain = '';
  try {
    domain = new URL(item.url).hostname;
  } catch {
    domain = 'server';
  }

  const renderCategoryIcon = () => {
    switch (item.category.toLowerCase()) {
      case 'video':
        return <Film className="w-5 h-5 text-rose-400" />;
      case 'audio':
        return <Music className="w-5 h-5 text-amber-400" />;
      case 'compressed':
        return <Archive className="w-5 h-5 text-purple-400" />;
      case 'documents':
        return <FileText className="w-5 h-5 text-blue-400" />;
      case 'programs':
        return <Package className="w-5 h-5 text-emerald-400" />;
      default:
        return <Layers className="w-5 h-5 text-cyan-400" />;
    }
  };

  return (
    <div className="bg-[#121933] border border-[#1E294D] hover:border-cyan-500/40 rounded-2xl p-4 transition-all duration-200 group">
      {/* Top Details Row */}
      <div className="flex items-start justify-between gap-4 mb-3">
        {/* Left: Icon & Title */}
        <div className="flex items-center gap-3.5 flex-1 min-w-0">
          <div
            className={`w-12 h-12 rounded-xl flex items-center justify-center border shrink-0 ${badgeColors.bg} ${badgeColors.border}`}
          >
            {renderCategoryIcon()}
          </div>
          <div className="min-w-0 flex-1">
            <h3
              className="text-sm font-bold text-slate-100 truncate hover:text-cyan-400 transition-colors cursor-pointer text-left"
              dir="ltr"
              title={item.filename}
              onClick={() => isCompleted && onOpenFile(item.save_path)}
            >
              {item.filename}
            </h3>
            <div className="flex items-center gap-2 text-xs text-slate-400 mt-0.5">
              <span className="font-mono text-slate-300 font-semibold">{formatBytes(item.total_size)}</span>
              <span>•</span>
              <span className="truncate text-slate-400" dir="ltr">{domain}</span>
              <span>•</span>
              <span className="text-cyan-400 font-mono font-medium">{item.connections} قنوات توازي</span>
              {item.supports_range && (
                <span className="text-[10px] px-1.5 py-0.2 rounded bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 font-mono">
                  Range 206
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Right: Actions */}
        <div className="flex items-center gap-1.5 shrink-0">
          {isDownloading ? (
            <button
              onClick={() => onPause(item.id)}
              title="إيقاف مؤقت"
              className="w-9 h-9 rounded-xl bg-[#1A2346] hover:bg-amber-500/20 hover:text-amber-400 text-slate-300 flex items-center justify-center transition-colors cursor-pointer"
            >
              <Pause className="w-4 h-4 fill-current" />
            </button>
          ) : isCompleted ? (
            <button
              onClick={() => onOpenFile(item.save_path)}
              title="فتح الملف"
              className="w-9 h-9 rounded-xl bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-400 flex items-center justify-center transition-colors cursor-pointer"
            >
              <FileCheck className="w-4 h-4" />
            </button>
          ) : (
            <button
              onClick={() => onResume(item.id)}
              title="استئناف التنزيل"
              className="w-9 h-9 rounded-xl bg-[#1A2346] hover:bg-cyan-500/20 hover:text-cyan-400 text-slate-300 flex items-center justify-center transition-colors cursor-pointer"
            >
              <Play className="w-4 h-4 fill-current" />
            </button>
          )}

          <button
            onClick={() => onOpenFolder(item.save_path)}
            title="فتح مجلد الحفظ"
            className="w-9 h-9 rounded-xl bg-[#1A2346] hover:bg-cyan-500/20 hover:text-cyan-400 text-slate-400 flex items-center justify-center transition-colors cursor-pointer"
          >
            <FolderOpen className="w-4 h-4" />
          </button>

          {onOpenProgressModal && (
            <button
              onClick={() => onOpenProgressModal(item.id)}
              title="نافذة تفاصيل التنزيل العائمة"
              className="w-9 h-9 rounded-xl bg-[#1A2346] hover:bg-cyan-500/20 hover:text-cyan-400 text-slate-400 flex items-center justify-center transition-colors cursor-pointer"
            >
              <Maximize2 className="w-4 h-4" />
            </button>
          )}

          <button
            onClick={() => onDelete(item.id)}
            title="حذف من القائمة"
            className="w-9 h-9 rounded-xl bg-[#1A2346] hover:bg-rose-500/20 hover:text-rose-400 text-slate-400 flex items-center justify-center transition-colors cursor-pointer"
          >
            <Trash2 className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Progress & Speed Metrics */}
      <div className="flex items-center justify-between text-xs mb-2">
        <div className="flex items-center gap-3">
          <span className={`font-mono font-bold text-sm ${isCompleted ? 'text-emerald-400' : 'text-cyan-400'}`}>
            {isCompleted ? '100.0%' : `${item.progress_percent.toFixed(1)}%`}
          </span>
          <span className="text-slate-400 font-mono">
            {isCompleted
              ? `${formatBytes(item.total_size)} / ${formatBytes(item.total_size)}`
              : `${formatBytes(item.downloaded_size)} / ${formatBytes(item.total_size)}`}
          </span>
        </div>
        <div className="flex items-center gap-3 font-mono">
          {isDownloading && (
            <>
              <span className="text-cyan-400 font-bold">{formatSpeed(item.speed_bps)}</span>
              <span className="text-slate-500">•</span>
              <span className="text-slate-400">متبقي: {formatTime(item.eta_seconds)}</span>
            </>
          )}
          {isCompleted && <span className="text-emerald-400 font-bold">✓ اكتمل التنزيل بنجاح</span>}
          {isPaused && <span className="text-amber-400 font-bold">⏸ متوقف مؤقتاً</span>}
        </div>
      </div>

      {/* Multi-Segment Parallel Stream Visualizer */}
      <div className="space-y-1.5">
        {item.segments && item.segments.length > 1 ? (
          <div className="flex gap-1 h-3.5 bg-[#0C1226] p-0.5 rounded-lg border border-[#1A2449] overflow-hidden">
            {item.segments.map((seg) => {
              const segPercent =
                seg.total_bytes > 0
                  ? Math.min(100, (seg.downloaded_bytes / seg.total_bytes) * 100)
                  : 0;
              const isFinished = seg.is_finished || segPercent >= 100;
              const isActive = seg.is_active || (isDownloading && !isFinished);

              return (
                <div
                  key={seg.index}
                  className="flex-1 bg-[#141B34] rounded relative overflow-hidden h-full"
                  title={`قناة ${seg.index + 1}: ${segPercent.toFixed(0)}% (${formatBytes(seg.downloaded_bytes)})`}
                >
                  <div
                    className={`h-full transition-all duration-150 rounded ${
                      isFinished
                        ? 'bg-gradient-to-r from-emerald-400 to-cyan-400'
                        : isActive
                        ? 'bg-gradient-to-r from-cyan-400 via-purple-500 to-pink-500 animate-pulse'
                        : 'bg-cyan-500/40'
                    }`}
                    style={{ width: `${segPercent}%` }}
                  />
                </div>
              );
            })}
          </div>
        ) : (
          /* Single Connection Progress Bar */
          <div className="w-full h-3 bg-[#0C1226] rounded-full overflow-hidden border border-[#1A2449]">
            <div
              className={`h-full rounded-full transition-all duration-150 ${
                isCompleted
                  ? 'bg-emerald-400'
                  : 'bg-gradient-to-r from-cyan-400 via-purple-500 to-pink-500'
              }`}
              style={{ width: isCompleted ? '100%' : `${Math.min(100, Math.max(0, item.progress_percent))}%` }}
            />
          </div>
        )}
      </div>
    </div>
  );
};
