import { afterEach, expect, it, onTestFinished, vi } from "vitest";

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

// A font is loaded only when chosen, and the reader waits for the one it is set in.
it("loads only the chosen reader font, and the reader waits for that one", async () => {
  withFonts(async () => []);
  const { readingFont } = await import("@/fonts");
  const load = document.fonts.load as unknown as ReturnType<typeof vi.fn>;
  await readingFont("legible");
  await readingFont();
  expect(load.mock.calls.map(([font]) => font)).toEqual([
    '400 1em "Atkinson Hyperlegible Next"',
    'italic 400 1em "Atkinson Hyperlegible Next"',
  ]);
  // The system's font is the system's: nothing to load.
  load.mockClear();
  await readingFont("system");
  await readingFont();
  expect(load).not.toHaveBeenCalled();
});

it("starts loading the reader font when the appearance chooses it", async () => {
  const load = vi.fn(async (_font: string, _text: string) => []);
  Object.defineProperty(document, "fonts", { value: { load }, configurable: true });
  onTestFinished(() => {
    delete (document as { fonts?: unknown }).fonts;
  });
  const { applyAppearance } = await import("@/theme");
  const { readingFont } = await import("@/fonts");
  applyAppearance(
    {
      mode: "auto",
      theme: "default",
      textSize: 2,
      readerMode: "page",
      zoom: 1,
      timeLeft: "chapter",
      readerFont: "sans",
      lineSpacing: 2,
      textWidth: 2,
      justify: false,
      hyphenate: false,
    },
    [],
  );
  await readingFont();
  expect(load.mock.calls.map(([font]) => font)).toEqual(['400 1em "Manrope"']);
});
