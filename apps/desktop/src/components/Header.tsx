//! Header Component
//! Developed by Ramy Badraan

import React from 'react';
import { Plus, Search, ShieldCheck } from 'lucide-react';

interface HeaderProps {
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  onOpenAddModal: () => void;
  onOpenAboutModal: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  searchQuery,
  setSearchQuery,
  onOpenAddModal,
  onOpenAboutModal,
}) => {
  return (
    <header className="h-16 border-b border-[#1E294D] bg-[#0E1428]/95 backdrop-blur-md px-6 flex items-center justify-between z-20">
      {/* Title / App Identification */}
      <div className="flex items-center gap-3">
        <div className="flex flex-col">
          <div className="flex items-center gap-2">
            <h1 className="text-xl font-extrabold tracking-wide text-white flex items-center gap-2">
              <span className="text-gradient">ZDM</span>
              <span className="text-sm font-semibold text-slate-300 hidden sm:inline">مدير التنزيل فائق السرعة</span>
            </h1>
            <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-cyan-500/10 text-cyan-400 border border-cyan-500/30 font-mono">
              TURBO v1.0
            </span>
          </div>
          <button
            onClick={onOpenAboutModal}
            className="text-[11px] text-slate-400 hover:text-cyan-400 text-right transition-colors flex items-center gap-1 group"
          >
            <ShieldCheck className="w-3 h-3 text-cyan-400" />
            <span>Developed by <strong className="text-slate-200 group-hover:text-cyan-300 font-mono">Ramy Badraan</strong></span>
          </button>
        </div>
      </div>

      {/* Center Search Bar */}
      <div className="flex-1 max-w-md mx-6">
        <div className="relative">
          <Search className="w-4 h-4 absolute right-3.5 top-1/2 -translate-y-1/2 text-slate-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="بحث في أسماء الملفات أو الروابط..."
            className="w-full h-10 pr-10 pl-4 rounded-xl bg-[#141B34] border border-[#232F58] text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-cyan-400/80 focus:ring-1 focus:ring-cyan-400/40 transition-all text-right"
          />
        </div>
      </div>

      {/* Action Buttons */}
      <div className="flex items-center gap-3">
        <button
          onClick={onOpenAddModal}
          className="h-10 px-5 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 hover:opacity-95 active:scale-95 text-slate-950 font-bold text-sm flex items-center gap-2 transition-all glow-cyan shadow-lg cursor-pointer"
        >
          <Plus className="w-4 h-4 stroke-[3]" />
          <span>+ إضافة رابط جديد</span>
        </button>
      </div>
    </header>
  );
};
