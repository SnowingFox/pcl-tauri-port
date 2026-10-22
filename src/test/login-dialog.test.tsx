// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { LoginDialog } from "../pages/launch/LoginDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let host: HTMLDivElement;
let opener: HTMLButtonElement;

beforeEach(() => {
  // The dialog restores focus to whatever had it before opening (the "add account" button).
  opener = document.createElement("button");
  document.body.append(opener);
  opener.focus();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  opener.remove();
  document.body.innerHTML = "";
});

function open(onClose: () => void = () => {}) {
  act(() => root.render(<LoginDialog onClose={onClose} />));
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
}

/** What a keystroke does to a controlled React input: set the value, fire `input`. */
function typeInto(input: HTMLInputElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  act(() => {
    set.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("LoginDialog focus", () => {
  it("puts the cursor in the player name field when it opens", () => {
    const dialog = open();
    const name = dialog.querySelector<HTMLInputElement>("input.input")!;
    expect(name).toBeTruthy();
    // Not on the "offline / Microsoft / third-party" tab buttons that come before it in the DOM.
    expect(document.activeElement).toBe(name);
  });

  it("keeps focus in the same input on every keystroke", () => {
    const dialog = open();
    const name = dialog.querySelector<HTMLInputElement>("input.input")!;
    name.focus();
    // A blur followed by an immediate re-focus ends up on the same element, so checking
    // `activeElement` alone is not enough: it still breaks IME composition and resets the caret.
    const blurs: string[] = [];
    name.addEventListener("focusout", () => blurs.push("focusout"));

    let typed = "";
    for (const ch of "Steve_01") {
      typed += ch;
      typeInto(name, typed);
      // Regression: the dialog used to re-run its focus effect on every render (it depended on
      // `onClose`, a new function each render), which moved focus off the input after each key.
      expect(document.activeElement, `focus after typing "${typed}"`).toBe(name);
      expect(dialog.querySelector("input.input"), "input must not be remounted").toBe(name);
      expect(name.value).toBe(typed);
    }
    expect(blurs, "the input must never lose focus while typing").toEqual([]);
  });

  it("does not steal focus back to the opener while the dialog stays open", () => {
    const dialog = open();
    const name = dialog.querySelector<HTMLInputElement>("input.input")!;
    typeInto(name, "abc");
    typeInto(name, "abcd");
    expect(document.activeElement).not.toBe(opener);
  });

  it("restores focus to the opener when it closes", () => {
    open();
    act(() => root.render(null));
    expect(document.activeElement).toBe(opener);
  });

  it("Escape calls the latest onClose, not a stale one", () => {
    const first = vi.fn();
    const second = vi.fn();
    open(first);
    act(() => root.render(<LoginDialog onClose={second} />));
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
