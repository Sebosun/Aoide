import { beforeEach, afterEach, expect, it, onTestFinished, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { mount, flushPromises } from "@vue/test-utils";
import { createMockApi } from "@/api/mock";
import App from "@/App.vue";
import { useNavigationStore } from "@/stores/navigation";
import { useReaderStore } from "@/stores/reader";
import { useFeedsStore } from "@/stores/feeds";
import { pickMenuItem } from "@/tests/helpers";

const backend = vi.hoisted(() => ({ api: null as unknown }));
vi.mock("@/api", () => ({
  get api() {
    return backend.api;
  },
}));
let mock: ReturnType<typeof createMockApi>;
const start = (search = "") => {
  mock = createMockApi(search);
  backend.api = mock;
};
beforeEach(() => {
  setActivePinia(createPinia());
  localStorage.clear();
  start();
});
afterEach(() => {
  vi.useRealTimers();
  mock.dispose();
  document.body.innerHTML = "";
});

async function openFeeds() {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').trigger("click");
  await flushPromises();
  return wrapper;
}
const subscriptions = (wrapper: ReturnType<typeof mount>) =>
  wrapper.findAll('[aria-label="Subscriptions"] .feeds__line').map((e) =>
    e
      .findAll("span")
      .map((part) => part.text())
      .join(" "),
  );
const titles = (wrapper: ReturnType<typeof mount>) => wrapper.findAll(".feed-entry__title").map((e) => e.text());
const entry = (wrapper: ReturnType<typeof mount>, title: string) =>
  wrapper.findAll(".feed-entry").find((e) => e.find(".feed-entry__title").text() === title)!;

it("shows the unread count beside Feeds and lists subscriptions and entries by day", async () => {
  // Midday, so entries from minutes ago are today whenever the test runs.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 10, 15, 12));
  start();
  const wrapper = await openFeeds();
  expect(wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').text()).toBe("Feeds5");
  expect(wrapper.get(".feeds__summary").text()).toBe("5 new · checked 20 minutes ago");
  expect(subscriptions(wrapper)).toEqual([
    "All new 5",
    "Saved for later",
    "Sea 2",
    "Harbour Letters",
    "Tidewatch 2",
    "Field Notes 1",
    "Small Cities 2",
  ]);
  expect(wrapper.findAll(".feeds__day").map((d) => d.text())[0]).toBe("Today");
  expect(titles(wrapper).slice(0, 3)).toEqual([
    "The ferry that runs on the tide table",
    "Who decides where the bus shelter goes",
    "The quiet return of the night train",
  ]);
  const ferry = entry(wrapper, "The ferry that runs on the tide table");
  expect(ferry.get(".feed-entry__meta").text()).toBe("Tidewatch · 2 hours ago · 1 min read");
  expect(ferry.find('[aria-label="Unread"]').exists()).toBe(true);
  const shelter = entry(wrapper, "Who decides where the bus shelter goes");
  expect(shelter.get(".feed-entry__meta").text()).toBe("Small Cities · 5 hours ago · summary only");
  expect(shelter.text()).toContain("The full article is fetched when you open it.");
});

it("shows one subscription's entries when it is chosen", async () => {
  const wrapper = await openFeeds();
  await wrapper
    .findAll(".feeds__source")
    .find((s) => s.text().startsWith("Small Cities"))!
    .trigger("click");
  await flushPromises();
  expect(titles(wrapper)).toEqual(["Who decides where the bus shelter goes", "The last public clock in town"]);
  expect(wrapper.get(".feeds__source[aria-current='true']").text()).toContain("Small Cities");
});

it("opens an entry in the reader and it is read after", async () => {
  const wrapper = await openFeeds();
  await entry(wrapper, "The ferry that runs on the tide table").get(".feed-entry__open").trigger("click");
  await flushPromises();
  expect(useNavigationStore().view).toBe("reader");
  expect(useReaderStore().book).not.toBeNull();
  useNavigationStore().leaveReader();
  await flushPromises();
  expect(entry(wrapper, "The ferry that runs on the tide table").find('[aria-label="Unread"]').exists()).toBe(false);
  expect(wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').text()).toBe("Feeds4");
});

it("saves an entry to the library and takes it out again", async () => {
  const wrapper = await openFeeds();
  const save = () => entry(wrapper, "The quiet return of the night train").get(".feed-entry__save");
  expect(save().attributes("aria-pressed")).toBe("false");
  await save().trigger("click");
  await flushPromises();
  expect(save().attributes("aria-pressed")).toBe("true");
  expect(save().attributes("aria-label")).toBe("Saved to library");
  expect(entry(wrapper, "The quiet return of the night train").get(".feed-entry__meta").text()).toContain(
    "in your library",
  );
  expect((await mock.listBooks()).some((b) => b.kind === "article")).toBe(true);
  await save().trigger("click");
  await flushPromises();
  expect(save().attributes("aria-pressed")).toBe("false");
  expect((await mock.listBooks()).some((b) => b.kind === "article")).toBe(false);
});

it("saves a feed's entries into the collection chosen in its menu", async () => {
  const harbour = await mock.createCollection({ name: "Harbour" });
  const choose = vi.spyOn(mock, "setFeedCollection");
  const wrapper = await openFeeds();
  await wrapper.get('[aria-label="More for Field Notes"]').trigger("click");
  await flushPromises();
  const row = (name: string) =>
    document.body.querySelector<HTMLElement>(`[role="menu"] [role="menuitemcheckbox"][aria-label="${name}"]`)!;
  expect(row("No collection").getAttribute("aria-checked")).toBe("true");
  row("Harbour").click();
  await flushPromises();
  expect(choose).toHaveBeenCalledWith({ feedId: 3, collectionId: harbour });

  await entry(wrapper, "The quiet return of the night train").get(".feed-entry__save").trigger("click");
  await flushPromises();
  const saved = (await mock.listBooks()).find((b) => b.kind === "article")!;
  expect(saved.collections).toEqual([harbour]);

  await wrapper.get('[aria-label="More for Field Notes"]').trigger("click");
  await flushPromises();
  expect(row("Harbour").getAttribute("aria-checked")).toBe("true");
  row("No collection").click();
  await flushPromises();
  expect(choose).toHaveBeenLastCalledWith({ feedId: 3, collectionId: null });
});

it("leaves a feed with no collection once its collection is deleted", async () => {
  const harbour = await mock.createCollection({ name: "Harbour" });
  await mock.setFeedCollection({ feedId: 3, collectionId: harbour });
  const wrapper = await openFeeds();
  await mock.deleteCollection({ id: harbour });
  await flushPromises();
  await wrapper.get('[aria-label="More for Field Notes"]').trigger("click");
  await flushPromises();
  const row = document.body.querySelector('[role="menu"] [role="menuitemcheckbox"][aria-label="No collection"]')!;
  expect(row.getAttribute("aria-checked")).toBe("true");
  expect((await mock.feeds()).find((f) => f.id === 3)!.collectionId).toBeNull();
});

it("says since when a feed is not reachable", async () => {
  const wrapper = await openFeeds();
  const harbour = wrapper.findAll(".feeds__source").find((s) => s.text().startsWith("Harbour Letters"))!;
  expect(harbour.get(".feeds__failing").text()).toMatch(/^Not reachable since \w+/);
});

it("loads older entries at the end of the list", async () => {
  mock.addFeedEntriesForTest(60);
  const wrapper = await openFeeds();
  expect(titles(wrapper)).toHaveLength(50);
  await wrapper.get(".feeds__more").trigger("click");
  await flushPromises();
  expect(titles(wrapper)).toHaveLength(67);
  expect(wrapper.find(".feeds__more").exists()).toBe(false);
});

it("loads each older page once, and never into another source's list", async () => {
  mock.addFeedEntriesForTest(60);
  const wrapper = await openFeeds();
  const feeds = useFeedsStore();
  // The end of the list seen and the button pressed together.
  await Promise.all([feeds.loadMore(), feeds.loadMore()]);
  await flushPromises();
  expect(titles(wrapper)).toHaveLength(67);
  expect(new Set(feeds.entries.map((e) => e.id)).size).toBe(67);

  // A page still loading when another source is chosen is dropped.
  await feeds.choose({ kind: "all" });
  const older = feeds.loadMore();
  await feeds.choose({ kind: "saved" });
  await older;
  await flushPromises();
  expect(feeds.entries.every((e) => e.inLibrary)).toBe(true);
});

it("invites a first subscription when there is none", async () => {
  start("?feeds=empty");
  const wrapper = await openFeeds();
  expect(wrapper.get(".feeds__empty").text()).toContain("No subscriptions yet");
  await wrapper.get(".feeds__empty button").trigger("click");
  expect(useNavigationStore().addOpen).toBe(true);
  useNavigationStore().closeAdd();
  await wrapper.get(".feeds__empty .feeds__link").trigger("click");
  await flushPromises();
  expect(subscriptions(wrapper).slice(2)).toEqual(["Lantern Notes", "Quay Weekly", "Tidewatch"]);
});

it("says when nothing is new", async () => {
  const wrapper = await openFeeds();
  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Mark all read")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.get('nav[aria-label="Main"] [aria-label="Feeds"]').text()).toBe("Feeds");
  expect(wrapper.get(".feeds__summary").text()).toBe("Nothing new · checked 20 minutes ago");
  await wrapper
    .findAll(".feeds__source")
    .find((s) => s.text() === "Saved for later")!
    .trigger("click");
  await flushPromises();
  expect(wrapper.get(".feeds__empty").text()).toContain("Nothing saved yet");
});

const reader = (wrapper: ReturnType<typeof mount>) => wrapper.get('[aria-label="About this article"]');

it("reads an entry opened from a feed with Save to library, then says it is in the library", async () => {
  const wrapper = await openFeeds();
  await entry(wrapper, "The ferry that runs on the tide table").get(".feed-entry__open").trigger("click");
  await flushPromises();
  expect((await mock.listBooks()).some((b) => b.kind === "article")).toBe(false);
  await reader(wrapper).get("button.article-header__save").trigger("click");
  await flushPromises();
  expect(reader(wrapper).find("button.article-header__save").exists()).toBe(false);
  expect(reader(wrapper).get(".article-header__kept").text()).toBe("In your library");
  expect((await mock.listBooks()).some((b) => b.kind === "article")).toBe(true);
});

it("puts an entry on a collection with C only once it is saved to the library", async () => {
  const wrapper = await openFeeds();
  await entry(wrapper, "The ferry that runs on the tide table").get(".feed-entry__open").trigger("click");
  await flushPromises();
  const picker = () => document.body.querySelector('[role="dialog"][aria-label^="Collections for"]');
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "c" }));
  await flushPromises();
  expect(picker()).toBeNull();
  await reader(wrapper).get("button.article-header__save").trigger("click");
  await flushPromises();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "c" }));
  await flushPromises();
  const saved = (await mock.listBooks()).find((b) => b.kind === "article")!;
  expect(picker()?.getAttribute("aria-label")).toBe(`Collections for ${saved.title}`);
});

