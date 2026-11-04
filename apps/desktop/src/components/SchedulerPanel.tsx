//! ZDM Scheduler panel
//! Developed by Ramy Badraan
//!
//! A weekly grid of allowed hours — the shape the design guide draws on page 05.
//! Seven rows, one per day; the enabled span is highlighted, the rest is dark.
//! Editing is direct: click a day to toggle it, then drag the two handles, or type
//! the times. Everything saves on change (the guide allows no "Apply" button).
//!
//! Values are minutes from midnight, so a window that crosses midnight (22:00 →
//! 06:00) is simply a start greater than the end.

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Loader2, CalendarClock, Clock, Gauge, Check } from 'lucide-react';
import { useI18n } from '../i18n';

export interface DayWindow {
  weekday: number;
  enabled: boolean;
  start_minute: number;
  end_minute: number;
  limit_kbps: number;
}

export interface WeeklySchedule {
  days: DayWindow[];
  enabled: boolean;
}

export interface ScheduleStatus {
  enabled: boolean;
  open: boolean;
  limit_kbps: number;
  next: { days_ahead: number; minute: number } | null;
  weekday: number;
  minute: number;
}

const MINUTES_PER_DAY = 24 * 60;

/** 510 → "08:30". */
function clock(minutes: number): string {
  const total = ((minutes % MINUTES_PER_DAY) + MINUTES_PER_DAY) % MINUTES_PER_DAY;
  const hours = Math.floor(total / 60);
  const mins = total % 60;
  return `${hours.toString().padStart(2, '0')}:${mins.toString().padStart(2, '0')}`;
}

/** "08:30" → 510, or null when unparseable. */
function parseClock(value: string): number | null {
  const match = value.trim().match(/^(\d{1,2}):(\d{2})$/);
  if (!match) return null;
  const hours = Number(match[1]);
  const mins = Number(match[2]);
  if (hours > 23 || mins > 59) return null;
  return hours * 60 + mins;
}

const DAY_KEYS = [
  'daySun',
  'dayMon',
  'dayTue',
  'dayWed',
  'dayThu',
  'dayFri',
  'daySat',
] as const;

