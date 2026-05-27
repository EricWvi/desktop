import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

const root = path.resolve(import.meta.dirname, "../../..");

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
      "@ora/ui": path.resolve(root, "packages/ui/src"),
      "@ora/features": path.resolve(root, "packages/features/src"),
      "@ora/contracts": path.resolve(root, "packages/contracts/src"),
    },
  },
  server: {
    host: "0.0.0.0",
    proxy: {
      "/api": {
        target: `http://localhost:${process.env.ORA_PORT ?? "32578"}`,
        changeOrigin: true,
        // Enable WebSocket proxying for terminal sessions.
        ws: true,
      },
    },
  },
});
