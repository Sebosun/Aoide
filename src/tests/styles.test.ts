import { expect, it } from "vitest";
import themeSource from "../styles/theme.css?raw";

const components = import.meta.glob("../**/*.vue", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const OLD_TOKENS = /var\(--(background|surface|border|text|text-secondary|text-muted|blue|blue-light|highlight|hover|warning|warning-bg|danger)\)/;
const LITERAL = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(|(?<![\w-])(white|black)(?![\w-])/;

it("styles components only with theme tokens", () => {
  expect(Object.keys(components).length).toBeGreaterThan(20);
  for (const [file, source] of Object.entries(components)) {
    const styles = [...source.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map(m => m[1]).join("\n");
    expect(styles, file).not.toMatch(OLD_TOKENS);
    expect(styles, file).not.toMatch(LITERAL);
    expect(source, file).not.toMatch(/Georgia|Inter\b/);
  }
});

it("has no leftover aliases for the old palette", () => {
  expect(themeSource).not.toMatch(/--(surface|blue|text-muted|warning-bg|highlight):/);
});
