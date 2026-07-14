import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: [],
  },
  server: {
    port: 3000,
    // Localhost-only by default (secure). To reach the dev server over
    // LAN/Tailscale, set VITE_HOST:
    //   VITE_HOST=true       all interfaces (0.0.0.0)
    //   VITE_HOST=100.x.y.z  bind the host's Tailscale IP (tailnet-only)
    host: process.env.VITE_HOST === 'true' ? true : process.env.VITE_HOST,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});
