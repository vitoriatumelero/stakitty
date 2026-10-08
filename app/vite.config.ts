import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// base: "./" keeps the build portable (GitHub Pages, Vercel, any static host).
export default defineConfig({
  plugins: [react()],
  base: "./",
});
