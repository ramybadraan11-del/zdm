//! ZDM Logo Mark
//! Developed by Ramy Badraan
//!
//! Inline SVG of the brand mark so it renders instantly, scales crisply, and
//! needs no network/asset request. The gradient ids are suffixed per instance to
//! avoid collisions when several sizes appear on the same screen.

import React, { useId } from 'react';

interface LogoProps {
  size?: number;
  /** Rounded square plate vs. transparent mark. */
  variant?: 'plate' | 'mark';
  className?: string;
}

export const Logo: React.FC<LogoProps> = ({ size = 40, variant = 'plate', className = '' }) => {
  const uid = useId().replace(/[:]/g, '');
  const body = `body-${uid}`;
  const rim = `rim-${uid}`;
  const mark = `mark-${uid}`;
  const shelf = `shelf-${uid}`;
  const halo = `halo-${uid}`;

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 512 512"
      className={className}
      role="img"
      aria-label="ZDM"
    >
      <defs>
        <linearGradient id={body} x1="0" y1="0" x2="0.85" y2="1">
          <stop offset="0" stopColor="#1E2E52" />
          <stop offset="0.5" stopColor="#111C36" />
          <stop offset="1" stopColor="#080F1F" />
        </linearGradient>
        <linearGradient id={rim} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#9BE2FF" stopOpacity="0.9" />
          <stop offset="0.5" stopColor="#3B82F6" stopOpacity="0.18" />
          <stop offset="1" stopColor="#22D3EE" stopOpacity="0.7" />
        </linearGradient>
        <linearGradient id={mark} x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stopColor="#7CF2DC" />
          <stop offset="0.42" stopColor="#38BDF8" />
          <stop offset="1" stopColor="#8B5CF6" />
        </linearGradient>
        <linearGradient id={shelf} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#38BDF8" />
          <stop offset="1" stopColor="#A78BFA" />
        </linearGradient>
        <radialGradient id={halo} cx="0.5" cy="0.38" r="0.52">
          <stop offset="0" stopColor="#38BDF8" stopOpacity="0.22" />
          <stop offset="0.7" stopColor="#6366F1" stopOpacity="0.06" />
          <stop offset="1" stopColor="#000000" stopOpacity="0" />
        </radialGradient>
      </defs>

      {variant === 'plate' ? (
        <>
          <rect x="16" y="16" width="480" height="480" rx="116" fill={`url(#${body})`} />
          <rect x="16" y="16" width="480" height="480" rx="116" fill={`url(#${halo})`} />
          <rect
            x="17.5"
            y="17.5"
            width="477"
            height="477"
            rx="114.5"
            fill="none"
            stroke={`url(#${rim})`}
            strokeWidth="3"
          />
        </>
      ) : null}

      <path
        fill={`url(#${mark})`}
        d="M128 128 H384 V178 L236 268 H384 V320 H128 V270 L276 180 H128 Z"
      />
      <rect x="176" y="366" width="160" height="22" rx="11" fill={`url(#${shelf})`} />
    </svg>
  );
};

/** Wordmark: mark + product name, used in the sidebar header. */
export const LogoWordmark: React.FC<{ size?: number; subtitle?: string }> = ({
  size = 38,
  subtitle,
}) => (
  <div className="flex items-center gap-3 min-w-0">
    <div className="shrink-0 rounded-2xl overflow-hidden ring-1 ring-lineStrong/60 shadow-panel">
      <Logo size={size} />
    </div>
    <div className="min-w-0">
      <div className="flex items-baseline gap-1.5">
        <span className="text-[15px] font-extrabold tracking-tight brand-text leading-tight">
          Zyzoom
        </span>
        <span className="text-[12px] font-bold tracking-[0.16em] text-inkMuted uppercase">
          ZDM
        </span>
      </div>
      {subtitle ? (
        <div className="text-[12.5px] text-inkMuted truncate leading-tight mt-0.5">{subtitle}</div>
      ) : null}
    </div>
  </div>
);

/**
 * Compact brand mark from the design guide: a small Volt dot before the word
 * "ZDM". Pure CSS, so it inherits the theme and costs no SVG ids.
 *
 * The guide describes the logo as «ZDM» in heavy type with a small Volt dot
 * before it — this is that mark, drawn to sit inside the 52px command bar.
 */
export const LogoMark: React.FC<{ size?: number; className?: string }> = ({
  size = 20,
  className = '',
}) => (
  <span
    className={`logo-mark ${className}`.trim()}
    style={{ width: size, height: size }}
    aria-hidden="true"
  >
    <span className="logo-mark-dot" />
  </span>
);