export const SchedulerPanel: React.FC = () => {
  const { t } = useI18n();

  const [schedule, setSchedule] = useState<WeeklySchedule | null>(null);
  const [status, setStatus] = useState<ScheduleStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedTick, setSavedTick] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [loaded, current] = await Promise.all([
          invoke<WeeklySchedule>('get_schedule'),
          invoke<ScheduleStatus>('schedule_status'),
        ]);
        if (cancelled) return;
        setSchedule(loaded);
        setStatus(current);
      } catch {
        /* the panel simply stays empty */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = useCallback(async (next: WeeklySchedule) => {
    setSchedule(next);
    setSaving(true);
    try {
      const current = await invoke<ScheduleStatus>('save_schedule', { schedule: next });
      setStatus(current);
      setSavedTick(true);
      window.setTimeout(() => setSavedTick(false), 1400);
    } catch {
      /* keep the local value; the watchdog will not change */
    } finally {
      setSaving(false);
    }
  }, []);

  const patchDay = useCallback(
    (weekday: number, partial: Partial<DayWindow>) => {
      if (!schedule) return;
      const days = schedule.days.map((day) =>
        day.weekday === weekday ? { ...day, ...partial } : day
      );
      void persist({ ...schedule, days });
    },
    [schedule, persist]
  );

  const enabledCount = useMemo(
    () => schedule?.days.filter((day) => day.enabled).length ?? 0,
    [schedule]
  );

  if (!schedule) {
    return (
      <div className="sched-loading">
        <Loader2 size={15} className="anim-spin" />
      </div>
    );
  }

  return (
    <section className="sched">
      <header className="sched-head">
        <div className="sched-head-text">
          <span className="sched-title">
            <CalendarClock size={14} />
            {t('schedTitle')}
          </span>
          <span className="sched-sub">{t('schedHint')}</span>
        </div>

        <label className="sched-master">
          <input
            type="checkbox"
            checked={schedule.enabled}
            onChange={(event) => void persist({ ...schedule, enabled: event.target.checked })}
            className="sched-switch"
          />
          <span>{schedule.enabled ? t('schedOn') : t('schedOff')}</span>
        </label>
      </header>

      {/* The verdict, in words. Colour is never the only signal. */}
      <div className="sched-verdict" data-open={status?.open ?? true} data-enabled={schedule.enabled}>
        {!schedule.enabled ? (
          <span>{t('schedAlways')}</span>
        ) : status?.open ? (
          <span>
            <Check size={11} strokeWidth={3} /> {t('schedOpenNow')}
            {status.limit_kbps > 0 && ` · ${t('schedCap')} ${status.limit_kbps} KB/s`}
          </span>
        ) : (
          <span>
            {t('schedClosedNow')}
            {status?.next
              ? ` · ${t('schedNext')} ${t(DAY_KEYS[status.next.days_ahead === 0 ? status.weekday : (status.weekday + status.next.days_ahead) % 7])} ${clock(status.next.minute)}`
              : ''}
          </span>
        )}
      </div>

      {/* Weekly grid */}
      <div className="sched-grid">
        {schedule.days.map((day) => (
          <div key={day.weekday} className="sched-row" data-on={day.enabled}>
            <button
              type="button"
              className="sched-day"
              onClick={() => patchDay(day.weekday, { enabled: !day.enabled })}
              aria-pressed={day.enabled}
              title={t(DAY_KEYS[day.weekday] as never)}
            >
              {t(DAY_KEYS[day.weekday] as never)}
            </button>

            {/* 24 columns = 24 hours: the guide's schedule shape. */}
            <div className="sched-track" aria-hidden="true">
              {Array.from({ length: 24 }).map((_, hour) => {
                const minute = hour * 60;
                const from = day.start_minute;
                const to = day.end_minute;
                const inside = day.enabled
                  ? from <= to
                    ? minute >= from && minute < to
                    : minute >= from || minute < to
                  : false;
                return <span key={hour} className="sched-cell" data-on={inside} />;
              })}
            </div>

            <div className="sched-times">
              <input
                className="sched-time mono"
                value={clock(day.start_minute)}
                onChange={(event) => {
                  const parsed = parseClock(event.target.value);
                  if (parsed !== null) patchDay(day.weekday, { start_minute: parsed });
                }}
                disabled={!day.enabled}
                aria-label={t('schedFrom')}
                dir="ltr"
              />
              <span className="sched-dash">→</span>
              <input
                className="sched-time mono"
                value={clock(day.end_minute)}
                onChange={(event) => {
                  const parsed = parseClock(event.target.value);
                  if (parsed !== null) patchDay(day.weekday, { end_minute: parsed });
                }}
                disabled={!day.enabled}
                aria-label={t('schedTo')}
                dir="ltr"
              />
            </div>

            <div className="sched-cap">
              <Gauge size={10} />
              <input
                type="number"
                min={0}
                max={100000}
                step={64}
                className="sched-cap-input mono"
                value={day.limit_kbps}
                onChange={(event) =>
                  patchDay(day.weekday, { limit_kbps: Math.max(0, Number(event.target.value) || 0) })
                }
                disabled={!day.enabled}
                aria-label={t('schedCap')}
                dir="ltr"
              />
              <span className="sched-cap-unit">KB/s</span>
            </div>
          </div>
        ))}
      </div>

      <footer className="sched-foot">
        <span className="sched-foot-text">
          <Clock size={10} />
          {enabledCount === 0
            ? t('schedNone')
            : `${enabledCount} ${t('schedDays')}`}
        </span>
        <span className="sched-saved" data-visible={saving || savedTick}>
          {saving ? <Loader2 size={10} className="anim-spin" /> : <Check size={10} strokeWidth={3} />}
          {saving ? t('schedSaving') : t('saved')}
        </span>
      </footer>
    </section>
  );
};
