import type { AoideApi } from "./api/types";
import { useUiStore } from "./stores/ui";

/** Shows each failure the core carried on from ("core://problem") as an error toast. */
export function listenForProblems(api: AoideApi) {
  return api.onProblem(problem => useUiStore().toast(problem.message, "error"));
}
