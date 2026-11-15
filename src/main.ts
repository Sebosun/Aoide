import { createApp } from "vue";
import { createPinia } from "pinia";
import { isTauri } from "@tauri-apps/api/core";
import { applyPreviewParams } from "@/preview";
import { api } from "@/api";
import { report } from "@/log";
import { useNavigationStore } from "@/stores/navigation";
import App from "@/App.vue";
import "@/styles/fonts.css";
import "@/styles/tokens.css";
import "@/styles/theme.css";
import "@/styles/base.css";

const app = createApp(App).use(createPinia());
/** Sends an error no view caught to Aoide's log, with the view on screen, and to the console. */
const uncaught = (error: unknown) => {
  console.error(error);
  const msg = error instanceof Error ? `${error.name}: ${error.message}` : String(error);
  report({ level: "error", event: "ui.error", msg, view: useNavigationStore().view }, (entry) => api.logUi(entry));
};
app.config.errorHandler = uncaught;
window.addEventListener("unhandledrejection", (event) => uncaught(event.reason));
app.mount("#app");
if (!isTauri()) void applyPreviewParams();
