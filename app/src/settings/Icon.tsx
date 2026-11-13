// The editor's Lucide icons (the overlay's names, for the toolbar buttons)
// and the few the settings and welcome windows add.

import {
  Download,
  FolderOpen,
  Keyboard,
  Palette,
  RotateCcw,
  Search,
  Settings,
  SlidersHorizontal,
  Trash2,
  Upload,
  type LucideIcon,
} from "lucide-react";
import { ICONS as EDITOR_ICONS } from "../editor/Icon";

const ICONS: Record<string, LucideIcon> = {
  ...EDITOR_ICONS,
  download: Download,
  "folder-open": FolderOpen,
  keyboard: Keyboard,
  palette: Palette,
  "rotate-ccw": RotateCcw,
  search: Search,
  settings: Settings,
  "sliders-horizontal": SlidersHorizontal,
  "trash-2": Trash2,
  upload: Upload,
};

export function Icon({ name }: { name: string }) {
  const Glyph = ICONS[name];
  return Glyph ? <Glyph className="icon" aria-hidden="true" strokeWidth={2} /> : null;
}
