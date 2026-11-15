<script setup lang="ts">
import ConfirmDialog from "@/components/ui/ConfirmDialog.vue";
import PromptDialog from "@/components/ui/PromptDialog.vue";
import ToastStack from "@/components/ui/ToastStack.vue";
import { useUiStore } from "@/stores/ui";
/** Renders the app-wide confirm and prompt dialogs and toasts (see stores/ui). Mounted once by App. */
const ui = useUiStore();
</script>

<template>
  <ConfirmDialog v-if="ui.pending" :key="ui.pending.id" :options="ui.pending" @answer="ui.answer" />
  <PromptDialog v-if="ui.asking" :key="ui.asking.id" :options="ui.asking" @answer="ui.reply" />
  <ToastStack />
</template>
