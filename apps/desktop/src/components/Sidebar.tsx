//! Sidebar Component
//! Developed by Ramy Badraan

import React from 'react';
import {
  Download,
  PlayCircle,
  CheckCircle2,
  Film,
  Magnet,
  Settings,
  Info,
  HardDrive,
  Sparkles,
} from 'lucide-react';

export type NavTab = 'all' | 'downloading' | 'completed' | 'video' | 'torrents' | 'settings';

interface SidebarProps {
  currentTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  activeCount: number;
  completedCount: number;
  onOpenAboutModal: () => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentTab,
  onSelectTab,
  activeCount,
  completedCount,
  onOpenAboutModal,
}) => {
  const navItems = [
    { id: 'all' as NavTab, label: 'كل التنزيلات', icon: Download },
    {
      id: 'downloading' as NavTab,
      label: 'قيد التنزيل',
      icon: PlayCircle,
      badge: activeCount > 0 ? activeCount : undefined,
      badgeColor: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40',
    },
    {
      id: 'completed' as NavTab,
      label: 'المكتملة',
      icon: CheckCircle2,
      badge: completedCount > 0 ? completedCount : undefined,
      badgeColor: 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40',
    },
    { id: 'video' as NavTab, label: 'تنزيل الفيديو (YouTube)', icon: Film },
    { id: 'torrents' as NavTab, label: 'ملفات التورنت', icon: Magnet },
    { id: 'settings' as NavTab, label: 'الإعدادات', icon: Settings },
  ];

  return (
    <aside className="w-64 border-l border-[#1E294D] bg-[#0A0E1D] flex flex-col justify-between p-4 z-10 select-none">
      {/* Top Branding */}
      <div>
        <div className="flex items-center gap-3 px-3 py-4 mb-4 border-b border-[#1A2342]">
          <div className="w-11 h-11 rounded-2xl bg-gradient-to-br from-cyan-400 via-purple-600 to-pink-500 p-0.5 glow-cyan shadow-md flex items-center justify-center">
            <div className="w-full h-full bg-[#0B1020] rounded-[14px] flex items-center justify-center font-black text-xl text-gradient font-mono">
              Z
            </div>
          </div>
          <div>
            <h2 className="font-extrabold text-lg text-white leading-tight tracking-wider font-mono">ZDM</h2>
            <p className="text-[11px] text-slate-400 font-medium">Zyzoom Manager</p>
          </div>
        </div>

        {/* Navigation Links */}
        <nav className="space-y-1.5">
          {navItems.map((item) => {
            const Icon = item.icon;
            const isActive = currentTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => onSelectTab(item.id)}
                className={`w-full flex items-center justify-between px-3.5 py-2.5 rounded-xl text-sm font-semibold transition-all ${
                  isActive
                    ? 'bg-gradient-to-r from-cyan-500/15 to-purple-600/15 text-white border border-cyan-500/30 shadow-sm'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-[#141B32]'
                }`}
              >
                <div className="flex items-center gap-3">
                  <Icon className={`w-4 h-4 ${isActive ? 'text-cyan-400' : 'text-slate-400'}`} />
                  <span>{item.label}</span>
                </div>
                {item.badge !== undefined && (
                  <span
                    className={`text-xs px-2 py-0.5 rounded-full border font-mono font-bold ${item.badgeColor}`}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Bottom Section: Disk Free & Attribution */}
      <div className="space-y-3 pt-4 border-t border-[#1A2342]">
        {/* Turbo Engine Status */}
        <div className="bg-[#121933] border border-[#202B52] rounded-xl p-3 flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-cyan-500/15 flex items-center justify-center text-cyan-400">
            <Sparkles className="w-4 h-4" />
          </div>
          <div>
            <p className="text-xs font-bold text-slate-200">محرك Turbo Direct-Write</p>
            <p className="text-[10px] text-cyan-400 font-mono">32 Stream • 0ms Lag</p>
          </div>
        </div>

        {/* Disk Space Meter */}
        <div className="bg-[#121933] border border-[#202B52] rounded-xl p-3">
          <div className="flex items-center justify-between text-xs text-slate-400 mb-1.5">
            <span className="flex items-center gap-1.5 font-medium">
              <HardDrive className="w-3.5 h-3.5 text-cyan-400" />
              <span>المساحة المتاحة</span>
            </span>
            <span className="text-slate-200 font-mono font-bold">142.8 GB</span>
          </div>
          <div className="w-full h-1.5 bg-[#1B254A] rounded-full overflow-hidden">
            <div className="h-full bg-gradient-to-r from-cyan-400 to-purple-500 w-[68%] rounded-full"></div>
          </div>
        </div>

        {/* Developer Attribution Link */}
        <button
          onClick={onOpenAboutModal}
          className="w-full py-2 px-3 rounded-lg hover:bg-[#141B32] transition-colors flex items-center justify-between text-xs text-slate-400 group cursor-pointer"
        >
          <div className="flex items-center gap-2">
            <Info className="w-3.5 h-3.5 text-slate-400 group-hover:text-cyan-400" />
            <span className="group-hover:text-slate-200">عن البرنامج والحقوق</span>
          </div>
          <span className="text-[10px] text-cyan-400 font-mono font-semibold">Ramy Badraan</span>
        </button>
      </div>
    </aside>
  );
};
