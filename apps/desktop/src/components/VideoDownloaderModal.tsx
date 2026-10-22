//! VideoDownloaderModal Component (YouTube & Media Sniffer)
//! Developed by Ramy Badraan

import React, { useState } from 'react';
import { X, Film, Download, Music, AlertCircle, Loader2, Sparkles } from 'lucide-react';
import { VideoInfo, VideoFormat } from '../types';
import { formatBytes, formatTime } from '../utils/format';

interface VideoDownloaderModalProps {
  isOpen: boolean;
  onClose: () => void;
  onQueryVideo: (url: string) => Promise<VideoInfo>;
  onStartDownload: (url: string, filename: string, category: string) => Promise<void>;
}

export const VideoDownloaderModal: React.FC<VideoDownloaderModalProps> = ({
  isOpen,
  onClose,
  onQueryVideo,
  onStartDownload,
}) => {
  const [url, setUrl] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedFormat, setSelectedFormat] = useState<VideoFormat | null>(null);
  const [isStarting, setIsStarting] = useState(false);

  const handleFetch = async () => {
    if (!url.trim()) return;
    setIsLoading(true);
    setError(null);
    setVideoInfo(null);
    setSelectedFormat(null);

    try {
      const info = await onQueryVideo(url.trim());
      setVideoInfo(info);
      // Auto-select highest resolution video format with URL or first format
      const best = info.formats.find((f) => f.resolution && f.url) || info.formats[0];
      setSelectedFormat(best || null);
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setIsLoading(false);
    }
  };

  const handleDownload = async () => {
    if (!videoInfo || !selectedFormat) return;
    setIsStarting(true);
    try {
      const downloadUrl = selectedFormat.url || url;
      const cleanTitle = videoInfo.title.replace(/[\/\\?%*:|"<>]/g, '_');
      const filename = `${cleanTitle}.${selectedFormat.ext}`;
      const isAudio = !selectedFormat.vcodec || selectedFormat.vcodec === 'none';
      await onStartDownload(downloadUrl, filename, isAudio ? 'Audio' : 'Video');
      onClose();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setIsStarting(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-md select-none">
      <div className="bg-[#101730] border border-[#23315D] rounded-3xl w-full max-w-2xl p-6 shadow-2xl relative glow-cyan max-h-[90vh] overflow-y-auto">
        {/* Close Button */}
        <button
          onClick={onClose}
          className="w-8 h-8 rounded-full bg-[#1A2346] hover:bg-[#253262] text-slate-400 hover:text-white flex items-center justify-center absolute left-5 top-5 transition-colors cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        {/* Modal Header */}
        <div className="flex items-center gap-3 mb-5">
          <div className="w-10 h-10 rounded-2xl bg-rose-500/15 border border-rose-500/30 flex items-center justify-center text-rose-400">
            <Film className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-lg font-extrabold text-white">تنزيل الفيديو واليوتيوب (yt-dlp Engine)</h2>
            <p className="text-xs text-slate-400">التقاط الجودات وتحميل المقاطع بالسرعة القصوى الموازية</p>
          </div>
        </div>

        {/* URL Input Form */}
        <div className="flex gap-2 mb-4">
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="الصق رابط فيديو من YouTube أو الفيسبوك أو أي موقع..."
            className="flex-1 h-11 px-4 rounded-xl bg-[#0B1020] border border-[#23315D] text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-rose-400 text-left font-mono"
            dir="ltr"
          />
          <button
            onClick={handleFetch}
            disabled={isLoading || !url}
            className="px-5 h-11 rounded-xl bg-gradient-to-r from-rose-500 to-purple-600 hover:opacity-95 text-white font-bold text-xs flex items-center gap-1.5 transition-all disabled:opacity-50 cursor-pointer"
          >
            {isLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Sparkles className="w-4 h-4" />}
            <span>استخراج</span>
          </button>
        </div>

        {error && (
          <div className="p-3 mb-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-400 text-xs flex items-center gap-2">
            <AlertCircle className="w-4 h-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        {/* Video Card Details */}
        {videoInfo && (
          <div className="space-y-4 pt-2">
            <div className="flex gap-4 p-4 rounded-2xl bg-[#0B1020] border border-[#202C55]">
              {videoInfo.thumbnail && (
                <img
                  src={videoInfo.thumbnail}
                  alt={videoInfo.title}
                  className="w-36 h-24 object-cover rounded-xl border border-[#202C55]"
                />
              )}
              <div className="flex-1 min-w-0">
                <h3 className="text-sm font-bold text-white line-clamp-2 mb-1" title={videoInfo.title}>
                  {videoInfo.title}
                </h3>
                <div className="flex items-center gap-3 text-xs text-slate-400">
                  {videoInfo.uploader && <span>القناة: {videoInfo.uploader}</span>}
                  {videoInfo.duration_seconds && <span>المدة: {formatTime(videoInfo.duration_seconds)}</span>}
                </div>
              </div>
            </div>

            {/* Quality Options Grid */}
            <div>
              <label className="block text-xs font-semibold text-slate-300 mb-2 text-right">
                اختر الجودة أو الصيغة المطلوبة:
              </label>
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-2 max-h-48 overflow-y-auto p-1">
                {videoInfo.formats
                  .filter((f) => f.resolution || f.ext === 'm4a' || f.ext === 'mp3')
                  .slice(0, 15)
                  .map((fmt, idx) => {
                    const isSelected = selectedFormat?.format_id === fmt.format_id;
                    const isAudio = !fmt.vcodec || fmt.vcodec === 'none';

                    return (
                      <button
                        key={`${fmt.format_id}-${idx}`}
                        type="button"
                        onClick={() => setSelectedFormat(fmt)}
                        className={`p-2.5 rounded-xl text-xs text-right border transition-all flex items-center justify-between cursor-pointer ${
                          isSelected
                            ? 'bg-rose-500/20 border-rose-500 text-white font-bold shadow-md'
                            : 'bg-[#0E1428] border-[#1E294D] text-slate-300 hover:border-slate-500'
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          {isAudio ? (
                            <Music className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                          ) : (
                            <Film className="w-3.5 h-3.5 text-rose-400 shrink-0" />
                          )}
                          <span className="font-mono">
                            {fmt.resolution || 'صوت فقط'} ({fmt.ext})
                          </span>
                        </div>
                        {fmt.filesize && (
                          <span className="text-[10px] text-slate-400 font-mono">
                            {formatBytes(fmt.filesize)}
                          </span>
                        )}
                      </button>
                    );
                  })}
              </div>
            </div>

            {/* Action Download */}
            <div className="pt-3 border-t border-[#1E294D] flex justify-end gap-3">
              <button
                onClick={onClose}
                className="px-4 h-10 rounded-xl text-slate-400 hover:text-white text-xs font-bold transition-colors cursor-pointer"
              >
                إلغاء
              </button>
              <button
                onClick={handleDownload}
                disabled={isStarting || !selectedFormat}
                className="px-6 h-10 rounded-xl bg-gradient-to-r from-rose-500 to-purple-600 hover:opacity-95 text-white font-bold text-xs flex items-center gap-2 glow-magenta transition-all active:scale-95 cursor-pointer"
              >
                {isStarting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Download className="w-4 h-4 stroke-[3]" />}
                <span>تحميل الفيديو بمحرك ZDM</span>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
