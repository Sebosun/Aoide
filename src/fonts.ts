/** The reading font's faces (fonts.css), loaded ahead of use. A face loads on first use, which
 *  is usually the first book opened: its text would be laid out in the fallback font and again
 *  once Newsreader came, part-way through the open. */
const READING = ['400 1em "Newsreader"', 'italic 400 1em "Newsreader"'];
/** A letter from each of its subsets (fonts.css: Latin, extended Latin): a face loads only for
 *  the text it is asked for. */
const SAMPLE = "a\u0101";

let loading: Promise<void> | undefined;

/** Loads the reading font once; resolves when it is in, or could not be loaded (the fallback is
 *  used then), and at once where the page has no font loading (a test DOM). */
export function readingFont(): Promise<void> {
  const fonts = typeof document === "undefined" ? undefined : document.fonts;
  if (typeof fonts?.load !== "function") return Promise.resolve();
  return (loading ??= Promise.all(READING.map((face) => fonts.load(face, SAMPLE))).then(
    () => undefined,
    () => undefined,
  ));
}
