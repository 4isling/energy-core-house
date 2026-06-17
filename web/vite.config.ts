import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

// Le cœur Rust est compilé en WASM (wasm-pack) dans src/pkg ; les plugins
// wasm + top-level-await permettent l'import du module et son `init()`.
export default defineConfig({
  base: "./",
  plugins: [react(), wasm(), topLevelAwait()],
});
