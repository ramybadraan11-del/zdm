//! DownloadCompleteModal Component (IDM Style Completion Dialog)
//! Developed by Ramy Badraan

import React from 'react';
import {
  CheckCircle2,
  Film,
  Music,
  FileArchive,
  FileText,
  File,
  FolderOpen,
  Play,
  X,
  ExternalLink,
} from 'lucide-react';
import { formatBytes } from '../utils/format';

interface CompletedDownloadData {
  id: string;
  filename: string;
  save_path: string;
  total_size: number;
  category?: string;
}

interface DownloadCompleteModalProps {
  isOpen: boolean;
  onClose: () => void;
  download: CompletedDownloadData | null;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
}

export const DownloadCompleteModal: React.FC<DownloadCompleteModalProps> = ({
  isOpen,
  onClose,
  download,
  onOpenFile,
  onOpenFolder,
}) => {
  if (!isOpen || !download) return null;

  const isVideo =
    download.category === 'Video' ||
    download.filename.match(/\.(mp4|mkv|webm|avi|mov|flv)$/i);
  const isAudio =
    download.category === 'Audio' ||
    download.filename.match(/\.(mp3|m4a|wav|aac|flac|ogg)$/i);
  const isArchive =
    download.category === 'Compressed' ||
    download.filename.match(/\.(zip|rar|7z|tar|gz|bz2)$/i);
  const isDoc = download.filename.match(/\.(pdf|doc|docx|xls|xlsx|txt)$/i);

  const getIcon = () => {
    if (isVideo) return <Film className="w-8 h-8 text-cyan-400" />;
    if (isAudio) return <Music className="w-8 h-8 text-amber-400" />;
    if (isArchive) return <FileArchive className="w-8 h-8 text-purple-400" />;
    if (isDoc) return <FileText className="w-8 h-8 text-emerald-400" />;
    return <File className="w-8 h-8 text-blue-400" />;
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-md select-none animate-fadeIn">
      <div className="bg-[#101730] border border-[#23315D] rounded-3xl w-full max-w-lg p-6 shadow-2xl relative glow-cyan space-y-5">
        {/* Top Close Button */}
        <button
          onClick={onClose}
          className="w-8 h-8 rounded-full bg-[#1A2346] hover:bg-[#253262] text-slate-400 hover:text-white flex items-center justify-center absolute left-5 top-5 transition-colors cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        {/* Modal Header */}
        <div className="flex items-center gap-3">
          <div className="w-11 h-11 rounded-2xl bg-emerald-500/20 border border-emerald-500/40 flex items-center justify-center text-emerald-400 shadow-md">
            <CheckCircle2 className="w-6 h-6 stroke-[2.5]" />
          </div>
          <div>
            <h2 className="text-lg font-black text-white">اكتمل التنزيل بنجاح!</h2>
            <p className="text-xs text-emerald-400 font-semibold">جاهز للفتح والاستخدام عبر ZDM</p>
          </div>
        </div>

        {/* File Details Card */}
        <div className="p-4 rounded-2xl bg-[#0B1020] border border-[#202C55] flex items-center gap-4">
          <div className="w-14 h-14 rounded-2xl bg-[#141C38] border border-[#263566] flex items-center justify-center shrink-0 shadow-inner">
            {getIcon()}
          </div>

          <div className="flex-1 min-w-0">
            <h3
              className="text-sm font-bold text-white truncate mb-1"
              title={download.filename}
              dir="auto"
            >
              {download.filename}
            </h3>
            <div className="flex items-center gap-2 text-xs">
              <span className="font-mono text-cyan-400 font-bold">
                {download.total_size > 0 ? formatBytes(download.total_size) : 'مكتمل'}
              </span>
              <span className="text-slate-600">•</span>
              <span className="text-slate-400 text-[11px] truncate" title={download.save_path} dir="ltr">
                {download.save_path}
              </span>
            </div>
          </div>
        </div>

        {/* Actions (IDM style buttons) */}
        <div className="grid grid-cols-2 gap-3 pt-2">
          <button
            type="button"
            onClick={() => {
              onOpenFile(download.save_path);
              onClose();
            }}
            className="py-3 px-4 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 hover:opacity-95 text-slate-950 font-black text-xs flex items-center justify-center gap-2 glow-cyan transition-all active:scale-95 cursor-pointer shadow-lg"
          >
            {isVideo ? <Play className="w-4 h-4 fill-slate-950 stroke-none" /> : <ExternalLink className="w-4 h-4 stroke-[2.5]" />}
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

        {/* Footer with Copyright and Close */}
        <div className="flex items-center justify-between border-t border-[#1A2449] pt-3 text-[11px] text-slate-500">
          <span>Developed by Ramy Badraan</span>
          <button
            type="button"
            onClick={onClose}
            className="text-slate-400 hover:text-white transition-colors cursor-pointer"
          >
            إغلاق النافذة
          </button>
        </div>
      </div>
    </div>
  );
};