const addDialog = () => document.body.querySelector<HTMLElement>('[role="dialog"][aria-label="Add to your library"]');
async function submitLink(wrapper: ReturnType<typeof mount>, link: string) {
  const field = wrapper.get<HTMLInputElement>('[role="dialog"] input[type="url"]');
  await field.setValue(link);
  await wrapper.get('[role="dialog"] form').trigger("submit");
  await flushPromises();
}

it("subscribes when the link given to Add is a feed, and shows it in Feeds", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  useNavigationStore().openAdd();
  await flushPromises();
  expect(addDialog()!.textContent).toContain("A link to an article or a feed");
  await submitLink(wrapper, "https://lanterns.example/feed.xml");
  expect(addDialog()).toBeNull();
  expect(useNavigationStore().view).toBe("feeds");
  expect(wrapper.get(".feeds__source[aria-current='true']").text()).toContain("Lantern Notes");
});

it("pastes a link into Add from the clipboard, and says to use Ctrl V where it cannot read it", async () => {
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  useNavigationStore().openAdd();
  await flushPromises();
  const readText = vi.fn().mockResolvedValueOnce("  https://lanterns.example/feed.xml \n");
  Object.defineProperty(navigator, "clipboard", { value: { readText }, configurable: true });
  onTestFinished(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });
  const field = wrapper.get<HTMLInputElement>('[role="dialog"] input[type="url"]');
  await wrapper.get('[aria-label="Paste link"]').trigger("click");
  await flushPromises();
  expect(field.element.value).toBe("https://lanterns.example/feed.xml");
  expect(document.activeElement).toBe(field.element);

  readText.mockRejectedValueOnce(new Error("not allowed"));
  await field.setValue("");
  await wrapper.get('[aria-label="Paste link"]').trigger("click");
  await flushPromises();
  expect(field.element.value).toBe("");
  expect(addDialog()!.textContent).toContain("Press Ctrl V to paste");
  expect(document.activeElement).toBe(field.element);
  wrapper.unmount();
});

