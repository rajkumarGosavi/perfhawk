import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    // The Prism server's Host guard requires its own host, hence changeOrigin.
    proxy: { '/api': { target: 'http://127.0.0.1:7878', changeOrigin: true } },
  },
})
