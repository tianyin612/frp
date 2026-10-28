import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 需要固定端口、不清屏、忽略 src-tauri 的变更
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "safari15", // macOS WKWebView / Windows WebView2 都能吃
    sourcemap: false,
  },
});
