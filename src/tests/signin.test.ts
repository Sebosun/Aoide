import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import NeedsYouNotice from "@/components/article/NeedsYouNotice.vue";
import { useUiStore } from "@/stores/ui";
import { useNavigationStore } from "@/stores/navigation";
import type { ArticleView, FetchReason } from "@/api/types";
import fixtureArticle from "@/tests/fixtures/article.json";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => {
  setActivePinia(createPinia());
});
afterEach(() => {
  mock?.dispose();
  vi.useRealTimers();
  document.body.innerHTML = "";
});

/** The app with the mock made from `search`, on the library. */
async function app(search: string) {
  mock = createMockApi(search);
  backend.api = mock;
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const dialog = () => document.body.querySelector<HTMLElement>('[role="dialog"]');
const buttonIn = (text: string) =>
  [...(dialog()?.querySelectorAll("button") ?? [])].find((b) => b.textContent?.trim() === text)!;

it("offers the window that fits each reason, or turning the browser on", () => {
  const article = { ...(fixtureArticle.article as unknown as ArticleView), declaredWords: null };
  const first = (reason: FetchReason, browserOn: boolean) =>
    mount(NeedsYouNotice, { props: { article, reason, browserOn } }).findAll("button")[0];
  expect(first("signIn", true).text()).toBe("Sign in to Quay Weekly…");
  expect(first("paywall", true).text()).toBe("Sign in to Quay Weekly…");
  expect(first("check", true).text()).toBe("Open the check in a window…");
  expect(first("cutShort", true).text()).toBe("Open in a window…");
  expect(first("empty", true).text()).toBe("Open in a window…");
  for (const reason of ["signIn", "check", "empty"] as FetchReason[])
    expect(first(reason, false).text()).toBe("Turn on the browser…");
  const notice = mount(NeedsYouNotice, { props: { article, reason: "check", browserOn: true } });
  void notice.findAll("button")[0].trigger("click");
  expect(notice.emitted("signIn")).toHaveLength(1);
});

it("shows the sign-in's steps, keeps the choice to stay signed in, and closes with the result once fetched", async () => {
  const wrapper = await app("?signin=waiting");
  expect(dialog()?.getAttribute("aria-label")).toBe("Sign in to quay.example");
  expect(dialog()?.textContent).toContain("For “The Quay Lights”");
  const steps = () => [...dialog()!.querySelectorAll("li")];
  expect(steps().map((s) => s.className.match(/signin__step--(\w+)/)?.[1])).toEqual(["done", "current", "todo"]);
  expect(steps()[1].getAttribute("aria-current")).toBe("step");

  const stay = vi.spyOn(mock, "staySignedIn"),
    show = vi.spyOn(mock, "showSignInWindow");
  dialog()!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  await flushPromises();
  expect(stay).toHaveBeenCalledWith({ stay: false });
  buttonIn("Show the window").click();
  await flushPromises();
  expect(show).toHaveBeenCalled();

  vi.useFakeTimers();
  buttonIn("I’m signed in, fetch again").click();
  await vi.advanceTimersByTimeAsync(0);
  expect(steps()[2].getAttribute("aria-current")).toBe("step");
  expect(buttonIn("Cancel").disabled).toBe(true);
  await vi.advanceTimersByTimeAsync(500);
  vi.useRealTimers();
  await flushPromises();
  expect(dialog()).toBeNull();
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Signed in to quay.example: the whole article was fetched.");
  // Not staying signed in: the site keeps no session.
  const site = (await mock.sites()).find((s) => s.host === "quay.example");
  expect(site?.session.kind).not.toBe("signedIn");
  wrapper.unmount();
});

it("cancels from the dialog or with Escape, leaving the article as it was", async () => {
  const wrapper = await app("?signin=waiting");
  const before = await mock.article({ bookId: "book-6" });
  const cancel = vi.spyOn(mock, "cancelSignIn");
  dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await flushPromises();
  expect(cancel).toHaveBeenCalled();
  expect(dialog()).toBeNull();
  expect(useUiStore().toasts.map((t) => t.text)).toContain("Sign-in cancelled; the article is as it was.");
  expect(await mock.article({ bookId: "book-6" })).toEqual(before);
  wrapper.unmount();
});
