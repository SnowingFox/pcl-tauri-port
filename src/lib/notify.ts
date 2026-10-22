import { describeError, normalizeError } from "../api/errors";
import { useUi } from "../store/ui";

/**
 * Turn any thrown value (AppError, backend string, Error) into a readable toast.
 * Cancellations are expected user actions and stay silent. Errors that have a remedy get a button.
 */
export function reportError(e: unknown, prefix?: string): void {
  const err = normalizeError(e);
  if (err.code === "CANCELLED") return;
  const v = describeError(err);
  useUi.getState().toast(prefix ? `${prefix}：${v.text}` : v.text, "error", v.action ? 9000 : 6000, v.action ? { label: v.action.label, route: v.action.route } : undefined);
}
