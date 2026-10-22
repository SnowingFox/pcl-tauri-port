/// <reference types="node" />
import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}
const root = new URL("../", import.meta.url).pathname;
const sources = walk(root).filter((f) => /\.(tsx?|html)$/.test(f) && !f.includes("/test/"));

describe("untrusted content handling", () => {
  it("never injects HTML: no dangerouslySetInnerHTML / innerHTML / outerHTML / insertAdjacentHTML in src", () => {
    for (const f of sources) {
      const text = readFileSync(f, "utf-8");
      expect(text, f).not.toMatch(/dangerouslySetInnerHTML|\.innerHTML|\.outerHTML|insertAdjacentHTML|document\.write\(/);
    }
  });
  it("external links go through appApi.openUrl, never window.open / location from UI code", () => {
    for (const f of sources) {
      if (f.endsWith("mock/app.ts")) continue; // browser fallback of the mock
      const text = readFileSync(f, "utf-8");
      expect(text, f).not.toMatch(/window\.open\(|location\.href\s*=/);
    }
  });
  it("does not use the opener plugin or shell directly from the frontend", () => {
    for (const f of sources) expect(readFileSync(f, "utf-8"), f).not.toMatch(/plugin-opener|plugin-shell/);
  });
});

describe("licence requirements", () => {
  const zh = readFileSync(join(root, "i18n/zh-CN.ts"), "utf-8");
  const settingsPages = readFileSync(join(root, "pages/settings/SettingsPages.tsx"), "utf-8");
  it("project name starts with Plain Craft Launcher (PCL) and states it is third party", () => {
    expect(zh).toMatch(/name: "Plain Craft Launcher \(PCL\)/);
    expect(zh).toMatch(/第三方/);
    expect(zh).toMatch(/非官方/);
  });
  it("the About page credits 龙腾猫跃 first, with the sponsor link", () => {
    const about = settingsPages.slice(settingsPages.indexOf("export function AboutPage"));
    expect(about.indexOf("about.authorName")).toBeGreaterThan(-1);
    expect(about.indexOf("about.authorName")).toBeLessThan(about.indexOf("app.name"));
    expect(zh).toContain("龙腾猫跃");
    expect(zh).toContain("https://meloong.com/afd/a/LTCat");
  });
  it("the HTML title carries the PCL-prefixed name", () => {
    expect(readFileSync(new URL("../../index.html", import.meta.url), "utf-8")).toContain("<title>Plain Craft Launcher (PCL)");
  });
  it("has no demo crash button or demo data in real code paths", () => {
    expect(zh).not.toMatch(/testCrash|模拟一次崩溃/);
    for (const f of sources) {
      if (f.includes("/api/mock/")) continue;
      expect(readFileSync(f, "utf-8"), f).not.toMatch(/launchId: "demo"|示例：模组冲突/);
    }
  });
});
