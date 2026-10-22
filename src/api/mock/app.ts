import type { AppApi } from "../app";
import { isHttpUrl } from "../util";
import { AppError } from "../errors";

export const appMock: AppApi = {
  async openUrl(url) {
    if (!isHttpUrl(url)) throw new AppError("INVALID_ARGUMENT", "only http/https links can be opened");
    if (typeof window !== "undefined") window.open(url, "_blank", "noopener,noreferrer");
  },
  async reveal() {},
  async dataDirs() {
    return { data: "~/Library/Application Support/com.snowingfox.pcltauriport", logs: "~/Library/Application Support/com.snowingfox.pcltauriport/logs" };
  },
};
