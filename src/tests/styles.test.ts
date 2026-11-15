import { expect, it } from "vitest";
import themeSource from "@/styles/theme.css?raw";

const components = import.meta.glob("@/**/*.vue", { query: "?raw", import: "default", eager: true }) as Record<
  string,
  string
>;
const OLD_TOKENS =
  /var\(--(background|surface|border|text|text-secondary|text-muted|blue|blue-light|highlight|hover|warning|warning-bg|danger)\)/;
const LITERAL = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(|(?<![\w-])(white|black)(?![\w-])/;

it("styles components only with theme tokens", () => {
  expect(Object.keys(components).length).toBeGreaterThan(20);
  for (const [file, source] of Object.entries(components)) {
    const styles = [...source.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join("\n");
    expect(styles, file).not.toMatch(OLD_TOKENS);
    expect(styles, file).not.toMatch(LITERAL);
    expect(source, file).not.toMatch(/Georgia|Inter\b/);
  }
});

it("has no leftover aliases for the old palette", () => {
  expect(themeSource).not.toMatch(/--(surface|blue|text-muted|warning-bg|highlight):/);
});

const sources = import.meta.glob(["@/**/*.ts", "@/**/*.vue"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

it("imports modules by the @/ alias, never by a relative path", () => {
  expect(Object.keys(sources).length).toBeGreaterThan(100);
  for (const [file, source] of Object.entries(sources)) {
    const relative = [
      ...source.matchAll(/(?:from|import|vi\.mock|import\.meta\.glob)\s*\(?\s*["'](\.\.?\/[^"']*)["']/g),
    ]
      .map((m) => m[1])
      // Rust sources read raw by contract tests sit outside src/.
      .filter((path) => !path.includes("/crates/"));
    expect(relative, file).toEqual([]);
  }
});
