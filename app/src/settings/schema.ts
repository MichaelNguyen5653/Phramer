// Every option of 15.0.0's configuration window (General, Interface,
// Filename Editor, Shortcuts and Advanced tabs), regrouped into the
// sections of the sidebar. Labels and help follow 15.0.0's texts.

import type { Snapshot } from "./types";

export type SectionId = "general" | "capture" | "saving" | "appearance" | "editing" | "shortcuts" | "advanced";

export interface Section {
  id: SectionId;
  title: string;
  /** A Lucide name from Icon.tsx. */
  icon: string;
}

export const SECTIONS: readonly Section[] = [
  { id: "general", title: "General", icon: "settings" },
  { id: "capture", title: "Capture", icon: "scan" },
  { id: "saving", title: "Saving", icon: "save" },
  { id: "appearance", title: "Appearance", icon: "palette" },
  { id: "editing", title: "Editing", icon: "pencil" },
  { id: "shortcuts", title: "Shortcuts", icon: "keyboard" },
  { id: "advanced", title: "Advanced", icon: "sliders-horizontal" },
];

export type Control =
  | { kind: "switch"; inverted?: boolean }
  | { kind: "number"; min: number; max: number; unit?: string }
  | { kind: "percent"; max: number }
  | { kind: "select"; options: readonly (readonly [number | string, string])[] }
  | { kind: "extension" }
  | { kind: "folder" }
  | { kind: "pattern" }
  | { kind: "color" }
  | { kind: "palette" }
  | { kind: "toolbar" }
  | { kind: "printscreen" }
  | { kind: "file" };

export interface SettingDef {
  /** The `[General]` key, or a name of the page's own for a row that is not one. */
  key: string;
  section: SectionId;
  label: string;
  help?: string;
  /** More words the search box should find it by. */
  keywords?: string;
  control: Control;
  /** Why it cannot be changed here now, if it cannot. */
  unavailable?: (snapshot: Snapshot) => string | null;
}

const MONITOR_MODES = [
  [0, "Ask which monitor to capture"],
  [1, "Capture the monitor under the cursor"],
  [2, "Snip across all monitors"],
] as const;

const GEOMETRY_PLACES = [
  [0, "None"],
  [1, "Top left"],
  [2, "Top right"],
  [3, "Bottom left"],
  [4, "Bottom right"],
  [5, "Center"],
] as const;

