import type { FetchReason, FetchStage } from "@/api/types";

/** `text` as a link to save, or null when it is not one. The core checks again and decides
 *  (core/articles.rs `parse_link`); this only keeps a typo from becoming an item. */
export function asLink(text: string): string | null {
  const link = text.trim();
  if (!link || /\s/.test(link)) return null;
  try {
    const url = new URL(/^[a-z][a-z0-9+.-]*:/i.test(link) ? link : `https://${link}`);
    return /^https?:$/.test(url.protocol) && url.hostname.includes(".") ? link : null;
  } catch {
    return null;
  }
}

/** Why an article needs the user, after "Needs you · ". */
export const NEEDS: Record<FetchReason, string> = {
  signIn: "sign in to get the rest",
  paywall: "it is behind a paywall",
  check: "the site asks for a check",
  cutShort: "only part of it was saved",
  empty: "no text was found",
};

/** Whether the fetch is still under way. */
export const fetching = (stage: FetchStage) => stage.kind === "queued" || stage.kind === "fetching";

/** The article's site: its name, else the host of its page. */
export function siteOf(article: { siteName: string | null; canonicalUrl: string }): string {
  if (article.siteName) return article.siteName;
  try {
    return new URL(article.canonicalUrl).hostname;
  } catch {
    return article.canonicalUrl;
  }
}
