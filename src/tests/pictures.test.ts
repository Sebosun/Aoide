/** A book's pictures on the page: between the paragraphs the core places them, their caption
 *  paragraph set as a caption, and opened large on a click. */
import { afterEach, expect, it } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import type { ChapterView, SegmentView } from "@/api/types";
import ChapterText from "@/components/reader/ChapterText.vue";

const sentence = (id: string, text: string): SegmentView => ({
  id,
  words: text.split(" "),
  starts: [],
  speakable: true,
  canSayAgain: true,
  note: false,
  notes: [],
});
const chapter: ChapterView = {
  index: 0,
  title: "The Lamp",
  blocks: [
    { kind: "heading", level: 1, segments: [sentence("0:0:0", "The Lamp")] },
    { kind: "paragraph", level: null, segments: [sentence("0:1:0", "The keeper climbed the stair.")] },
    { kind: "paragraph", level: null, segments: [sentence("0:2:0", "The lamp, lit at dusk.")] },
  ],
  notNarrated: null,
  inserts: [
    {
      type: "picture",
      beforeBlock: 2,
      url: "asset://lamp.png",
      alt: "The great lamp",
      captionBlock: 2,
      caption: null,
    },
    { type: "picture", beforeBlock: 3, url: "asset://end.png", alt: "", captionBlock: null, caption: "The end" },
  ],
};

let wrapper: VueWrapper | null = null;
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

it("draws pictures between the paragraphs, with their captions", () => {
  wrapper = mount(ChapterText, {
    attachTo: document.body,
    props: { chapter, currentSegment: null, currentWord: null, marginNotes: false },
  });
  const order = [...wrapper.element.querySelectorAll(".chapter-text__block, .chapter-text__picture img")].map(
    (e) => e.getAttribute("src") ?? e.textContent?.trim(),
  );
  expect(order).toEqual([
    "The Lamp",
    "The keeper climbed the stair.",
    "asset://lamp.png",
    "The lamp, lit at dusk.",
    "asset://end.png",
  ]);
  expect(wrapper.get('img[src="asset://lamp.png"]').attributes("alt")).toBe("The great lamp");
  // The caption paragraph is still the text (read, selectable), set as a caption.
  expect(wrapper.findAll(".chapter-text__block")[2].classes()).toContain("chapter-text__block--caption");
  expect(wrapper.text()).toContain("The end");
});

it("opens a picture large, and closes it", async () => {
  wrapper = mount(ChapterText, {
    attachTo: document.body,
    props: { chapter, currentSegment: null, currentWord: null, marginNotes: false },
  });
  await wrapper.get('[aria-label="Enlarge picture: The great lamp"]').trigger("click");
  const large = document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Picture"]');
  expect(large?.querySelector("img")?.getAttribute("src")).toBe("asset://lamp.png");
  large!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await wrapper.vm.$nextTick();
  expect(document.body.querySelector('[role="dialog"][aria-label="Picture"]')).toBeNull();
});