export const SETTINGS: readonly SettingDef[] = [
  // --- General -------------------------------------------------------------
  {
    key: "startupLaunch",
    section: "general",
    label: "Launch at sign-in",
    help: "Start Phramer in the background when you sign in to Windows.",
    keywords: "startup autostart boot",
    control: { kind: "switch" },
  },
  {
    key: "captureRegionMode",
    section: "general",
    label: "Multi-monitor capture",
    help: "How the capture region is chosen when more than one monitor is connected.",
    keywords: "screen display",
    control: { kind: "select", options: MONITOR_MODES },
  },
  {
    key: "autoOpenInEditor",
    section: "general",
    label: "Open every capture in the editor",
    help: "Send each capture straight to the editor once the region is selected, instead of showing the action bar.",
    control: { kind: "switch" },
  },
  {
    key: "copyOnDoubleClick",
    section: "general",
    label: "Copy on double-click",
    help: "Double-click the selection to copy it to the clipboard.",
    keywords: "clipboard",
    control: { kind: "switch" },
  },
  {
    key: "showQuitPrompt",
    section: "general",
    label: "Ask before closing a capture",
    help: "Confirm before Esc throws a capture away.",
    keywords: "quit escape confirm",
    control: { kind: "switch" },
  },
  {
    key: "checkForUpdates",
    section: "general",
    label: "Check for updates automatically",
    keywords: "update version",
    control: { kind: "switch" },
  },
  {
    key: "disabledTrayIcon",
    section: "general",
    label: "Show the tray icon",
    help: "Phramer's icon in the notification area, with its menu.",
    keywords: "notification area system tray",
    control: { kind: "switch", inverted: true },
  },
  {
    key: "autoCloseIdleDaemon",
    section: "general",
    label: "Unload from memory when not needed",
    help: "Closes Phramer in the background when nothing needs it. Saves memory, but hotkeys and the tray icon stop until Phramer is started again.",
    keywords: "daemon background",
    control: { kind: "switch" },
  },
  {
    key: "allowMultipleGuiInstances",
    section: "general",
    label: "Allow more than one capture at a time",
    help: "Lets a new capture start while one is already open, for example to capture Phramer itself.",
    keywords: "multiple instances",
    control: { kind: "switch" },
  },
  {
    key: "showDesktopNotification",
    section: "general",
    label: "Show desktop notifications",
    keywords: "toast balloon",
    control: { kind: "switch" },
  },
  {
    key: "showAbortNotification",
    section: "general",
    label: "Notify when a capture is cancelled",
    keywords: "abort notification",
    control: { kind: "switch" },
  },
  {
    key: "showStartupLaunchMessage",
    section: "general",
    label: "Show a notification when Phramer starts",
    keywords: "welcome message launch",
    control: { kind: "switch" },
  },
  {
    key: "showWelcomeMessage",
    section: "general",
    label: "Show the welcome again on next start",
    help: "The welcome shows once and then turns this off. Switch it on to see it again.",
    keywords: "first run tour print screen",
    control: { kind: "switch" },
  },

  // --- Capture -------------------------------------------------------------
  {
    key: "showHelp",
    section: "capture",
    label: "Show the help message",
    help: "Lists the main keys on screen when a capture starts.",
    control: { kind: "switch" },
  },
  {
    key: "showEditorHint",
    section: "capture",
    label: "Show the keyboard tip",
    help: "A tip on the capture screen naming the keys that open the editor and the tool settings.",
    control: { kind: "switch" },
  },
  {
    key: "showToolLabels",
    section: "capture",
    label: "Show tool names",
    help: "Each tool's name under its button on the capture screen and in the editor.",
    keywords: "labels",
    control: { kind: "switch" },
  },
  {
    key: "showSidePanelButton",
    section: "capture",
    label: "Show the tool settings button",
    help: "A button on the capture screen that opens tool settings (colour, size). Space does the same.",
    keywords: "side panel",
    control: { kind: "switch" },
  },
  {
    key: "showMagnifier",
    section: "capture",
    label: "Show the magnifier",
    help: "A magnifier next to the pointer while you select the area.",
    keywords: "zoom loupe",
    control: { kind: "switch" },
  },
  {
    key: "squareMagnifier",
    section: "capture",
    label: "Square magnifier",
    control: { kind: "switch" },
  },
  {
    key: "saveLastRegion",
    section: "capture",
    label: "Start from the last selection",
    help: "Each new capture starts with your previous selection already drawn.",
    keywords: "region remember",
    control: { kind: "switch" },
  },
  {
    key: "showSelectionGeometry",
    section: "capture",
    label: "Selection size display",
    help: "Where the selection's position and size are shown.",
    keywords: "geometry dimensions xywh",
    control: { kind: "select", options: GEOMETRY_PLACES },
  },
  {
    key: "showSelectionGeometryHideTime",
    section: "capture",
    label: "Hide the size display after",
    help: "0 keeps it on screen.",
    keywords: "geometry timeout",
    control: { kind: "number", min: 0, max: 2147483647, unit: "ms" },
  },
  {
    key: "contrastOpacity",
    section: "capture",
    label: "Dimming outside the selection",
    keywords: "opacity contrast darken",
    control: { kind: "percent", max: 255 },
  },
  {
    key: "videoCaptureEnabled",
    section: "capture",
    label: "Video capture",
    help: "Adds a Video mode to the capture screen and Record Video to the tray menu. The Windows Snipping Tool does the recording.",
    keywords: "record screen recording snipping",
    control: { kind: "switch" },
    unavailable: (snapshot) =>
      snapshot.snipping_tool_installed ? null : "Needs the Windows Snipping Tool, which is not installed",
  },
  {
    key: "buttons",
    section: "capture",
    label: "Toolbar buttons",
    help: "The buttons around a selection.",
    keywords: "button selection tools",
    control: { kind: "toolbar" },
  },

  // --- Saving --------------------------------------------------------------
  {
    key: "savePath",
    section: "saving",
    label: "Save to",
    keywords: "folder directory path location",
    control: { kind: "folder" },
  },
  {
    key: "savePathFixed",
    section: "saving",
    label: "Always save to this folder",
    help: "Off: Save As starts in the folder you saved to last.",
    keywords: "fixed path",
    control: { kind: "switch" },
  },
  {
    key: "filenamePattern",
    section: "saving",
    label: "File name",
    help: "Made from the date and time of the capture.",
    keywords: "filename pattern name editor strftime",
    control: { kind: "pattern" },
  },
  {
    key: "saveAsFileExtension",
    section: "saving",
    label: "File type",
    keywords: "extension format png jpg",
    control: { kind: "extension" },
  },
  {
    key: "jpegQuality",
    section: "saving",
    label: "JPEG quality",
    help: "Higher is better quality and a larger file.",
    keywords: "jpg compression",
    control: { kind: "number", min: 0, max: 100 },
  },
  {
    key: "saveAfterCopy",
    section: "saving",
    label: "Save after copying",
    help: "A copied capture is also saved to a file.",
    keywords: "clipboard",
    control: { kind: "switch" },
  },
  {
    key: "copyPathAfterSave",
    section: "saving",
    label: "Copy the file path after saving",
    keywords: "clipboard",
    control: { kind: "switch" },
  },

  // --- Appearance ------------------------------------------------------------
  {
    key: "uiColor",
    section: "appearance",
    label: "Main colour",
    help: "The capture screen's buttons and selection.",
    keywords: "ui color theme accent",
    control: { kind: "color" },
  },
  {
    key: "contrastUiColor",
    section: "appearance",
    label: "Contrast colour",
    help: "The active button and other highlights.",
    keywords: "ui color theme",
    control: { kind: "color" },
  },
  {
    key: "userColors",
    section: "appearance",
    label: "Colour palette",
    help: "The colours offered for drawing, in the editor and on the capture screen.",
    keywords: "colorpicker presets swatches",
    control: { kind: "palette" },
  },
  {
    key: "predefinedColorPaletteLarge",
    section: "appearance",
    label: "Large default palette",
    help: "17 colours instead of 11 while the palette above is not customised.",
    keywords: "color",
    control: { kind: "switch" },
  },
  {
    key: "uiLanguage",
    section: "appearance",
    label: "Language",
    keywords: "translation locale",
    control: { kind: "select", options: [["auto", "Same as Windows"]] },
  },

  // --- Editing ---------------------------------------------------------------
  {
    key: "undoLimit",
    section: "editing",
    label: "Undo steps",
    keywords: "history",
    control: { kind: "number", min: 1, max: 999 },
  },
  {
    key: "reverseArrow",
    section: "editing",
    label: "Arrow head at the start",
    help: "Draw arrows head first.",
    keywords: "reverse arrow",
    control: { kind: "switch" },
  },
  {
    key: "insecurePixelate",
    section: "editing",
    label: "Insecure pixelate",
    help: "A real blur: it looks nicer but can sometimes be reversed to reveal what was hidden. Leave off for anything sensitive.",
    keywords: "blur redact",
    control: { kind: "switch" },
  },
  {
    key: "antialiasingPinZoom",
    section: "editing",
    label: "Smooth zoomed pins",
    help: "Smooths a pinned capture when you zoom it. Off keeps the pixels sharp.",
    keywords: "anti-aliasing pin",
    control: { kind: "switch" },
  },

  // --- Shortcuts (the list itself is built from the snapshot) -----------------
  {
    key: "printScreen",
    section: "shortcuts",
    label: "Use Print Screen to capture",
    help: "Windows opens its Snipping Tool on Print Screen unless this is on. Changes only your own Windows setting, and can be switched back at any time.",
    keywords: "prtscr prtsc snipping tool key",
    control: { kind: "printscreen" },
  },

  // --- Advanced --------------------------------------------------------------
  {
    key: "settingsFile",
    section: "advanced",
    label: "Settings file",
    help: "Export a copy, import one, or put every setting but the shortcuts back to its default.",
    keywords: "configuration import export reset backup ini",
    control: { kind: "file" },
  },
];

/** The text the search box looks through for a setting. */
export function searchText(def: SettingDef): string {
  const section = SECTIONS.find((entry) => entry.id === def.section)?.title ?? "";
  return [def.label, def.help, def.keywords, def.key, section].filter(Boolean).join(" ");
}
