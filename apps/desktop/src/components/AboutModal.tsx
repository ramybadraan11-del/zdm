//! About & Copyright Modal Component
//! Developed by Ramy Badraan

import React from 'react';
import { X, ShieldCheck, Cpu, Award, Zap } from 'lucide-react';

interface AboutModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const AboutModal: React.FC<AboutModalProps> = ({ isOpen, onClose }) => {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-md select-none">
      <div className="bg-[#0F152C] border border-[#23315D] rounded-3xl w-full max-w-lg p-6 shadow-2xl relative glow-cyan overflow-hidden">
        {/* Glow backdrop accent */}
        <div className="absolute -right-20 -top-20 w-48 h-48 bg-cyan-500/15 rounded-full blur-3xl pointer-events-none" />
        <div className="absolute -left-20 -bottom-20 w-48 h-48 bg-purple-500/15 rounded-full blur-3xl pointer-events-none" />

        {/* Close Button */}
        <button
          onClick={onClose}
          className="w-8 h-8 rounded-full bg-[#1A2346] hover:bg-[#253262] text-slate-400 hover:text-white flex items-center justify-center absolute left-5 top-5 transition-colors cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        {/* Header with App Logo and Badges */}
        <div className="text-center pt-2 pb-5 border-b border-[#1E294D]">
          <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-cyan-400 via-purple-600 to-pink-500 p-0.5 mx-auto mb-3.5 glow-cyan shadow-xl flex items-center justify-center">
            <div className="w-full h-full bg-[#0B1020] rounded-[14px] flex items-center justify-center font-black text-2xl text-gradient font-mono">
              Z
            </div>
          </div>
          <h2 className="text-xl font-black text-white tracking-wide">
            Zyzoom Download Manager (ZDM)
          </h2>
          <p className="text-xs text-cyan-400 font-mono font-semibold mt-0.5">
            Version 1.0.0 Pro • Turbo 32-Stream Architecture
          </p>
        </div>

        {/* Developer Attribution Box (Required) */}
        <div className="my-5 p-4 rounded-2xl bg-gradient-to-r from-cyan-950/40 via-purple-950/30 to-pink-950/30 border border-cyan-500/40 text-center relative">
          <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-cyan-500/15 border border-cyan-500/30 text-cyan-300 text-xs font-bold mb-2">
            <ShieldCheck className="w-3.5 h-3.5 text-cyan-400" />
            <span>حقوق التطوير والملكية الفكرية</span>
          </div>
          <div className="text-lg font-black text-white tracking-wider font-mono">
            Developed by <span className="text-gradient">Ramy Badraan</span>
          </div>
          <p className="text-xs text-slate-400 mt-1">
            جميع الحقوق محفوظة للمطور رامي بدران © 2026
          </p>
        </div>

        {/* Technical Highlights */}
        <div className="space-y-2.5 text-xs">
          <div className="p-2.5 rounded-xl bg-[#141C38] border border-[#202C54] flex items-center gap-3">
            <Zap className="w-4 h-4 text-cyan-400 shrink-0" />
            <div className="text-right">
              <span className="font-bold text-slate-200">محرك Direct-Write فائق السرعة: </span>
              <span className="text-slate-400">كتابة فورية في موضع البايت المحدد دون استهلاك وقت في الدمج عند 99%.</span>
            </div>
          </div>

          <div className="p-2.5 rounded-xl bg-[#141C38] border border-[#202C54] flex items-center gap-3">
            <Cpu className="w-4 h-4 text-purple-400 shrink-0" />
            <div className="text-right">
              <span className="font-bold text-slate-200">نواة Rust غير متزامنة (Tokio): </span>
              <span className="text-slate-400">استهلاك ذاكرة RAM أقل من 100MB وأداء شبكي واستقرار أقصى.</span>
            </div>
          </div>

          <div className="p-2.5 rounded-xl bg-[#141C38] border border-[#202C54] flex items-center gap-3">
            <Award className="w-4 h-4 text-emerald-400 shrink-0" />
            <div className="text-right">
              <span className="font-bold text-slate-200">خوارزمية سرقة المهام (Work Stealing): </span>
              <span className="text-slate-400">استغلال سرعة الإنترنت حتى آخر بايت دون انتظار الجزء الأبطأ.</span>
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="mt-6 pt-4 border-t border-[#1E294D] flex items-center justify-between text-[11px] text-slate-400">
          <span>Linux (Arch / Fedora / Debian) &amp; Windows</span>
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-cyan-500/15 hover:bg-cyan-500/25 text-cyan-300 font-bold transition-colors cursor-pointer"
          >
            إغلاق
          </button>
        </div>
      </div>
    </div>
  );
};
