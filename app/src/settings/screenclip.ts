// The MS-SCREENCLIP row of Settings > Advanced (15.0.0's
// updateScreenClipRow): what it says and offers for a registration status
// (app/src-tauri/src/screenclip.rs).

export interface ScreenClipStatus {
  /** This copy of Phramer is listed as an ms-screenclip handler. */
  registered: boolean;
  /** Some copy of Phramer is listed, this one or another install. */
  registered_by_phramer: boolean;
  /** The user chose Phramer for ms-screenclip in Windows Settings. */
  chosen: boolean;
  /** The Store edition: its package registers it; nothing to toggle. */
  store: boolean;
}

export interface ScreenClipToggled {
  outcome: "succeeded" | "cancelled" | "failed";
  /** It was Unregister. */
  unregistered: boolean;
  status: ScreenClipStatus;
}

export interface ScreenClipRow {
  text: string;
  /** Register or Unregister (both through UAC); none in the Store edition. */
  button: "Register" | "Unregister" | null;
  /** Show the button to Settings > Default apps. */
  settings: boolean;
}

export function screenClipRow(status: ScreenClipStatus): ScreenClipRow {
  // Windows keeps the user's choice after Phramer leaves the list, and then
  // asks which app to use at every Print Screen
  const stillChosen = !status.registered_by_phramer && status.chosen;
  let text: string;
  if (stillChosen) {
    text =
      "Phramer is not registered, but Windows still sends Print Screen to it. Choose Snipping Tool for MS-SCREENCLIP in Windows Settings.";
  } else if (!status.registered_by_phramer) {
    text = "Phramer is not registered as MS-SCREENCLIP";
  } else if (!status.registered) {
    text = "Another copy, or an older build, of Phramer is registered as MS-SCREENCLIP. Unregister, then register again.";
  } else if (status.chosen) {
    text = "Phramer is registered and chosen for MS-SCREENCLIP";
  } else {
    text = "Phramer is registered as MS-SCREENCLIP. Choose it in Windows Settings to finish.";
  }
  return {
    text,
    button: status.store ? null : status.registered_by_phramer ? "Unregister" : "Register",
    settings: status.registered_by_phramer || stillChosen,
  };
}

/** What a finished Register or Unregister says, if it needs saying. */
export function toggledMessage(toggled: ScreenClipToggled): { text: string; error: boolean } | null {
  if (toggled.outcome === "failed") {
    return {
      text: toggled.unregistered
        ? "Phramer could not be unregistered as MS-SCREENCLIP."
        : "Phramer could not be registered as MS-SCREENCLIP.",
      error: true,
    };
  }
  // Unregistered while still chosen: Print Screen asks which app to use
  // until the user picks another, which no program can do for them
  if (toggled.outcome === "succeeded" && toggled.unregistered && toggled.status.chosen) {
    return {
      text: "Phramer is unregistered, but Windows still has it chosen for Print Screen, so the key will ask which app to use. Choose Snipping Tool for MS-SCREENCLIP in the Windows Settings page that opened, or pick it with Always in that prompt.",
      error: false,
    };
  }
  return null;
}