it("offers a site's feeds to choose from when subscribing to the site", async () => {
  const wrapper = await openFeeds();
  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Subscribe")!
    .trigger("click");
  await flushPromises();
  expect(addDialog()!.textContent).toContain("A feed, or a site that has one");
  await submitLink(wrapper, "https://lanterns.example/");
  const choices = wrapper.findAll('[role="dialog"] .add-dialog__choice');
  expect(choices.map((c) => c.get(".add-dialog__choice-title").text())).toEqual(["Articles", "Comments"]);
  await choices[0].get("button").trigger("click");
  await flushPromises();
  expect(addDialog()).toBeNull();
  expect(wrapper.get(".feeds__source[aria-current='true']").text()).toContain("Lantern Notes");

  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Subscribe")!
    .trigger("click");
  await submitLink(wrapper, "https://lanterns.example/about");
  expect(wrapper.get('[role="dialog"] [role="alert"]').text()).toBe("No feed was found at that address.");
});

it("offers the feed of an article's site in the reader", async () => {
  start("?article=ready");
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await useReaderStore().open("book-6");
  await flushPromises();
  const offer = wrapper.get(".feed-offer");
  expect(offer.text()).toContain("This site has a feed: Quay Weekly");
  await offer.get("button").trigger("click");
  await flushPromises();
  expect(wrapper.find(".feed-offer").exists()).toBe(false);
  expect((await mock.feeds()).some((f) => f.url === "https://quay.example/feed.xml")).toBe(true);
});

