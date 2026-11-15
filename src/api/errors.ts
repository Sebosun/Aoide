import type { CoreError } from "./contract";

/** A command's failure, as the core reported it: `kind` tells failures apart, and the text is
 *  the core's message (so `String(error)` reads as before). */
export class ApiError extends Error {
  readonly kind: CoreError["kind"];
  constructor(readonly error: CoreError) {
    super(error.message);
    this.name = "ApiError";
    this.kind = error.kind;
  }
  override toString() {
    return this.message;
  }
}

const isCoreError = (value: unknown): value is CoreError =>
  typeof value === "object" &&
  value !== null &&
  typeof (value as CoreError).kind === "string" &&
  typeof (value as CoreError).message === "string";

/** What a rejected `invoke` gave, as an ApiError: the core's own error, or Tauri's text as an internal one. */
export function apiError(value: unknown): ApiError {
  if (value instanceof ApiError) return value;
  if (isCoreError(value)) return new ApiError(value);
  return new ApiError({ kind: "internal", message: value instanceof Error ? value.message : String(value) });
}
