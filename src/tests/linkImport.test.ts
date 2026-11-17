import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useNavigationStore } from "@/stores/navigation";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
beforeEach(() => {
  setActivePinia(createPinia());
  mock = createMockApi();
  backend.api = mock;
});
afterEach(() => {
  mock.dispose();
  document.body.innerHTML = "";
});

async function app() {
  useNavigationStore().view = "library";
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}
const inBody = (selector: string) => document.body.querySelector<HTMLElement>(selector);
const click = async (selector: string) => {
  inBody(selector)!.click();
  await flushPromises();
};

it("imports links from another app's export, choosing only the unread ones", async () => {
  const start = vi.spyOn(mock, "startLinkImport");
  const wrapper = await app();
  useNavigationStore().openAdd();
  await flushPromises();
  await click('[aria-label="Import links from another app"]');
  const dialog = inBody('[aria-label="Import from Pocket"]')!;
  expect(dialog.textContent).toContain("12 links");
  expect(dialog.textContent).toContain("3 already in your library");
  dialog.querySelector<HTMLInputElement>('[aria-label="Only the unread ones (9)"]')!.click();
  // The count starts within what was found, so no field blocks the form (jsdom does not check).
  expect(dialog.querySelector<HTMLInputElement>('[aria-label="How many of the most recent"]')!.value).toBe("12");
  expect(dialog.querySelector("input:invalid")).toBeNull();
  // jsdom does not submit a form from its button: submit it as the button would.
  expect(inBody('[aria-label="Import links"]')?.getAttribute("type")).toBe("submit");
  inBody("form.link-import")!.dispatchEvent(new Event("submit", { cancelable: true }));
  await flushPromises();
  expect(start).toHaveBeenCalledWith({ choice: { which: "unread" } });
  expect(inBody('[aria-label="Import from Pocket"]')).toBeNull();
  // The sidebar shows how far it is, with pause.
  const card = () => inBody('[aria-label="Importing links"]');
  expect(card()?.textContent).toContain("0 of 9");
  await click('[aria-label="Pause importing"]');
  expect(card()?.textContent).toContain("Paused");
  wrapper.unmount();
});

it("sums an import up, retries the failed ones from an archive, and is dismissed", async () => {
  const retry = vi.spyOn(mock, "retryLinkImportArchived");
  const wrapper = await app();
  mock.emitLinkImport({
    stage: "done",
    source: "Pocket",
    found: 12,
    unread: 9,
    already: 1,
    duplicates: 2,
    notWeb: 1,
    of: 11,
    ready: 8,
    partial: 1,
    failed: 2,
  });
  await flushPromises();
  const card = () => inBody('[aria-label="Importing links"]');
  expect(card()?.textContent).toContain("8 ready");
  expect(card()?.textContent).toContain("2 failed");
  await click('[aria-label="Try archived copies of the 2 that failed"]');
  expect(retry).toHaveBeenCalled();
  mock.emitLinkImport({
    stage: "done",
    source: "Pocket",
    found: 12,
    unread: 9,
    already: 1,
    duplicates: 2,
    notWeb: 1,
    of: 11,
    ready: 10,
    partial: 1,
    failed: 0,
  });
  await flushPromises();
  await click('[aria-label="Dismiss the import summary"]');
  expect(card()).toBeNull();
  wrapper.unmount();
});

it("offers an archived copy on a failed article's page", async () => {
  mock.dispose();
  mock = createMockApi("?article=failed");
  backend.api = mock;
  const refetch = vi.spyOn(mock, "refetchArticle");
  useNavigationStore().openItem("book-6");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await click('.item-page [aria-label="Try an archived copy"]');
  expect(refetch).toHaveBeenCalledWith({ bookId: "book-6", source: "archive" });
  wrapper.unmount();
});
