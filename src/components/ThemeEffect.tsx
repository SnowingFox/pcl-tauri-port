import { useEffect } from "react";
import { HUES, useUi } from "../store/ui";

export function applyTheme(mode: "light" | "dark" | "system", hue: keyof typeof HUES, reduce: boolean): void {
  const root = document.documentElement;
  const dark = mode === "dark" || (mode === "system" && window.matchMedia?.("(prefers-color-scheme: dark)").matches);
  root.dataset.theme = dark ? "dark" : "light";
  root.style.setProperty("--hue", String(HUES[hue]));
  root.dataset.motion = reduce ? "reduce" : "";
}

export function ThemeEffect() {
  const { mode, hue, reduceMotion } = useUi();
  useEffect(() => {
    applyTheme(mode, hue, reduceMotion);
    if (mode !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const h = () => applyTheme(mode, hue, reduceMotion);
    mq.addEventListener("change", h);
    return () => mq.removeEventListener("change", h);
  }, [mode, hue, reduceMotion]);
  return null;
}
