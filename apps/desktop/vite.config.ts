import { defineConfig } from 'vite'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./web"),
      "@ora/ui": path.resolve(__dirname, "../../packages/ui/src"),
      "@ora/features": path.resolve(__dirname, "../../packages/features/src"),
      "@ora/contracts": path.resolve(__dirname, "../../packages/contracts/src"),
    },
  },
  server: {
    host: "0.0.0.0",
    proxy: {
      "/api": {
        target: "http://localhost:32578",
        changeOrigin: true,
      },
    },
  },
})
