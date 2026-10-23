import { call, useMockApi } from "./runtime";
import { AppError } from "./errors";
import { isHttpUrl } from "./util";
import * as mock from "./mock/app";

/** App-level helpers (docs/api/app.md). */
export const APP_CMD = {
  openUrl: "app_open_url",
  reveal: "app_reveal",
  dataDirs: "app_data_dirs",
} as const;

export interface DataDirs {
  data: string;
  logs: string;
}

export interface AppApi {
  /** http/https only; anything else rejects with INVALID_ARGUMENT */
  openUrl(url: string): Promise<void>;
  /** reveal a path inside a registered game folder / app data dir in Finder */
  reveal(path: string): Promise<void>;
  dataDirs(): Promise<DataDirs>;
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realAppApi: AppApi = {
  async openUrl(url) {
    if (!isHttpUrl(url)) throw new AppError("INVALID_ARGUMENT", "only http/https links can be opened");
    await call<void>(APP_CMD.openUrl, { url });
  },
  reveal: (path) => call(APP_CMD.reveal, { path }),
  dataDirs: () => call(APP_CMD.dataDirs),
};

export const appApi: AppApi = useMockApi ? mock.appMock : realAppApi;
