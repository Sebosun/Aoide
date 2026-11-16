import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  // The global sheets are processed so tests can read them (`?raw`); only theme.css is applied.
  test: { environment: "jsdom", css: { include: [/src\/styles\/[\w-]+\.css/] } },
});
