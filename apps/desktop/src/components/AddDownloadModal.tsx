//! AddDownloadModal Component
//! Developed by Ramy Badraan

import React, { useState, useEffect } from 'react';
import { X, Link2, Download, Zap, CheckCircle, AlertCircle, Loader2 } from 'lucide-react';
import { NewDownloadRequest, UrlMetadata } from '../types';
import { formatBytes } from '../utils/format';

interface AddDownloadModalProps {
  isOpen: boolean;
  onClose: () => void;
  onProbeUrl: (url: string) => Promise<UrlMetadata>;
  onAddDownload: (req: NewDownloadRequest) => Promise<void>;
}

export const AddDownloadModal: React.FC<AddDownloadModalProps> = ({
  isOpen,
  onClose,
  onProbeUrl,
  onAddDownload,
}) => {
  const [url, setUrl] = useState('');
  const [filename, setFilename] = useState('');
  const [connections, setConnections] = useState<number>(16);
  const [category, setCategory] = useState('Others');
  const [totalSize, setTotalSize] = useState<number>(0);
  const [supportsRange, setSupportsRange] = useState<boolean>(true);
  const [isProbing, setIsProbing] = useState(false);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (isOpen) {
      // Try to read clipboard if it contains a URL
      navigator.clipboard?.readText().then((text) => {
        if (text && (text.startsWith('http://') || text.startsWith('https://'))) {
          setUrl(text);
          handleProbe(text);
        }
      }).catch(() => {});
    } else {
      setUrl('');
      setFilename('');
      setProbeError(null);
      setTotalSize(0);
    }
  }, [isOpen]);

  const handleProbe = async (targetUrl: string) => {
    if (!targetUrl.trim()) return;
    setIsProbing(true);
    setProbeError(null);

    try {
      const meta = await onProbeUrl(targetUrl.trim());
      setFilename(meta.filename);
      setTotalSize(meta.total_size);
      setSupportsRange(meta.supports_range);
      setCategory(meta.category);

      // Set optimal connections
      if (meta.supports_range && meta.total_size > 500 * 1024 * 1024) {
        setConnections(32);
      } else if (meta.supports_range) {
        setConnections(16);
      } else {
        setConnections(1);
      }
    } catch (err: any) {
      setProbeError(err.toString());
    } finally {
      setIsProbing(false);
    }
  };

  const handleSubmit = async (startNow: boolean) => {
    if (!url.trim()) return;
    setIsSubmitting(true);
    try {
      await onAddDownload({
        url: url.trim(),
        filename: filename.trim() || undefined,
        connections,
        category,
        start_now: startNow,
      });
      onClose();
    } catch (err: any) {
      setProbeError(err.toString());
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm select-none">
      <div className="bg-[#101730] border border-[#23315D] rounded-3xl w-full max-w-xl p-6 shadow-2xl relative glow-cyan">
        {/* Close Button */}
        <button
          onClick={onClose}
          className="w-8 h-8 rounded-full bg-[#1A2346] hover:bg-[#253262] text-slate-400 hover:text-white flex items-center justify-center absolute left-5 top-5 transition-colors cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        {/* Modal Header */}
        <div className="flex items-center gap-3 mb-6">
          <div className="w-10 h-10 rounded-2xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
            <Link2 className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-lg font-extrabold text-white">إضافة رابط تنزيل جديد</h2>
            <p className="text-xs text-slate-400">سيقوم محرك ZDM بتقسيم الملف ومضاعفة السرعة</p>
          </div>
        </div>

        {/* Form */}
        <div className="space-y-4">
          {/* URL Input */}
          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1.5 text-right">
              رابط الملف (URL)
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onBlur={() => handleProbe(url)}
                placeholder="https://example.com/file.zip"
                className="flex-1 h-11 px-4 rounded-xl bg-[#0B1020] border border-[#23315D] text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-cyan-400 focus:ring-1 focus:ring-cyan-400 text-left font-mono"
                dir="ltr"
              />
              <button
                type="button"
                onClick={() => handleProbe(url)}
                disabled={isProbing || !url}
                className="px-4 h-11 rounded-xl bg-[#1C264C] hover:bg-cyan-500/20 text-cyan-400 font-semibold text-xs border border-cyan-500/30 flex items-center gap-1.5 transition-colors disabled:opacity-50 cursor-pointer"
              >
                {isProbing ? <Loader2 className="w-4 h-4 animate-spin" /> : <Zap className="w-4 h-4" />}
                <span>فحص</span>
              </button>
            </div>
          </div>

          {/* Filename Input */}
          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1.5 text-right">
              اسم الملف
            </label>
            <input
              type="text"
              value={filename}
              onChange={(e) => setFilename(e.target.value)}
              placeholder="اسم الملف الذي سيتم حفظه..."
              className="w-full h-11 px-4 rounded-xl bg-[#0B1020] border border-[#23315D] text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-cyan-400 text-left"
              dir="ltr"
            />
          </div>

          {/* File Metadata Info Box */}
          {totalSize > 0 && (
            <div className="p-3.5 rounded-xl bg-[#0C1226] border border-[#1E2B54] flex items-center justify-between text-xs">
              <div className="flex items-center gap-2">
                <CheckCircle className="w-4 h-4 text-emerald-400" />
                <span className="text-slate-300 font-medium">حجم الملف:</span>
                <span className="text-cyan-400 font-mono font-bold">{formatBytes(totalSize)}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-400">دعم الاستئناف:</span>
                <span className={`font-semibold ${supportsRange ? 'text-emerald-400' : 'text-amber-400'}`}>
                  {supportsRange ? 'مدعوم (Range 206)' : 'اتصال مفرد فقط'}
                </span>
              </div>
            </div>
          )}

          {probeError && (
            <div className="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-400 text-xs flex items-center gap-2">
              <AlertCircle className="w-4 h-4 shrink-0" />
              <span>{probeError}</span>
            </div>
          )}

          {/* Parallel Connections Picker */}
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label className="text-xs font-semibold text-slate-300">
                عدد قنوات التحميل المتوازية (Parallel Connections)
              </label>
              <span className="text-xs font-mono font-bold text-cyan-400">{connections} قنوات</span>
            </div>
            <div className="grid grid-cols-5 gap-2">
              {[1, 4, 8, 16, 32].map((num) => (
                <button
                  key={num}
                  type="button"
                  onClick={() => setConnections(num)}
                  disabled={!supportsRange && num > 1}
                  className={`py-2 rounded-xl text-xs font-mono font-bold transition-all border ${
                    connections === num
                      ? 'bg-cyan-500 text-slate-950 border-cyan-400 shadow-md font-black'
                      : 'bg-[#0B1020] text-slate-300 border-[#23315D] hover:border-cyan-500/50'
                  } disabled:opacity-30 cursor-pointer`}
                >
                  {num} {num === 32 ? 'Turbo' : ''}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Modal Actions */}
        <div className="flex items-center justify-end gap-3 mt-8 pt-4 border-t border-[#1E294D]">
          <button
            type="button"
            onClick={onClose}
            className="px-5 h-11 rounded-xl text-slate-400 hover:text-slate-200 text-xs font-bold transition-colors cursor-pointer"
          >
            إلغاء
          </button>
          <button
            type="button"
            onClick={() => handleSubmit(false)}
            disabled={isSubmitting || !url}
            className="px-5 h-11 rounded-xl bg-[#1C264C] hover:bg-[#253262] text-slate-200 text-xs font-bold border border-[#2E3E72] transition-colors cursor-pointer"
          >
            إضافة للطابور
          </button>
          <button
            type="button"
            onClick={() => handleSubmit(true)}
            disabled={isSubmitting || !url}
            className="px-6 h-11 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 hover:opacity-95 text-slate-950 text-xs font-black flex items-center gap-2 glow-cyan transition-all active:scale-95 cursor-pointer"
          >
            {isSubmitting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Download className="w-4 h-4 stroke-[3]" />
            )}
            <span>بدء التحميل الآن</span>
          </button>
        </div>
      </div>
    </div>
  );
};
