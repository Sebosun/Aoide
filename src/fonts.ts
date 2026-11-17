import type { ReaderFont } from "@/api/types";

/** Each reader font's faces (fonts.css), loaded ahead of use. A face loads on first use, which
 *  is usually the first book opened: its text would be laid out in the fallback font and again
 *  once the font came, part-way through the open. The system's font is there already. */
const FACES: Record<ReaderFont, string[]> = {
  serif: ['400 1em "Newsreader"', 'italic 400 1em "Newsreader"'],
  sans: ['400 1em "Manrope"'],
  legible: ['400 1em "Atkinson Hyperlegible Next"', 'italic 400 1em "Atkinson Hyperlegible Next"'],
  system: [],
};
/** A letter from each of its subsets (fonts.css: Latin, extended Latin): a face loads only for
 *  the text it is asked for. */
const SAMPLE = "aā";

/** The reader font the appearance chose last (theme.ts applyAppearance). */
let chosen: ReaderFont = "serif";
const loading = new Map<ReaderFont, Promise<void>>();

/** Loads a reader font once, `font` or else the one chosen last, and makes `font` the chosen
 *  one; resolves when it is in, or could not be loaded (the fallback is used then), and at once
 *  where the page has no font loading (a test DOM). Only a chosen font is loaded. */
export function readingFont(font: ReaderFont = chosen): Promise<void> {
  chosen = font;
  const fonts = typeof document === "undefined" ? undefined : document.fonts;
  if (typeof fonts?.load !== "function") return Promise.resolve();
  let promise = loading.get(font);
  if (!promise) {
    promise = Promise.all((FACES[font] ?? []).map((face) => fonts.load(face, SAMPLE))).then(
      () => undefined,
      () => undefined,
    );
    loading.set(font, promise);
  }
  return promise;
}
