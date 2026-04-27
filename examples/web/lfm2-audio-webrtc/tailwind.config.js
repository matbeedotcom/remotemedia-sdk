/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        panel: '#0f172a',
        panelhi: '#1e293b',
        accent: '#38bdf8',
        good: '#4ade80',
        warn: '#fbbf24',
        bad: '#f87171',
      },
    },
  },
  plugins: [],
}
