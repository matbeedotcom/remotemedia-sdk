/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      animation: {
        'soundbar': 'soundbar 0.3s ease-in-out infinite alternate',
        'ping': 'ping 1.5s cubic-bezier(0, 0, 0.2, 1) infinite',
      },
      keyframes: {
        soundbar: {
          '0%': { height: '10%' },
          '100%': { height: '100%' },
        },
      },
    },
  },
  plugins: [],
};
