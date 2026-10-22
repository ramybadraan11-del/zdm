/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: ["class", '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: {
        cyber: {
          cyan: "#00F5FF",
          blue: "#38BDF8",
          purple: "#7C4DFF",
          magenta: "#FF2E93",
          green: "#10B981",
          amber: "#F59E0B",
          red: "#EF4444"
        },
        bg: {
          dark: "#0B1020",
          card: "#121933",
          cardHover: "#172042",
          border: "#1E294D",
          input: "#0F152B"
        }
      },
      fontFamily: {
        sans: ["Cairo", "Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "monospace"]
      }
    },
  },
  plugins: [],
}
