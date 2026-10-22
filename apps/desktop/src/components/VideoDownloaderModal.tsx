//! VideoDownloaderModal Component (YouTube & Media Sniffer)
//! Developed by Ramy Badraan

import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X, Film, Download, Music, AlertCircle, Loader2, Sparkles } from 'lucide-react';
import { VideoInfo, VideoFormat } from '../types';
import { formatBytes, formatTime } from '../utils/format';
import { FileConflictModal, FileConflictData } from './FileConflictModal';

interface VideoDownloaderModalProps {
  isOpen: boolean;
  onClose: () => void;
  onQueryVideo: (url: string) => Promise<VideoInfo>;
  initialUrl?: string;
}

export const VideoDownloaderModal: React.FC<VideoDownloaderModalProps> = ({
  isOpen,
  onClose,
  onQueryVideo,
  initialUrl,
}) => {
  const [url, setUrl] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedFormat, setSelectedFormat] = useState<VideoFormat | null>(null);
  const [isStarting, setIsStarting] = useState(false);

  const fetchWithUrl = async (targetUrl: string) => {
    if (!targetUrl.trim()) return;
    setIsLoading(true);
    setError(null);
    setVideoInfo(null);
    setSelectedFormat(null);

    try {
      const info = await onQueryVideo(targetUrl.trim());
      setVideoInfo(info);
      const best = info.formats.find((f) => f.format_id !== 'bestaudio_mp3' && f.ext !== 'mp3') || info.formats[0];
      setSelectedFormat(best || null);
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen && initialUrl && initialUrl.trim()) {
      setUrl(initialUrl);
      fetchWithUrl(initialUrl.trim());
    } else if (!isOpen) {
      setError(null);
      setVideoInfo(null);
      setSelectedFormat(null);
    }
  }, [isOpen, initialUrl]);

  const handleFetch = async () => {
    await fetchWithUrl(url);
  };

  const [conflictData, setConflictData] = useState<FileConflictData | null>(null);
  const [isConflictModalOpen, setIsConflictModalOpen] = useState(false);

  const startDownloadWithParams = async (customFilename?: string, forceOverwrite?: boolean) => {
    if (!videoInfo || !selectedFormat) return;
    setIsStarting(true);
    try {
      const isAudio = selectedFormat.format_id === 'bestaudio_mp3' || selectedFormat.ext === 'mp3';

      await invoke('start_video_download', {
        url,
        formatId: selectedFormat.format_id,
        title: videoInfo.title,
        ext: selectedFormat.ext,
        isAudio,
        estSize: selectedFormat.filesize || 0,
        customFilename: customFilename || null,
        forceOverwrite: forceOverwrite || false,
      });

      setIsConflictModalOpen(false);
      onClose();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setIsStarting(false);
    }
  };

  const handleDownload = async () => {
    if (!videoInfo || !selectedFormat) return;
    const isAudio = selectedFormat.format_id === 'bestaudio_mp3' || selectedFormat.ext === 'mp3';
    const cat = isAudio ? 'Audio' : 'Video';
    const cleanTitle = videoInfo.title.replace(/[\/\\?%*:|"<>]/g, '_');
    const targetFilename = `${cleanTitle.trim()}.${selectedFormat.ext}`;

    try {
      const conflict: any = await invoke('check_file_conflict', {
        filename: targetFilename,
        category: cat,
      });

      if (conflict && conflict.exists) {
        setConflictData({
          filename: targetFilename,
          category: cat,
          existingPath: conflict.existing_path,
          existingSize: conflict.existing_size,
          suggestedName: conflict.suggested_name,
        });
        setIsConflictModalOpen(true);
        return;
      }
    } catch (e) {
      console.warn('Conflict check failed, proceeding normally:', e);
    }

    await startDownloadWithParams();
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

            {/* Video Qualities (MP4) */}
            {(() => {
              const videoFormats = videoInfo.formats.filter((f) => f.format_id !== 'bestaudio_mp3' && f.ext !== 'mp3');
              const audioFormat = videoInfo.formats.find((f) => f.format_id === 'bestaudio_mp3' || f.ext === 'mp3');

              return (
                <div className="space-y-4">
                  {/* Video Section */}
                  {videoFormats.length > 0 && (
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <label className="text-xs font-bold text-slate-200 flex items-center gap-2">
                          <Film className="w-4 h-4 text-cyan-400" />
                          <span>جودة الفيديو (صيغة MP4 مع الصوت):</span>
                        </label>
                        <span className="text-[11px] text-cyan-400/80 font-mono">دمج تلقائي للصوت والصورة</span>
                      </div>

                      <div className="grid grid-cols-2 sm:grid-cols-3 gap-2.5 max-h-48 overflow-y-auto p-1">
                        {videoFormats.map((fmt) => {
                          const isSelected = selectedFormat?.format_id === fmt.format_id;
                          return (
                            <button
                              key={fmt.format_id}
                              type="button"
                              onClick={() => setSelectedFormat(fmt)}
                              className={`p-3 rounded-xl text-right border transition-all flex flex-col justify-between gap-1.5 cursor-pointer ${
                                isSelected
                                  ? 'bg-cyan-500/20 border-cyan-400 text-white shadow-lg glow-cyan'
                                  : 'bg-[#0E1428] border-[#1E294D] text-slate-300 hover:border-cyan-500/50 hover:bg-[#131B36]'
                              }`}
                            >
                              <div className="flex items-center justify-between w-full">
                                <span className="font-extrabold text-xs">{fmt.resolution || 'MP4 Video'}</span>
                                <span className="text-[10px] px-1.5 py-0.5 rounded bg-cyan-950 text-cyan-300 border border-cyan-800 font-mono">
                                  MP4
                                </span>
                              </div>
                              <div className="flex items-center justify-between w-full text-[11px] text-slate-400">
                                <span>الحجم المقدر:</span>
                                <span className="font-mono text-slate-200">
                                  {fmt.filesize ? formatBytes(fmt.filesize) : 'حسب الخادم'}
                                </span>
                              </div>
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  )}

                  {/* Audio Section (Exactly ONE clear button) */}
                  {audioFormat && (
                    <div>
                      <label className="block text-xs font-bold text-slate-200 mb-2 flex items-center gap-2">
                        <Music className="w-4 h-4 text-amber-400" />
                        <span>أو تنزيل الصوت فقط:</span>
                      </label>

                      <button
                        type="button"
                        onClick={() => setSelectedFormat(audioFormat)}
                        className={`w-full p-3.5 rounded-2xl border transition-all flex items-center justify-between cursor-pointer ${
                          selectedFormat?.format_id === audioFormat.format_id
                            ? 'bg-amber-500/20 border-amber-400 text-white shadow-lg'
                            : 'bg-[#0E1428] border-[#1E294D] text-slate-300 hover:border-amber-500/50 hover:bg-[#151B33]'
                        }`}
                      >
                        <div className="flex items-center gap-3">
                          <div className="w-9 h-9 rounded-xl bg-amber-500/20 border border-amber-500/40 flex items-center justify-center text-amber-400">
                            <Music className="w-5 h-5" />
                          </div>
                          <div className="text-right">
                            <div className="text-xs font-bold text-white">تنزيل الصوت فقط (MP3 - أعلى جودة)</div>
                            <div className="text-[11px] text-slate-400">استخراج المسار الصوتي بأعلى نقاء دون تحميل الفيديو</div>
                          </div>
                        </div>

                        <div className="text-left">
                          <span className="text-[10px] px-2 py-0.5 rounded bg-amber-950 text-amber-300 border border-amber-800 font-mono font-bold block mb-1">
                            MP3
                          </span>
                          <span className="text-xs text-slate-300 font-mono font-semibold">
                            {audioFormat.filesize ? formatBytes(audioFormat.filesize) : ''}
                          </span>
                        </div>
                      </button>
                    </div>
                  )}
                </div>
              );
            })()}

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
                className="px-6 h-10 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 hover:opacity-95 text-slate-950 font-extrabold text-xs flex items-center gap-2 glow-cyan transition-all active:scale-95 disabled:opacity-50 cursor-pointer"
              >
                {isStarting ? (
                  <Loader2 className="w-4 h-4 animate-spin text-slate-950" />
                ) : (
                  <Download className="w-4 h-4 stroke-[3] text-slate-950" />
                )}
                <span>
                  {isStarting
                    ? 'جارٍ التحميل والتجميع...'
                    : selectedFormat?.format_id === 'bestaudio_mp3'
                    ? 'تحميل الصوت فقط (MP3)'
                    : `تحميل الفيديو (${selectedFormat?.resolution || 'MP4'})`}
                </span>
              </button>
            </div>
          </div>
        )}
      </div>

      <FileConflictModal
        isOpen={isConflictModalOpen}
        onClose={() => setIsConflictModalOpen(false)}
        conflict={conflictData}
        onDownloadNewName={(newName) => startDownloadWithParams(newName, false)}
        onOverwrite={() => startDownloadWithParams(conflictData?.filename, true)}
        onOpenExisting={async (path) => {
          setIsConflictModalOpen(false);
          onClose();
          await invoke('open_file', { path });
        }}
      />
    </div>
  );
};