it("fetches the page of a summary-only entry, then reads it", async () => {
  const wrapper = await openFeeds();
  await entry(wrapper, "Who decides where the bus shelter goes").get(".feed-entry__open").trigger("click");
  await flushPromises();
  expect(entry(wrapper, "Who decides where the bus shelter goes").get(".feed-entry__note").text()).toBe(
    "Fetching the article…",
  );
  expect(useNavigationStore().view).toBe("feeds");
  await vi.waitFor(() => expect(useNavigationStore().view).toBe("reader"), { timeout: 3000 });
  expect(useReaderStore().error).toBe("");
});

it("marks an entry whose article needs the user", async () => {
  start("?article=needsyou-signin");
  const wrapper = await openFeeds();
  const night = () => entry(wrapper, "The quiet return of the night train");
  expect(night().find(".feed-entry__note--needs").exists()).toBe(false);
  // Saving it links the entry to the saved article, which needs a sign-in.
  await night().get(".feed-entry__save").trigger("click");
  await flushPromises();
  expect(night().get(".feed-entry__note--needs").text()).toBe("Needs you · sign in to get the rest");
});

const folderRow = (wrapper: ReturnType<typeof mount>, name: string) =>
  wrapper.findAll(".feeds__row--folder").find((r) => r.find(".feeds__name").text() === name)!;

it("lists a folder's feeds under it and reads the folder as one stream", async () => {
  const wrapper = await openFeeds();
  const sea = folderRow(wrapper, "Sea");
  await sea.get(".feeds__source").trigger("click");
  await flushPromises();
  expect(titles(wrapper)).toEqual([
    "The ferry that runs on the tide table",
    "How lighthouses kept their own time",
    "Charts drawn by the harbour pilots",
    "Letters from the breakwater",
  ]);
  await wrapper
    .findAll("button")
    .find((b) => b.text() === "Mark all read")!
    .trigger("click");
  await flushPromises();
  expect(subscriptions(wrapper)).toEqual([
    "All new 3",
    "Saved for later",
    "Sea",
    "Harbour Letters",
    "Tidewatch",
    "Field Notes 1",
    "Small Cities 2",
  ]);
});

