<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useId } from "vue";
import { api } from "@/api";
import UiButton from "@/components/ui/UiButton.vue";
import UiDialog from "@/components/ui/UiDialog.vue";
import { useUiStore } from "@/stores/ui";

/** The window was closed while Aoide was working (`window://close-asked`): keep working in
 *  the background, or quit. The core decides when to ask and what each answer does; closing
 *  this question leaves the window open. */
const ui = useUiStore();
const asked = ref(false),
  remember = ref(false),
  id = useId();
let off: (() => void) | undefined,
  mounted = true;
onMounted(() => {
  void api
    .onCloseAsked(() => {
      remember.value = false;
      asked.value = true;
    })
    .then((stop) => (mounted ? (off = stop) : stop()));
});
onBeforeUnmount(() => {
  mounted = false;
  off?.();
});
async function answer(keep: boolean) {
  asked.value = false;
  try {
    await api.chooseClose({ keep, remember: remember.value });
  } catch (e) {
    ui.toast(String(e).replace(/^Error: /, ""), "error");
  }
}
</script>

<template>
  <UiDialog
    v-if="asked"
    label="Keep working in the background?"
    role="alertdialog"
    :describedby="`${id}-body`"
    initial-focus="[data-answer='keep']"
    @close="asked = false"
  >
    <div class="close-question">
      <h2 class="close-question__title">Keep working in the background?</h2>
      <p :id="`${id}-body`" class="close-question__body">
        Aoide is still preparing, syncing or playing. It can carry on with the window closed; starting Aoide again
        brings the window back.
      </p>
      <label class="close-question__remember"
        ><input v-model="remember" type="checkbox" aria-label="Remember my choice" /> Remember my choice</label
      >
      <div class="close-question__actions">
        <UiButton variant="outline" aria-label="Quit" @click="answer(false)">Quit</UiButton>
        <UiButton variant="primary" aria-label="Keep working" data-answer="keep" @click="answer(true)"
          >Keep working</UiButton
        >
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.close-question {
  width: min(440px, calc(100vw - 32px));
  display: grid;
  gap: var(--space-5);
  padding: var(--space-12);
}
.close-question__title {
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.01em;
  line-height: 1.25;
}
.close-question__body {
  color: var(--dim);
  line-height: 1.5;
}
.close-question__remember {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  font-size: 13px;
}
.close-question__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-4);
  margin-top: var(--space-5);
}
</style>
