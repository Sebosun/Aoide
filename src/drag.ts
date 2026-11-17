/** What a library card carries while it is dragged (its item's id), for the sidebar's
 *  collections to take. A type of its own, so files, links and text dropped on the window are
 *  never mistaken for an item. */
export const ITEM_DRAG = "application/x-aoide-item";

/** What a queued item carries while it is dragged to another place in the player's queue (its
 *  item's id). */
export const QUEUE_DRAG = "application/x-aoide-queued";

/** What a feed in the Feeds screen's list carries while it is dragged onto a folder (its id). */
export const FEED_DRAG = "application/x-aoide-feed";
