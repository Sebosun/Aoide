import type { NoteMarker, SegmentView } from "@/api/types";

/** Names one footnote marker of a sentence: which popover is open, which margin note it is. */
export const noteKey = (segment: SegmentView, note: NoteMarker) => `${segment.id}:${note.afterWord}:${note.label}`;
/** The element id of a footnote's margin note. */
export const noteId = (segment: SegmentView, note: NoteMarker) =>
  `note-${noteKey(segment, note).replace(/[^\w-]/g, "-")}`;
