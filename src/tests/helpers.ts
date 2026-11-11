import { flushPromises } from "@vue/test-utils";

type Findable = { get(selector: string): { trigger(event: string): Promise<void>; attributes(name: string): string | undefined } };

/** Picks `option` from the UiSelect labelled `label` inside `root`, the way a person would:
 *  open it, then click the option. Listboxes are teleported to the body. */
export async function choose(root: Findable, label: string, option: string) {
  const button = root.get(`[aria-label="${label}"][aria-haspopup="listbox"]`);
  if (button.attributes("aria-expanded") !== "true") await button.trigger("click");
  await flushPromises();
  const match = [...document.body.querySelectorAll<HTMLElement>('[role="listbox"] [role="option"]')].find(o => o.textContent?.trim() === option);
  if (!match) throw new Error(`No option "${option}" in ${label}: ${[...document.body.querySelectorAll('[role="option"]')].map(o => o.textContent?.trim()).join(", ")}`);
  match.click();
  await flushPromises();
}

/** The open listbox's option labels. */
export const optionLabels = () => [...document.body.querySelectorAll<HTMLElement>('[role="listbox"] [role="option"]')].map(o => o.textContent?.trim());

/** The open menu (menus are teleported to the body). */
export const openMenu = () => document.body.querySelector<HTMLElement>('[role="menu"]');

/** Clicks the item with this accessible name in the open menu. */
export async function pickMenuItem(label: string) {
  const items = [...document.body.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')];
  const match = items.find(item => item.getAttribute("aria-label") === label) ?? items.find(item => item.textContent?.trim() === label);
  if (!match) throw new Error(`No menu item "${label}" in ${items.map(item => item.getAttribute("aria-label") ?? item.textContent?.trim()).join(", ")}`);
  match.click();
  await flushPromises();
}
