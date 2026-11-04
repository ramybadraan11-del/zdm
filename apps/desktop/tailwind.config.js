/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  // Themes are driven by `data-theme` on <html>, never by a `dark` class.
  darkMode: ['class', '[data-theme="moon"]'],
  theme: {
    extend: {
      // All colours resolve through CSS custom properties, so a theme switch is
      // one attribute change with no class churn and no rebuild.
      colors: {
        base: 'var(--bg-base)',
        canvas: 'var(--bg-canvas)',
        panel: 'var(--bg-panel)',
        panelHover: 'var(--bg-panel-hover)',
        elevated: 'var(--bg-elevated)',
        input: 'var(--bg-input)',
        line: 'var(--border)',
        lineStrong: 'var(--border-strong)',
        ink: 'var(--text-primary)',
        inkSoft: 'var(--text-secondary)',
        inkMuted: 'var(--text-muted)',
        accent1: 'var(--accent-1)',
        accent2: 'var(--accent-2)',
        accent3: 'var(--accent-3)',
        success: 'var(--success)',
        warning: 'var(--warning)',
        danger: 'var(--danger)',
      },
      fontFamily: {
        sans: [
          'Cairo',
          'Hacen Tunisia',
          'Hacen',
          'IBM Plex Sans Arabic',
          'Inter',
          'system-ui',
          'sans-serif',
        ],
        mono: ['JetBrains Mono', 'ui-monospace', 'monospace'],
      },
      borderRadius: {
        xs: '8px',
        sm: '10px',
        DEFAULT: '14px',
        lg: '20px',
        xl: '26px',
      },
      boxShadow: {
        panel: 'var(--shadow-panel)',
        pop: 'var(--shadow-pop)',
      },
      transitionTimingFunction: {
        smooth: 'cubic-bezier(0.22, 1, 0.36, 1)',
      },
      keyframes: {
        rise: {
          from: { opacity: '0', transform: 'translateY(10px) scale(0.985)' },
          to: { opacity: '1', transform: 'translateY(0) scale(1)' },
        },
      },
      animation: {
        rise: 'rise 320ms cubic-bezier(0.22, 1, 0.36, 1) both',
      },
    },
  },
  plugins: [],
};
