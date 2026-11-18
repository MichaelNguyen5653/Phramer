import { describe, expect, it } from "vitest";
import { screenClipRow, toggledMessage, type ScreenClipStatus } from "./screenclip";

const status = (over: Partial<ScreenClipStatus>): ScreenClipStatus => ({
  registered: false,
  registered_by_phramer: false,
  chosen: false,
  store: false,
  ...over,
});

describe("the MS-SCREENCLIP row (15.0.0's updateScreenClipRow)", () => {
  it("offers Register when no Phramer is listed", () => {
    expect(screenClipRow(status({}))).toEqual({
      text: "Phramer is not registered as MS-SCREENCLIP",
      button: "Register",
      settings: false,
    });
  });

  it("asks for the choice in Windows Settings once registered", () => {
    expect(screenClipRow(status({ registered: true, registered_by_phramer: true }))).toEqual({
      text: "Phramer is registered as MS-SCREENCLIP. Choose it in Windows Settings to finish.",
      button: "Unregister",
      settings: true,
    });
  });

  it("is done once registered and chosen", () => {
    const row = screenClipRow(status({ registered: true, registered_by_phramer: true, chosen: true }));
    expect(row.text).toBe("Phramer is registered and chosen for MS-SCREENCLIP");
    expect(row.button).toBe("Unregister");
  });

  it("sends another install's registration to Unregister first", () => {
    const row = screenClipRow(status({ registered_by_phramer: true }));
    expect(row.text).toBe(
      "Another copy, or an older build, of Phramer is registered as MS-SCREENCLIP. Unregister, then register again.",
    );
    expect(row.button).toBe("Unregister");
  });

  it("warns when Windows still sends Print Screen to an unregistered Phramer", () => {
    expect(screenClipRow(status({ chosen: true }))).toEqual({
      text: "Phramer is not registered, but Windows still sends Print Screen to it. Choose Snipping Tool for MS-SCREENCLIP in Windows Settings.",
      button: "Register",
      settings: true,
    });
  });

  it("has nothing to toggle in the Store edition", () => {
    const row = screenClipRow(status({ store: true, registered: true, registered_by_phramer: true }));
    expect(row.button).toBeNull();
    expect(row.settings).toBe(true);
  });

  it("says how to stop a chosen Store edition answering, as it cannot unregister", () => {
    const row = screenClipRow(status({ store: true, registered: true, registered_by_phramer: true, chosen: true }));
    expect(row).toEqual({
      text: "Phramer is chosen for MS-SCREENCLIP. To stop it answering, choose Snipping Tool for MS-SCREENCLIP in Windows Settings.",
      button: null,
      settings: true,
    });
  });
});

describe("after Register or Unregister", () => {
  it("says when it failed, by direction", () => {
    expect(toggledMessage({ outcome: "failed", unregistered: false, status: status({}) })).toEqual({
      text: "Phramer could not be registered as MS-SCREENCLIP.",
      error: true,
    });
    expect(toggledMessage({ outcome: "failed", unregistered: true, status: status({}) })?.text).toBe(
      "Phramer could not be unregistered as MS-SCREENCLIP.",
    );
  });

  it("warns that an unregistered but still chosen Phramer makes Print Screen ask", () => {
    const message = toggledMessage({ outcome: "succeeded", unregistered: true, status: status({ chosen: true }) });
    expect(message?.error).toBe(false);
    expect(message?.text).toMatch(/^Phramer is unregistered, but Windows still has it chosen/);
  });

  it("is quiet otherwise, a declined UAC prompt included", () => {
    expect(toggledMessage({ outcome: "succeeded", unregistered: false, status: status({ registered: true }) })).toBeNull();
    expect(toggledMessage({ outcome: "succeeded", unregistered: true, status: status({}) })).toBeNull();
    expect(toggledMessage({ outcome: "cancelled", unregistered: false, status: status({}) })).toBeNull();
  });
});
