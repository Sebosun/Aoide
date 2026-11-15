import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { SignInView } from "../api/types";
import { useUiStore } from "./ui";

/** The sign-in the core runs (core/signin.rs), kept current from "signin://state". The dialog
 *  shows while one is open (and after it fails, until dismissed); a finished one is reported
 *  as a toast. */
export const useSignInStore = defineStore("signin", () => {
  const view = ref<SignInView | null>(null);
  /** The dialog is showing (an open sign-in, or one that failed and is not dismissed). */
  const shown = ref(false);
  const error = ref("");
  let startup: Promise<void> | undefined;

  const open = computed(
    () => view.value !== null && ["opening", "waiting", "fetching"].includes(view.value.stage.kind),
  );

  /** What a finished sign-in tells the user. */
  function report(done: SignInView) {
    const ui = useUiStore();
    if (done.stage.kind === "done")
      ui.toast(
        done.result?.kind === "ready"
          ? `Signed in to ${done.site}: the whole article was fetched.`
          : `Fetched ${done.site} again, but still not the whole article.`,
        done.result?.kind === "ready" ? "info" : "error",
      );
    else if (done.stage.kind === "cancelled") ui.toast("Sign-in cancelled; the article is as it was.");
  }

  function receive(next: SignInView) {
    const was = view.value?.stage.kind;
    view.value = next;
    if (next.stage.kind === "done" || next.stage.kind === "cancelled") {
      if (shown.value && was !== next.stage.kind) report(next);
      shown.value = false;
    } else shown.value = true;
  }

  function init() {
    startup ??= (async () => {
      await api.onSignInState(receive);
      const current = await api.signIn();
      if (current) {
        view.value = current;
        shown.value = ["opening", "waiting", "fetching"].includes(current.stage.kind);
      }
    })();
    return startup;
  }

  async function run(task: () => Promise<unknown>) {
    error.value = "";
    try {
      await task();
      return true;
    } catch (e) {
      error.value = String(e).replace(/^Error: /, "");
      return false;
    }
  }

  return {
    view,
    shown,
    open,
    error,
    init,
    /** Opens a window on the article's page; refused while the browser is off. */
    start: (articleId: string) =>
      run(async () => {
        await init();
        receive(await api.startSignIn(articleId));
      }),
    finish: () => run(() => api.finishSignIn()),
    cancel: () => run(() => api.cancelSignIn()),
    showWindow: () => run(() => api.showSignInWindow()),
    stay: (stay: boolean) => run(() => api.staySignedIn(stay)),
    /** Closes a failed sign-in's dialog. */
    dismiss: () => {
      shown.value = false;
    },
  };
});
