//! StatsCards Component with Realtime Speed Sparkline
//! Developed by Ramy Badraan

import React, { useEffect, useRef } from 'react';
import { Activity, Zap, CheckCircle2, Cpu } from 'lucide-react';
import { formatSpeed } from '../utils/format';

interface StatsCardsProps {
  totalSpeedBps: number;
  activeCount: number;
  completedCount: number;
  speedHistory: number[];
}

export const StatsCards: React.FC<StatsCardsProps> = ({
  totalSpeedBps,
  activeCount,
  completedCount,
  speedHistory,
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Draw real-time sparkline chart
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;
    ctx.clearRect(0, 0, width, height);

    if (speedHistory.length < 2) return;

    const maxSpeed = Math.max(...speedHistory, 1024 * 1024); // min 1MB/s scale
    const step = width / (speedHistory.length - 1);

    // Gradient fill
    const grad = ctx.createLinearGradient(0, 0, 0, height);
    grad.addColorStop(0, 'rgba(0, 245, 255, 0.35)');
    grad.addColorStop(1, 'rgba(0, 245, 255, 0.0)');

    ctx.beginPath();
    ctx.moveTo(0, height);

    speedHistory.forEach((speed, i) => {
      const x = i * step;
      const y = height - (speed / maxSpeed) * (height - 8);
      if (i === 0) {
        ctx.lineTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    });

    ctx.lineTo(width, height);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();

    // Line stroke
    ctx.beginPath();
    speedHistory.forEach((speed, i) => {
      const x = i * step;
      const y = height - (speed / maxSpeed) * (height - 8);
      if (i === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    });
    ctx.strokeStyle = '#00F5FF';
    ctx.lineWidth = 2.5;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.stroke();
  }, [speedHistory]);

  return (
    <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
      {/* 1. Total Speed Card with Sparkline */}
      <div className="bg-[#121933] border border-[#202B52] rounded-2xl p-4 flex flex-col justify-between relative overflow-hidden group hover:border-cyan-500/40 transition-colors">
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs font-semibold text-slate-400">إجمالي سرعة التحميل</span>
          <div className="w-7 h-7 rounded-lg bg-cyan-500/15 flex items-center justify-center text-cyan-400">
            <Activity className="w-4 h-4" />
          </div>
        </div>
        <div className="z-10">
          <div className="text-2xl font-black font-mono text-cyan-400 tracking-tight">
            {formatSpeed(totalSpeedBps)}
          </div>
          <div className="text-[11px] text-slate-400 font-medium">معدل النقل الفعلي المباشر</div>
        </div>
        {/* Canvas Sparkline */}
        <div className="h-12 w-full mt-2">
          <canvas ref={canvasRef} width={260} height={50} className="w-full h-full" />
        </div>
      </div>

      {/* 2. Active Downloads */}
      <div className="bg-[#121933] border border-[#202B52] rounded-2xl p-4 flex flex-col justify-between hover:border-purple-500/40 transition-colors">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold text-slate-400">تنزيلات نشطة</span>
          <div className="w-7 h-7 rounded-lg bg-purple-500/15 flex items-center justify-center text-purple-400">
            <Zap className="w-4 h-4" />
          </div>
        </div>
        <div>
          <div className="text-3xl font-black font-mono text-purple-400 my-1">
            {activeCount}
          </div>
          <div className="text-[11px] text-slate-400 font-medium">
            {activeCount > 0 ? 'تعمل بكامل قنوات التوازي' : 'لا توجد تنزيلات نشطة'}
          </div>
        </div>
      </div>

      {/* 3. Completed Downloads */}
      <div className="bg-[#121933] border border-[#202B52] rounded-2xl p-4 flex flex-col justify-between hover:border-emerald-500/40 transition-colors">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold text-slate-400">الملفات المكتملة</span>
          <div className="w-7 h-7 rounded-lg bg-emerald-500/15 flex items-center justify-center text-emerald-400">
            <CheckCircle2 className="w-4 h-4" />
          </div>
        </div>
        <div>
          <div className="text-3xl font-black font-mono text-emerald-400 my-1">
            {completedCount}
          </div>
          <div className="text-[11px] text-slate-400 font-medium">جاهزة فوراً دون انتظار دمج</div>
        </div>
      </div>

      {/* 4. Turbo Concurrency Engine */}
      <div className="bg-[#121933] border border-[#202B52] rounded-2xl p-4 flex flex-col justify-between hover:border-pink-500/40 transition-colors">
        <div className="flex items-center justify-between">
          <span className="text-xs font-semibold text-slate-400">محرك التسريع الموازي</span>
          <div className="w-7 h-7 rounded-lg bg-pink-500/15 flex items-center justify-center text-pink-400">
            <Cpu className="w-4 h-4" />
          </div>
        </div>
        <div className="flex items-center justify-between">
          <div>
            <div className="text-2xl font-black font-mono text-gradient">
              x32 Turbo
            </div>
            <div className="text-[11px] text-slate-400 font-medium">Work Stealing مفعل</div>
          </div>
          <div className="w-12 h-12 rounded-full border-2 border-cyan-400/30 border-t-cyan-400 flex items-center justify-center text-xs font-mono font-bold text-cyan-300">
            100%
          </div>
        </div>
      </div>
    </div>
  );
};
