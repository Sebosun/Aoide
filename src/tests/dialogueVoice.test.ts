import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import BookVoice from "@/components/book/BookVoice.vue";
import { choose } from "@/tests/helpers";

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
  globalThis.URL.createObjectURL ??= () => "blob:sample";
});
afterEach(() => {
  mock.dispose();
  document.body.innerHTML = "";
});

async function voiceTab() {
  const wrapper = mount(BookVoice, { props: { bookId: "book-0" }, attachTo: document.body });
  await flushPromises();
  return wrapper;
}

it("chooses a second voice for dialogue, previews both and saves it", async () => {
  const preview = vi.spyOn(mock, "previewDialogue");
  const wrapper = await voiceTab();
  expect(wrapper.text()).not.toContain("What characters say");
  await choose(wrapper, "Voice for dialogue", "Adrian");
  expect(wrapper.text()).toContain("What characters say in quotation marks is read in this voice");
  expect(wrapper.find(".book-voice__warning").exists(), "Chatterbox switches voices freely").toBe(false);
  await wrapper.get('[aria-label="Preview both voices"]').trigger("click");
  await flushPromises();
  expect(preview).toHaveBeenCalledWith(expect.objectContaining({ bookId: "book-0", dialogueVoiceId: "Adrian.wav" }));
  await wrapper.get("form").trigger("submit");
  await flushPromises();
  expect((await mock.bookSettings({ bookId: "book-0" })).dialogueVoiceId).toBe("Adrian.wav");
  wrapper.unmount();
});

it("warns when the engine may be slow to switch voices", async () => {
  const options = await mock.engineOptions();
  vi.spyOn(mock, "engineOptions").mockResolvedValue({
    ...options,
    descriptors: options.descriptors.map((d) => ({ ...d, cheapVoiceSwitch: false })),
  });
  await mock.saveBookSettings({
    bookId: "book-0",
    settings: { ...(await mock.bookSettings({ bookId: "book-0" })), dialogueVoiceId: "Adrian.wav" },
  });
  const wrapper = await voiceTab();
  expect(wrapper.get(".book-voice__warning").text()).toContain("may load each voice anew");
  wrapper.unmount();
});
