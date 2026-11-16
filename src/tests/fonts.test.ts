import { afterEach, expect, it, vi } from "vitest";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

const withFonts = (load: (font: string) => Promise<unknown>) =>
  vi.stubGlobal("document", Object.assign(Object.create(document), { fonts: { load: vi.fn(load) } }));

// The reading font loads on first use, usually the first book opened: its text was laid out in the
// fallback font and again when Newsreader came, part-way through the open.
it("loads the reading font's faces once, for everyone who waits on it", async () => {
  withFonts(async () => []);
  const { readingFont } = await import("@/fonts");
  await Promise.all([readingFont(), readingFont()]);
  const load = document.fonts.load as unknown as ReturnType<typeof vi.fn>;
  expect(load.mock.calls.map(([font]) => font)).toEqual(['400 1em "Newsreader"', 'italic 400 1em "Newsreader"']);
  // Both subsets: Latin and extended Latin (Czech, Polish…).
  expect(load.mock.calls.every(([, text]) => /a/.test(text) && /\u0101/.test(text))).toBe(true);
});

it("lets the reader go on when the font cannot load", async () => {
  withFonts(async () => {
    throw new Error("no font");
  });
  const { readingFont } = await import("@/fonts");
  await expect(readingFont()).resolves.toBeUndefined();
});
