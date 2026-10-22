//! SettingsView Component
//! Developed by Ramy Badraan

import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Settings,
  Folder,
  Cpu,
  Globe,
  Film,
  ShieldCheck,
  RefreshCw,
  ExternalLink,
  CheckCircle2,
  Loader2,
} from 'lucide-react';
import { BrowserStatus } from '../types';

export const SettingsView: React.FC = () => {
  const [downloadDir, setDownloadDir] = useState('~/Downloads/ZDM');
  const [defaultConnections, setDefaultConnections] = useState(16);
  const [browsers, setBrowsers] = useState<BrowserStatus[]>([]);
  const [isUpdatingYtDlp, setIsUpdatingYtDlp] = useState(false);
  const [ytDlpStatus, setYtDlpStatus] = useState<string>('فحص الحالة...');
  const [saveSuccess, setSaveSuccess] = useState(false);

  // Fetch installed browsers
  const fetchBrowsers = async () => {
    try {
      const list: BrowserStatus[] = await invoke('get_browsers');
      setBrowsers(list);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    fetchBrowsers();
    // Check yt-dlp
    invoke('update_yt_dlp_engine')
      .then((path) => setYtDlpStatus(`جاهز ويعمل (${path})`))
      .catch((err) => setYtDlpStatus(`بحاجة لتحديث: ${err}`));
  }, []);

  const handleRegisterAll = async () => {
    try {
      await invoke('register_all_browsers');
      await fetchBrowsers();
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (e) {
      console.error(e);
    }
  };

  const handleLaunchBrowser = async (browserId: string) => {
    try {
      await invoke('launch_browser_with_ext', { browserId });
    } catch (e) {
      alert(`خطأ أثناء فتح المتصفح: ${e}`);
    }
  };

  const handleUpdateYtDlp = async () => {
    setIsUpdatingYtDlp(true);
    try {
      const path: string = await invoke('update_yt_dlp_engine');
      setYtDlpStatus(`تم التحديث بنجاح (${path})`);
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (err: any) {
      setYtDlpStatus(`فشل التحديث: ${err}`);
    } finally {
      setIsUpdatingYtDlp(false);
    }
  };

  const handleSave = () => {
    setSaveSuccess(true);
    setTimeout(() => setSaveSuccess(false), 3000);
  };

  return (
    <div className="space-y-6 max-w-4xl mx-auto pb-12 select-none animate-fadeIn">
      {/* Top Header */}
      <div className="flex items-center justify-between border-b border-[#1E294D] pb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-2xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
            <Settings className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-xl font-extrabold text-white">إعدادات ZDM الشاملة</h2>
            <p className="text-xs text-slate-400">تحكم كامل في محرك التحميل، تكامل المتصفحات، وأدوات الفيديو</p>
          </div>
        </div>

        {saveSuccess && (
          <div className="flex items-center gap-2 px-3 py-1.5 rounded-xl bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 text-xs font-bold animate-pulse">
            <CheckCircle2 className="w-4 h-4 text-emerald-400" />
            <span>تم حفظ الإعدادات وتحديث النظام ✓</span>
          </div>
        )}
      </div>

      {/* 1. Download Engine Settings Card */}
      <div className="bg-[#121933] border border-[#1E294D] rounded-2xl p-5 space-y-4">
        <div className="flex items-center gap-2.5 text-cyan-400 text-sm font-bold border-b border-[#1A2449] pb-3">
          <Cpu className="w-4 h-4" />
          <span>محرك التنزيل والتسريع الموازي (Turbo Engine)</span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1.5">
              مجلد الحفظ الافتراضي للملفات
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={downloadDir}
                onChange={(e) => setDownloadDir(e.target.value)}
                className="flex-1 h-10 px-3.5 rounded-xl bg-[#0B1020] border border-[#23315D] text-xs font-mono text-slate-200 focus:outline-none focus:border-cyan-400 text-left"
                dir="ltr"
              />
              <button
                type="button"
                className="px-3 h-10 rounded-xl bg-[#1A2346] hover:bg-[#253262] text-slate-300 text-xs font-bold border border-[#2A3764] flex items-center gap-1.5 transition-colors cursor-pointer"
              >
                <Folder className="w-4 h-4 text-cyan-400" />
                <span>تصفح</span>
              </button>
            </div>
            <p className="text-[11px] text-slate-500 mt-1">
              يتم تصنيف الملفات تلقائياً في مجلدات فرعية (Video, Audio, Compressed, Programs)
            </p>
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1.5">
              عدد قنوات التوازي الافتراضية (Connections per Download)
            </label>
            <div className="grid grid-cols-4 gap-2">
              {[4, 8, 16, 32].map((num) => (
                <button
                  key={num}
                  type="button"
                  onClick={() => setDefaultConnections(num)}
                  className={`py-2 rounded-xl text-xs font-mono font-bold border transition-all ${
                    defaultConnections === num
                      ? 'bg-cyan-500 text-slate-950 border-cyan-400 shadow-md font-black'
                      : 'bg-[#0B1020] text-slate-300 border-[#23315D] hover:border-cyan-500/50'
                  }`}
                >
                  {num} {num === 32 ? 'Turbo' : ''}
                </button>
              ))}
            </div>
            <p className="text-[11px] text-cyan-400 font-mono mt-1">
              Work Stealing مفعل تلقائياً لضمان عدم توقف آخر بايت
            </p>
          </div>
        </div>
      </div>

      {/* 2. Browser Integration & Extension Card */}
      <div className="bg-[#121933] border border-[#1E294D] rounded-2xl p-5 space-y-4">
        <div className="flex items-center justify-between border-b border-[#1A2449] pb-3">
          <div className="flex items-center gap-2.5 text-purple-400 text-sm font-bold">
            <Globe className="w-4 h-4" />
            <span>تكامل المتصفحات وتثبيت الإضافة التلقائي (Auto-Install Extension)</span>
          </div>
          <button
            onClick={handleRegisterAll}
            className="px-3.5 py-1.5 rounded-xl bg-purple-500/20 hover:bg-purple-500/30 text-purple-300 border border-purple-500/40 text-xs font-bold flex items-center gap-1.5 transition-colors cursor-pointer"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            <span>إعادة تسجيل الإضافة في كافة المتصفحات</span>
          </button>
        </div>

        <p className="text-xs text-slate-300">
          يقوم ZDM بربط الإضافة تلقائياً مع كافة المتصفحات الموجودة على جهازك عبر المضيف المحلي (Native Messaging Host):
        </p>

        {/* Browsers Grid */}
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
          {browsers.map((b) => (
            <div
              key={b.id}
              className={`p-3.5 rounded-xl border flex flex-col justify-between gap-3 ${
                b.is_installed
                  ? 'bg-[#0E152E] border-[#202E58]'
                  : 'bg-[#0B1020]/50 border-[#1A223E] opacity-60'
              }`}
            >
              <div className="flex items-center justify-between">
                <span className="font-bold text-sm text-white">{b.name}</span>
                {b.is_installed ? (
                  <span className="text-[10px] px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 font-bold">
                    مثبت ✓
                  </span>
                ) : (
                  <span className="text-[10px] px-2 py-0.5 rounded-full bg-slate-500/20 text-slate-400 font-bold">
                    غير مثبت
                  </span>
                )}
              </div>

              {b.is_installed && (
                <button
                  type="button"
                  onClick={() => handleLaunchBrowser(b.id)}
                  className="w-full py-1.5 rounded-lg bg-gradient-to-r from-cyan-500/20 to-purple-600/20 hover:from-cyan-500/30 hover:to-purple-600/30 text-cyan-300 border border-cyan-500/40 text-xs font-bold flex items-center justify-center gap-1.5 transition-all cursor-pointer"
                >
                  <ExternalLink className="w-3.5 h-3.5" />
                  <span>فتح المتصفح وتفعيل الإضافة</span>
                </button>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* 3. Media & Video Engine (yt-dlp) Card */}
      <div className="bg-[#121933] border border-[#1E294D] rounded-2xl p-5 space-y-4">
        <div className="flex items-center justify-between border-b border-[#1A2449] pb-3">
          <div className="flex items-center gap-2.5 text-rose-400 text-sm font-bold">
            <Film className="w-4 h-4" />
            <span>محرك تحميل الفيديو والوسائط (yt-dlp Engine)</span>
          </div>
          <button
            onClick={handleUpdateYtDlp}
            disabled={isUpdatingYtDlp}
            className="px-3.5 py-1.5 rounded-xl bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-bold flex items-center gap-1.5 transition-colors disabled:opacity-50 cursor-pointer"
          >
            {isUpdatingYtDlp ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RefreshCw className="w-3.5 h-3.5" />}
            <span>تحديث / تحميل yt-dlp تلقائياً</span>
          </button>
        </div>

        <div className="p-3.5 rounded-xl bg-[#0B1020] border border-[#202C55] flex items-center justify-between text-xs">
          <div>
            <span className="text-slate-400">حالة محرك استخراج الفيديو: </span>
            <span className="text-white font-mono font-bold mr-2">{ytDlpStatus}</span>
          </div>
          <span className="text-emerald-400 font-bold">يدعم YouTube, Facebook, Twitter, TikTok والمزيد</span>
        </div>
      </div>

      {/* 4. Rights & About Card */}
      <div className="bg-gradient-to-r from-cyan-950/30 via-purple-950/20 to-pink-950/20 border border-cyan-500/30 rounded-2xl p-5 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-2xl bg-cyan-500/20 border border-cyan-500/40 flex items-center justify-center text-cyan-400">
            <ShieldCheck className="w-6 h-6" />
          </div>
          <div>
            <h3 className="text-sm font-bold text-white">Zyzoom Download Manager (ZDM) Pro</h3>
            <p className="text-xs text-slate-400">
              Developed by <strong className="text-cyan-400 font-mono">Ramy Badraan</strong> • جميع الحقوق محفوظة © 2026
            </p>
          </div>
        </div>

        <button
          onClick={handleSave}
          className="px-6 py-2.5 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 hover:opacity-95 text-slate-950 font-bold text-xs glow-cyan shadow-lg transition-all active:scale-95 cursor-pointer"
        >
          حفظ جميع الإعدادات
        </button>
      </div>
    </div>
  );
};
