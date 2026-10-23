/** Error codes returned by the backend `app_*` commands, plus codes derived from legacy string errors. */
export type ErrorCode =
  | "JAVA_MISSING" | "ACCOUNT_NOT_FOUND" | "ACCOUNT_EXPIRED" | "INSTANCE_NOT_FOUND" | "FILES_MISSING"
  | "PRECHECK_FAILED" | "DOWNLOAD_FAILED" | "LAUNCH_FAILED" | "NETWORK" | "INVALID_ARGUMENT"
  | "FORBIDDEN_PATH" | "UNSUPPORTED" | "IO" | "INTERNAL"
  | "CANCELLED" | "DEVICE_CODE_EXPIRED" | "DECLINED" | "NOT_OWNED" | "MS_CLIENT_ID_MISSING"
  | "CURSEFORGE_KEY_MISSING" | "UNKNOWN";

export class AppError extends Error {
  constructor(public code: ErrorCode | string, message: string, public details?: unknown) {
    super(message);
    this.name = "AppError";
  }
}

/** Legacy modules reject with plain strings; map the well-known ones to codes. */
const PATTERNS: [RegExp, ErrorCode][] = [
  [/login was cancelled|^cancelled$/i, "CANCELLED"],
  [/device code expired/i, "DEVICE_CODE_EXPIRED"],
  [/authorization was declined/i, "DECLINED"],
  [/does not own Minecraft/i, "NOT_OWNED"],
  [/client id is not configured/i, "MS_CLIENT_ID_MISSING"],
  [/login expired|sign in again/i, "ACCOUNT_EXPIRED"],
  [/PCL_CURSEFORGE_KEY/i, "CURSEFORGE_KEY_MISSING"],
  [/^(network error|http:)|bad status|error sending request|timed out/i, "NETWORK"],
];

export function normalizeError(e: unknown): AppError {
  if (e instanceof AppError) return e;
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) {
    const o = e as { code: unknown; message: unknown; details?: unknown };
    return new AppError(String(o.code), String(o.message), o.details);
  }
  const msg = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  const hit = PATTERNS.find(([re]) => re.test(msg));
  return new AppError(hit ? hit[1] : "UNKNOWN", msg);
}

export interface ErrorView {
  text: string;
  /** optional follow-up, e.g. go to Java settings */
  action?: { label: string; route: string };
}

interface PrecheckIssueLike { kind: string; [k: string]: unknown }

function issueText(i: PrecheckIssueLike): string {
  switch (i.kind) {
    case "version_json_missing": return "缺少版本描述文件";
    case "client_jar_missing": return "缺少游戏主程序";
    case "asset_index_missing": return "缺少资源索引";
    case "libraries_missing": return `缺少 ${String(i.count ?? "")} 个依赖库`;
    case "assets_missing": return `缺少 ${String(i.count ?? "")} 个资源文件`;
    case "no_java": return `没有可用的 Java（需要 Java ${String(i.required_major ?? "")}）`;
    case "invalid_memory": return "内存设置无效";
    case "empty_player_name": return "玩家名为空";
    default: return i.kind;
  }
}

function issuesOf(details: unknown): PrecheckIssueLike[] {
  const issues = (details as { issues?: unknown } | undefined)?.issues;
  return Array.isArray(issues) ? (issues as PrecheckIssueLike[]) : [];
}

export function describeError(e: unknown): ErrorView {
  const err = normalizeError(e);
  const d = err.details as Record<string, unknown> | undefined;
  const withMsg = (base: string) => (err.message && err.message !== base ? `${base}（${err.message}）` : base);
  switch (err.code) {
    case "JAVA_MISSING":
      return {
        text: d?.requiredMajor ? `未找到可用的 Java，此版本需要 Java ${String(d.requiredMajor)}` : "未找到可用的 Java",
        action: { label: "前往 Java 设置", route: "/settings/java" },
      };
    case "ACCOUNT_NOT_FOUND": return { text: "找不到所选账号，请重新选择或添加账号" };
    case "ACCOUNT_EXPIRED":
      return { text: "账号登录已过期，请重新登录", action: { label: "前往登录", route: "/launch" } };
    case "INSTANCE_NOT_FOUND": return { text: "找不到该游戏实例，可能已被移动或删除" };
    case "FILES_MISSING": {
      const list = issuesOf(d).map(issueText).join("、");
      return {
        text: `游戏文件不完整${list ? `：${list}` : ""}，请重新安装该版本`,
        action: { label: "前往下载", route: "/download/game" },
      };
    }
    case "PRECHECK_FAILED": {
      const list = issuesOf(d).map(issueText).join("、");
      const memory = issuesOf(d).some((i) => i.kind === "invalid_memory" || i.kind === "no_java");
      return {
        text: `启动前检查未通过${list ? `：${list}` : ""}`,
        action: memory ? { label: "前往 Java 设置", route: "/settings/java" } : undefined,
      };
    }
    case "DOWNLOAD_FAILED": return { text: withMsg("下载失败，请检查网络后重试") };
    case "LAUNCH_FAILED": return { text: withMsg("游戏进程启动失败") };
    case "NETWORK": return { text: "网络请求失败，请检查网络连接后重试" };
    case "INVALID_ARGUMENT": return { text: withMsg("参数无效") };
    case "FORBIDDEN_PATH": return { text: "该路径不在已登记的游戏文件夹内，操作已被拒绝" };
    case "UNSUPPORTED": return { text: withMsg("不支持该格式或操作") };
    case "IO": return { text: withMsg("文件读写失败") };
    case "INTERNAL": return { text: withMsg("发生内部错误") };
    case "CANCELLED": return { text: "已取消" };
    case "DEVICE_CODE_EXPIRED": return { text: "验证码已过期，请重新开始登录" };
    case "DECLINED": return { text: "你在浏览器中拒绝了授权" };
    case "NOT_OWNED": return { text: "该微软账号没有 Minecraft Java 版，请先购买正版" };
    case "MS_CLIENT_ID_MISSING": return { text: "尚未配置微软登录所需的 Azure 应用 Client ID，请先在登录窗口中填写" };
    case "CURSEFORGE_KEY_MISSING": return { text: "未配置 CurseForge API Key（环境变量 PCL_CURSEFORGE_KEY），已跳过 CurseForge" };
    default: return { text: err.message || "未知错误" };
  }
}
