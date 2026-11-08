import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Static SPA ONLY (README §3): no Next.js server features, so the Tauri
// desktop shell can embed the same build output.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      // dev-time API proxy to the Rust server
      '/api': 'http://localhost:8080',
    },
  },
  // preview (pnpm preview): serve the production build on 4173 with the same API proxy
  preview: {
    port: 4173,
    proxy: {
      '/api': 'http://localhost:8080',
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
  },
})
