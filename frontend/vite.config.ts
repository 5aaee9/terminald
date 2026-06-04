import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  build: {
    rollupOptions: {
      output: {
        assetFileNames: (assetInfo) => (
          assetInfo.names?.some((name) => name.endsWith(".css"))
            ? "assets/terminald.css"
            : "assets/[name][extname]"
        ),
        entryFileNames: "assets/terminald.js",
        inlineDynamicImports: true,
      },
    },
  },
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "./vitest.setup.ts",
  },
});
