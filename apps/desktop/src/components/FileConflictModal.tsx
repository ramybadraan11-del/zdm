//! FileConflictModal Component (IDM Style File Conflict Dialog)
//! Developed by Ramy Badraan

import React from 'react';
import { AlertTriangle, Copy, Play, RefreshCw, X } from 'lucide-react';
import { formatBytes } from '../utils/format';

export interface FileConflictData {
  filename: string;
  category: string;
  existingPath: string;
  existingSize: number;
  suggestedName: string;
}

interface FileConflictModalProps {
  isOpen: boolean;
  onClose: () => void;
  conflict: FileConflictData | null;
  onDownloadNewName: (newName: string) => void;
  onOverwrite: () => void;
  onOpenExisting: (path: string) => void;
}

export const FileConflictModal: React.FC<FileConflictModalProps> = ({
  isOpen,
  onClose,
  conflict,
  onDownloadNewName,
  onOverwrite,
  onOpenExisting,
}) => {
  if (!isOpen || !conflict) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/85 backdrop-blur-md select-none animate-fadeIn">
      <div className="bg-[#101730] border border-amber-500/40 rounded-3xl w-full max-w-md p-6 shadow-2xl relative glow-amber space-y-4">
        {/* Close Button */}
        <button
          onClick={onClose}
          className="w-8 h-8 rounded-full bg-[#1A2346] hover:bg-[#253262] text-slate-400 hover:text-white flex items-center justify-center absolute left-5 top-5 transition-colors cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        {/* Modal Header */}
        <div className="flex items-center gap-3">
          <div className="w-11 h-11 rounded-2xl bg-amber-500/20 border border-amber-500/40 flex items-center justify-center text-amber-400 shrink-0">
            <AlertTriangle className="w-6 h-6 stroke-[2.5]" />
          </div>
          <div>
            <h2 className="text-base font-black text-white">الملف موجود بالفعل!</h2>
            <p className="text-xs text-amber-300 font-semibold">يوجد ملف بنفس الاسم تم تنزيله مسبقاً</p>
          </div>
        </div>

        {/* File Details Box */}
        <div className="p-3.5 rounded-2xl bg-[#0B1020] border border-[#202C55] space-y-1.5 text-xs">
          <div className="flex items-center justify-between text-slate-300">
            <span className="font-bold text-white truncate max-w-[280px]" dir="ltr">
              {conflict.filename}
            </span>
            <span className="font-mono text-cyan-400 font-bold shrink-0">
              {formatBytes(conflict.existingSize)}
            </span>
          </div>
          <div className="text-[11px] text-slate-400 truncate font-mono text-left" dir="ltr">
            {conflict.existingPath}
          </div>
        </div>

        <p className="text-xs text-slate-300 leading-relaxed text-right">
          ما الإجراء الذي ترغب في اتخاذه لهذا التحميل؟
        </p>

        {/* Action Options */}
        <div className="space-y-2 pt-1">
          {/* Option 1: Auto-Rename (Recommended) */}
          <button
            type="button"
            onClick={() => onDownloadNewName(conflict.suggestedName)}
            className="w-full p-3 rounded-xl bg-gradient-to-r from-cyan-500/20 to-purple-600/20 hover:from-cyan-500/30 hover:to-purple-600/30 border border-cyan-500/40 text-cyan-300 text-xs font-bold flex items-center justify-between transition-all cursor-pointer group"
          >
            <div className="flex items-center gap-2">
              <Copy className="w-4 h-4 text-cyan-400" />
              <span>تنزيل كنسخة جديدة (ترقيم تلقائي)</span>
            </div>
            <span className="font-mono text-[11px] text-cyan-400 bg-cyan-950/80 px-2 py-0.5 rounded border border-cyan-800">
              {conflict.suggestedName}
            </span>
          </button>

          {/* Option 2: Overwrite existing */}
          <button
            type="button"
            onClick={onOverwrite}
            className="w-full p-3 rounded-xl bg-[#141C38] hover:bg-rose-500/20 text-slate-300 hover:text-rose-300 border border-[#23315D] hover:border-rose-500/40 text-xs font-bold flex items-center gap-2 transition-all cursor-pointer"
          >
            <RefreshCw className="w-4 h-4 text-rose-400" />
            <span>استبدال الملف الموجود وتنزيل جديد</span>
          </button>

          {/* Option 3: Open existing */}
          <button
            type="button"
            onClick={() => onOpenExisting(conflict.existingPath)}
            className="w-full p-3 rounded-xl bg-[#141C38] hover:bg-[#1E2A54] text-slate-300 border border-[#23315D] text-xs font-bold flex items-center gap-2 transition-all cursor-pointer"
          >
            <Play className="w-4 h-4 text-emerald-400" />
            <span>فتح الملف القديم المحفوظ حالياً</span>
          </button>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-[#1A2449] pt-3 text-[11px] text-slate-500">
          <span>Developed by Ramy Badraan</span>
          <button
            type="button"
            onClick={onClose}
            className="text-slate-400 hover:text-white transition-colors cursor-pointer"
          >
            إلغاء الأمر
          </button>
        </div>
      </div>
    </div>
  );
};
