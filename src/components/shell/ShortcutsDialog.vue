<script setup lang="ts">
import { X } from "@lucide/vue";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import ShortcutList from "@/components/shell/ShortcutList.vue";
import { useNavigationStore } from "@/stores/navigation";

/** The keyboard shortcuts, over any screen (the ? key). */
const navigation = useNavigationStore();
</script>

<template>
  <UiDialog label="Keyboard shortcuts" @close="navigation.closeShortcuts">
    <div class="shortcuts-dialog">
      <header class="shortcuts-dialog__head">
        <h1 class="shortcuts-dialog__title">Keyboard shortcuts</h1>
        <UiButton variant="glass" size="sm" icon aria-label="Close shortcuts" @click="navigation.closeShortcuts">
          <X :size="17" />
        </UiButton>
      </header>
      <div class="shortcuts-dialog__body">
        <ShortcutList />
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.shortcuts-dialog {
  width: 560px;
  max-width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-7);
  padding: var(--space-12) var(--space-12) var(--space-10);
}
.shortcuts-dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-6);
}
.shortcuts-dialog__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.shortcuts-dialog__body {
  min-height: 0;
  overflow-y: auto;
}
</style>
