import { describe, expect, it } from "vitest";
import { AppError, describeError, normalizeError } from "../api/errors";

describe("normalizeError", () => {
  it("keeps structured backend errors", () => {
    const e = normalizeError({ code: "JAVA_MISSING", message: "no java", details: { requiredMajor: 21, found: [8] } });
    expect(e).toBeInstanceOf(AppError);
    expect(e.code).toBe("JAVA_MISSING");
    expect(e.details).toEqual({ requiredMajor: 21, found: [8] });
  });
  it("derives codes from legacy string errors", () => {
    expect(normalizeError("login was cancelled").code).toBe("CANCELLED");
    expect(normalizeError("the device code expired, please start again").code).toBe("DEVICE_CODE_EXPIRED");
    expect(normalizeError("authorization was declined").code).toBe("DECLINED");
    expect(normalizeError("this account does not own Minecraft: Java Edition").code).toBe("NOT_OWNED");
    expect(normalizeError("Microsoft client ID is not configured (set PCL_MS_CLIENT_ID or the accounts setting)").code).toBe("MS_CLIENT_ID_MISSING");
    expect(normalizeError("login expired, please sign in again").code).toBe("ACCOUNT_EXPIRED");
    expect(normalizeError("network error: dns").code).toBe("NETWORK");
    expect(normalizeError("whatever").code).toBe("UNKNOWN");
  });
  it("handles Error objects and unknowns", () => {
    expect(normalizeError(new Error("x")).message).toBe("x");
    expect(normalizeError(42).message).toBe("42");
  });
});

describe("describeError", () => {
  it("JAVA_MISSING offers the Java settings and names the required major", () => {
    const v = describeError({ code: "JAVA_MISSING", message: "m", details: { requiredMajor: 21, found: [] } });
    expect(v.text).toContain("Java 21");
    expect(v.action).toEqual({ label: "前往 Java 设置", route: "/settings/java" });
  });
  it("ACCOUNT_EXPIRED sends the user to log in again", () => {
    expect(describeError({ code: "ACCOUNT_EXPIRED", message: "" }).action?.route).toBe("/launch");
  });
  it("FILES_MISSING lists the issues in Chinese", () => {
    const v = describeError({ code: "FILES_MISSING", message: "", details: { issues: [{ kind: "libraries_missing", count: 3, first: [] }, { kind: "client_jar_missing", path: "/x" }] } });
    expect(v.text).toContain("缺少 3 个依赖库");
    expect(v.text).toContain("缺少游戏主程序");
    expect(v.action?.route).toBe("/download/game");
  });
  it("PRECHECK_FAILED points to Java settings for java/memory issues only", () => {
    expect(describeError({ code: "PRECHECK_FAILED", message: "", details: { issues: [{ kind: "no_java", required_major: 17 }] } }).action?.route).toBe("/settings/java");
    expect(describeError({ code: "PRECHECK_FAILED", message: "", details: { issues: [{ kind: "empty_player_name" }] } }).action).toBeUndefined();
  });
  it("covers every documented code with Chinese text", () => {
    const codes = ["JAVA_MISSING", "ACCOUNT_NOT_FOUND", "ACCOUNT_EXPIRED", "INSTANCE_NOT_FOUND", "FILES_MISSING", "PRECHECK_FAILED", "DOWNLOAD_FAILED", "LAUNCH_FAILED", "NETWORK", "INVALID_ARGUMENT", "FORBIDDEN_PATH", "UNSUPPORTED", "IO", "INTERNAL"];
    for (const code of codes) expect(describeError({ code, message: "raw" }).text).toMatch(/[一-鿿]/);
  });
  it("falls back to the message for unknown codes", () => {
    expect(describeError({ code: "WEIRD", message: "raw text" }).text).toBe("raw text");
  });
});
