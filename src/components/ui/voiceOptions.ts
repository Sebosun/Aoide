import type { Voice } from "../../api/types";
import type { SelectOption } from "./types";

/** Options for a voice picker: the default first (value ""), then a saved voice the server no
 *  longer lists (so it stays visible), then the server's voices. */
export function voiceOptions(voices: Voice[], current: string | null | undefined, defaultLabel: string): SelectOption[] {
  return [
    { value: "", label: defaultLabel },
    ...(current && !voices.some(v => v.id === current) ? [{ value: current, label: current, hint: "not on the server" }] : []),
    ...voices.map(voice => ({ value: voice.id, label: voice.displayName })),
  ];
}
