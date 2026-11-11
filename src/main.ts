import { createApp } from "vue";
import { createPinia } from "pinia";
import { isTauri } from "@tauri-apps/api/core";
import { applyPreviewParams } from "./preview";
import App from "./App.vue";
import "./styles/fonts.css";
import "./styles/theme.css";
import "./styles/base.css";

createApp(App).use(createPinia()).mount("#app");
if (!isTauri()) void applyPreviewParams();
