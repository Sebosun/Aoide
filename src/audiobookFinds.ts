import type { AudiobookSuggestion } from "@/api/types";
import { formatBytes } from "@/bytes";
import { formatDuration } from "@/estimate";

/** An audiobook's name: its folder's, or its (first) file's. */
export const nameOf = (c: AudiobookSuggestion) => c.paths[0].split("/").pop() ?? c.paths[0];

/** What it holds and what its files say: "Folder · 15 files · 1.2 GB · 9 h 12 min · Ada Penn". */
export const holds = (c: AudiobookSuggestion) =>
  [
    c.folder ? "Folder" : null,
    `${c.files.length} ${c.files.length === 1 ? "file" : "files"}`,
    formatBytes(c.bytes),
    c.durationMs === null ? null : formatDuration(c.durationMs),
    c.author,
  ]
    .filter(Boolean)
    .join(" · ");