it("folds a folder away and remembers it", async () => {
  let wrapper = await openFeeds();
  const toggle = () => folderRow(wrapper, "Sea").get('[aria-label="Show or hide Sea"]');
  expect(toggle().attributes("aria-expanded")).toBe("true");
  await toggle().trigger("click");
  expect(toggle().attributes("aria-expanded")).toBe("false");
  expect(subscriptions(wrapper)).not.toContain("Tidewatch 2");
  wrapper.unmount();
  setActivePinia(createPinia());
  wrapper = await openFeeds();
  expect(toggle().attributes("aria-expanded")).toBe("false");
  await toggle().trigger("click");
  expect(subscriptions(wrapper)).toContain("Tidewatch 2");
});

it("moves a feed to a folder from its menu, to a new one by name, and out again", async () => {
  const move = vi.spyOn(mock, "setFeedFolder");
  const wrapper = await openFeeds();
  const row = (name: string) =>
    document.body.querySelector<HTMLElement>(`[role="menu"] [role="menuitemcheckbox"][aria-label="${name}"]`)!;
  await wrapper.get('[aria-label="More for Field Notes"]').trigger("click");
  await flushPromises();
  expect(row("No folder").getAttribute("aria-checked")).toBe("true");
  row("Sea").click();
  await flushPromises();
  expect(move).toHaveBeenLastCalledWith({ feedId: 3, folder: "Sea" });
  expect(subscriptions(wrapper).slice(2, 6)).toEqual(["Sea 3", "Field Notes 1", "Harbour Letters", "Tidewatch 2"]);

  await wrapper.get('[aria-label="More for Small Cities"]').trigger("click");
  await flushPromises();
  await pickMenuItem("New folder…");
  const dialog = wrapper.get('[role="dialog"]');
  await dialog.get("input").setValue("Towns");
  await dialog.get("form").trigger("submit");
  await flushPromises();
  expect(move).toHaveBeenLastCalledWith({ feedId: 2, folder: "Towns" });
  expect(folderRow(wrapper, "Towns").text()).toContain("2");

  await wrapper.get('[aria-label="More for Small Cities"]').trigger("click");
  await flushPromises();
  row("No folder").click();
  await flushPromises();
  expect(move).toHaveBeenLastCalledWith({ feedId: 2, folder: null });
  expect(wrapper.findAll(".feeds__row--folder").map((r) => r.find(".feeds__name").text())).toEqual(["Sea"]);
});

it("renames a folder from its menu, and the chosen folder follows", async () => {
  const wrapper = await openFeeds();
  await folderRow(wrapper, "Sea").get(".feeds__source").trigger("click");
  await flushPromises();
  await wrapper.get('[aria-label="More for folder Sea"]').trigger("click");
  await flushPromises();
  await pickMenuItem("Rename…");
  const dialog = wrapper.get('[role="dialog"]');
  expect((dialog.get("input").element as HTMLInputElement).value).toBe("Sea");
  await dialog.get("input").setValue("Coast");
  await dialog.get("form").trigger("submit");
  await flushPromises();
  expect(folderRow(wrapper, "Coast").get(".feeds__source").attributes("aria-current")).toBe("true");
  expect(titles(wrapper)).toHaveLength(4);
});

it("moves a feed into a folder when it is dropped on it", async () => {
  const move = vi.spyOn(mock, "setFeedFolder");
  const wrapper = await openFeeds();
  const data = new Map<string, string>();
  const transfer = {
    get types() {
      return [...data.keys()];
    },
    setData: (type: string, value: string) => data.set(type, value),
    getData: (type: string) => data.get(type) ?? "",
    effectAllowed: "",
    dropEffect: "",
  };
  const drag = (target: Element, type: string) => {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(event, "dataTransfer", { value: transfer });
    target.dispatchEvent(event);
    return event;
  };
  const fieldNotes = wrapper.findAll(".feeds__row").find((r) => r.text().startsWith("Field Notes"))!;
  expect(fieldNotes.attributes("draggable")).toBe("true");
  expect(drag(fieldNotes.element, "dragstart").defaultPrevented).toBe(false);
  const sea = folderRow(wrapper, "Sea");
  expect(drag(sea.element, "dragover").defaultPrevented, "a folder takes the drop").toBe(true);
  drag(sea.element, "drop");
  await flushPromises();
  expect(move).toHaveBeenCalledWith({ feedId: 3, folder: "Sea" });
});
