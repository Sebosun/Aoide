import { afterEach, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import { mount } from "@vue/test-utils";
import { useMediaQuery } from "@/composables/useMediaQuery";

afterEach(() => vi.unstubAllGlobals());

// The reader lays out its margins from the first render: learning the width only once mounted
// drew the whole chapter twice on every open.
it("knows whether the query matches from the first render", () => {
  vi.stubGlobal("matchMedia", (media: string) => ({
    matches: true,
    media,
    addEventListener() {},
    removeEventListener() {},
  }));
  const rendered: boolean[] = [];
  const Probe = defineComponent(() => {
    const wide = useMediaQuery("(min-width: 1000px)");
    return () => {
      rendered.push(wide.value);
      return h("i");
    };
  });
  mount(Probe);
  expect(rendered).toEqual([true]);
});
