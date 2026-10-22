//! Zyzoom Download Manager (ZDM) - Main Application
//! Developed by Ramy Badraan

import React, { useState, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Header } from './components/Header';
import { Sidebar, NavTab } from './components/Sidebar';
import { StatsCards } from './components/StatsCards';
import { DownloadCard } from './components/DownloadCard';
import { AddDownloadModal } from './components/AddDownloadModal';
import { VideoDownloaderModal } from './components/VideoDownloaderModal';
import { AboutModal } from './components/AboutModal';
import { SettingsView } from './components/SettingsView';
import { DownloadItem, EngineStats, NewDownloadRequest, UrlMetadata, VideoInfo } from './types';
import { Inbox, RefreshCw } from 'lucide-react';

export const App: React.FC = () => {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [stats, setStats] = useState<EngineStats>({
    total_speed_bps: 0,
    active_count: 0,
    queued_count: 0,
    completed_count: 0,
    disk_free_bytes: 142 * 1024 * 1024 * 1024,
  });

  const [currentNav, setCurrentNav] = useState<NavTab>('all');
  const [activeCategory, setActiveCategory] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [speedHistory, setSpeedHistory] = useState<number[]>(new Array(40).fill(0));

  // Modals state
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isVideoModalOpen, setIsVideoModalOpen] = useState(false);
  const [isAboutModalOpen, setIsAboutModalOpen] = useState(false);

  // Load downloads from Rust backend
  const fetchDownloads = async () => {
    try {
      const items: DownloadItem[] = await invoke('get_downloads');
      setDownloads(items);
      const st: EngineStats = await invoke('get_stats');
      setStats(st);
    } catch (err) {
      console.error('Failed to fetch downloads:', err);
    }
  };

  useEffect(() => {
    fetchDownloads();

    // Listen to real-time progress events from Rust
    const unlistenProgress = listen<any>('download_progress', (event) => {
      const { id, downloaded_bytes, total_bytes, speed_bps, eta_seconds, progress_percent, segments } = event.payload;

      setDownloads((prev) =>
        prev.map((item) => {
          if (item.id === id) {
            return {
              ...item,
              downloaded_size: downloaded_bytes,
              total_size: total_bytes > 0 ? total_bytes : item.total_size,
              speed_bps,
              eta_seconds,
              progress_percent,
              segments,
              status: 'downloading',
            };
          }
          return item;
        })
      );

      // Update speed history
      setSpeedHistory((prev) => {
        const next = [...prev.slice(1), speed_bps];
        return next;
      });

      // Update stats total speed
      setStats((prev) => ({
        ...prev,
        total_speed_bps: speed_bps,
      }));
    });

    const unlistenCompleted = listen<any>('download_completed', () => {
      fetchDownloads();
    });

    const unlistenPaused = listen<any>('download_paused', () => {
      fetchDownloads();
    });

    const unlistenDeleted = listen<any>('download_deleted', () => {
      fetchDownloads();
    });

    const unlistenOpenAdd = listen<any>('open_add_modal', () => {
      setIsAddModalOpen(true);
    });

    return () => {
      unlistenProgress.then((f) => f());
      unlistenCompleted.then((f) => f());
      unlistenPaused.then((f) => f());
      unlistenDeleted.then((f) => f());
      unlistenOpenAdd.then((f) => f());
    };
  }, []);

  // Handlers
  const handlePause = async (id: string) => {
    try {
      await invoke('pause_download', { id });
      fetchDownloads();
    } catch (err) {
      console.error('Error pausing:', err);
    }
  };

  const handleResume = async (id: string) => {
    try {
      await invoke('resume_download', { id });
      fetchDownloads();
    } catch (err) {
      console.error('Error resuming:', err);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke('delete_download', { id, deleteFile: false });
      fetchDownloads();
    } catch (err) {
      console.error('Error deleting:', err);
    }
  };

  const handleOpenFile = async (path: string) => {
    try {
      await invoke('open_file', { path });
    } catch (err) {
      console.error('Error opening file:', err);
    }
  };

  const handleOpenFolder = async (path: string) => {
    try {
      await invoke('open_file_folder', { path });
    } catch (err) {
      console.error('Error opening folder:', err);
    }
  };

  const handleProbeUrl = async (url: string): Promise<UrlMetadata> => {
    return await invoke('probe_url', { url });
  };

  const handleAddDownload = async (req: NewDownloadRequest) => {
    await invoke('add_download', { req });
    await fetchDownloads();
  };

  const handleQueryVideo = async (url: string): Promise<VideoInfo> => {
    return await invoke('query_video_info', { url });
  };

  // Nav selection
  const handleNavSelect = (tab: NavTab) => {
    if (tab === 'video') {
      setIsVideoModalOpen(true);
      return;
    }
    setCurrentNav(tab);
  };

  // Filter downloads
  const filteredDownloads = useMemo(() => {
    return downloads.filter((item) => {
      // 1. Sidebar tab filter
      if (currentNav === 'downloading' && item.status !== 'downloading') return false;
      if (currentNav === 'completed' && item.status !== 'completed') return false;
      if (currentNav === 'torrents' && item.category.toLowerCase() !== 'torrents') return false;

      // 2. Category filter
      if (activeCategory !== 'all' && item.category.toLowerCase() !== activeCategory.toLowerCase()) {
        return false;
      }

      // 3. Search query
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        return item.filename.toLowerCase().includes(q) || item.url.toLowerCase().includes(q);
      }

      return true;
    });
  }, [downloads, currentNav, activeCategory, searchQuery]);

  // Categories list
  const categories = [
    { id: 'all', label: 'الكل' },
    { id: 'video', label: 'فيديو' },
    { id: 'audio', label: 'صوتيات' },
    { id: 'compressed', label: 'مضغوطة' },
    { id: 'documents', label: 'مستندات' },
    { id: 'programs', label: 'برامج' },
  ];

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-[#0B1020] text-[#E8ECFF]">
      {/* Sidebar Navigation */}
      <Sidebar
        currentTab={currentNav}
        onSelectTab={handleNavSelect}
        activeCount={stats.active_count}
        completedCount={stats.completed_count}
        onOpenAboutModal={() => setIsAboutModalOpen(true)}
      />

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Top Header */}
        <Header
          searchQuery={searchQuery}
          setSearchQuery={setSearchQuery}
          onOpenAddModal={() => setIsAddModalOpen(true)}
          onOpenAboutModal={() => setIsAboutModalOpen(true)}
        />

        {/* Scrollable Dashboard Body */}
        <main className="flex-1 overflow-y-auto p-6 space-y-6">
          {currentNav === 'settings' ? (
            <SettingsView />
          ) : (
            <>
              {/* Top Performance Stats Cards */}
              <StatsCards
                totalSpeedBps={stats.total_speed_bps}
                activeCount={stats.active_count}
                completedCount={stats.completed_count}
                speedHistory={speedHistory}
              />

              {/* Category Filter Pills & Refresh */}
              <div className="flex items-center justify-between pb-2">
                <div className="flex items-center gap-2 overflow-x-auto">
                  {categories.map((cat) => {
                    const isActive = activeCategory === cat.id;
                    return (
                      <button
                        key={cat.id}
                        onClick={() => setActiveCategory(cat.id)}
                        className={`px-4 py-1.5 rounded-xl text-xs font-bold transition-all cursor-pointer ${
                          isActive
                            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm'
                            : 'bg-[#121933] text-slate-400 hover:text-slate-200 border border-transparent'
                        }`}
                      >
                        {cat.label}
                      </button>
                    );
                  })}
                </div>

                <button
                  onClick={fetchDownloads}
                  title="تحديث القائمة"
                  className="p-2 rounded-xl bg-[#121933] hover:bg-[#1A2346] text-slate-400 hover:text-cyan-400 border border-[#1E294D] transition-colors cursor-pointer"
                >
                  <RefreshCw className="w-4 h-4" />
                </button>
              </div>

              {/* Downloads List */}
              {filteredDownloads.length > 0 ? (
                <div className="space-y-3">
                  {filteredDownloads.map((item) => (
                    <DownloadCard
                      key={item.id}
                      item={item}
                      onPause={handlePause}
                      onResume={handleResume}
                      onDelete={handleDelete}
                      onOpenFile={handleOpenFile}
                      onOpenFolder={handleOpenFolder}
                    />
                  ))}
                </div>
              ) : (
                /* Empty State */
                <div className="bg-[#121933]/50 border border-dashed border-[#1E294D] rounded-3xl p-12 text-center flex flex-col items-center justify-center my-8">
                  <div className="w-16 h-16 rounded-3xl bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center text-cyan-400 mb-4">
                    <Inbox className="w-8 h-8" />
                  </div>
                  <h3 className="text-base font-bold text-slate-200 mb-1">
                    {currentNav === 'torrents' ? 'لا توجد تنزيلات تورنت نشطة' : 'لا توجد تنزيلات في هذه القائمة'}
                  </h3>
                  <p className="text-xs text-slate-400 max-w-sm mb-5">
                    {currentNav === 'torrents'
                      ? 'يمكنك لصق رابط magnet: أو إضافة ملف تورنت لبدء التنزيل المتوازي فوراً.'
                      : 'اضغط على زر "+ إضافة رابط جديد" بالأعلى أو الصق أي رابط لبدء التحميل بأقصى سرعة.'}
                  </p>
                  <button
                    onClick={() => setIsAddModalOpen(true)}
                    className="px-5 py-2.5 rounded-xl bg-gradient-to-r from-cyan-400 via-purple-600 to-pink-500 text-slate-950 font-bold text-xs glow-cyan shadow-md cursor-pointer hover:opacity-95"
                  >
                    + إضافة رابط جديد
                  </button>
                </div>
              )}
            </>
          )}
        </main>

        {/* Global Bottom Status Bar with Developer Copyright */}
        <footer className="h-8 border-t border-[#1E294D] bg-[#090D1C] px-6 flex items-center justify-between text-[11px] text-slate-400 select-none">
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
            <span className="text-slate-300 font-medium">نواة التحميل جاهزة ومستقرة</span>
            <span className="text-slate-600">|</span>
            <span className="text-cyan-400 font-mono font-semibold">32-Stream Direct-Write</span>
          </div>

          {/* Developer Copyright Bar */}
          <div className="flex items-center gap-2">
            <span>Zyzoom Download Manager (ZDM) •</span>
            <button
              onClick={() => setIsAboutModalOpen(true)}
              className="text-cyan-400 hover:text-cyan-300 font-bold font-mono transition-colors cursor-pointer"
            >
              Developed by Ramy Badraan
            </button>
            <span>© 2026</span>
          </div>
        </footer>
      </div>

      {/* Modals */}
      <AddDownloadModal
        isOpen={isAddModalOpen}
        onClose={() => setIsAddModalOpen(false)}
        onProbeUrl={handleProbeUrl}
        onAddDownload={handleAddDownload}
      />

      <VideoDownloaderModal
        isOpen={isVideoModalOpen}
        onClose={() => setIsVideoModalOpen(false)}
        onQueryVideo={handleQueryVideo}
        onStartDownload={async (url, filename, cat) => {
          await handleAddDownload({
            url,
            filename,
            category: cat,
            start_now: true,
          });
        }}
      />

      <AboutModal
        isOpen={isAboutModalOpen}
        onClose={() => setIsAboutModalOpen(false)}
      />
    </div>
  );
};

export default App;
