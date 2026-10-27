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
  build: {
    outDir: 'dist',
    sourcemap: false,
  },
})
