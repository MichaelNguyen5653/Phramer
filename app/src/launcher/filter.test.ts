import { describe, expect, it } from "vitest";
import { filterApps, moveSelection, wildcard, type AppEntry } from "./filter";

const apps: AppEntry[] = [
  { name: "GIMP 2.10", exe: "C:\\g\\gimp.exe", graphics: true },
  { name: "Paint", exe: "C:\\w\\mspaint.exe", graphics: true },
  { name: "Notepad", exe: "C:\\w\\notepad.exe", graphics: false },
  { name: "Notepad++", exe: "C:\\n\\notepad++.exe", graphics: false },
  { name: "Paint", exe: "C:\\other\\paint.exe", graphics: false },
  { name: "Visual Studio Code", exe: "C:\\c\\code.exe", graphics: false },
];

const names = (list: AppEntry[]) => list.map((app) => app.name);

describe("App Launcher search", () => {
  it("lists everything when nothing is typed, each name once", () => {
    expect(names(filterApps(apps, ""))).toEqual(["GIMP 2.10", "Paint", "Notepad", "Notepad++", "Visual Studio Code"]);
  });

  it("matches anywhere in the name, ignoring case", () => {
    expect(names(filterApps(apps, "pad"))).toEqual(["Notepad", "Notepad++"]);
    expect(names(filterApps(apps, "CODE"))).toEqual(["Visual Studio Code"]);
    expect(names(filterApps(apps, "studio c"))).toEqual(["Visual Studio Code"]);
  });

  it("keeps the Graphics apps first", () => {
    expect(names(filterApps(apps, "p"))).toEqual(["GIMP 2.10", "Paint", "Notepad", "Notepad++"]);
  });

  it("takes * and ? as wildcards and everything else literally", () => {
    expect(names(filterApps(apps, "v*code"))).toEqual(["Visual Studio Code"]);
    expect(names(filterApps(apps, "p?int"))).toEqual(["Paint"]);
    expect(names(filterApps(apps, "++"))).toEqual(["Notepad++"]);
    expect(names(filterApps(apps, "2.10"))).toEqual(["GIMP 2.10"]);
    expect(names(filterApps(apps, "2x10"))).toEqual([]);
    expect(wildcard("(a)[b]").test("x(a)[b]y")).toBe(true);
  });

  it("finds nothing for text in no name", () => {
    expect(filterApps(apps, "zzz")).toEqual([]);
  });
});

describe("App Launcher selection", () => {
  it("moves with the arrows and wraps around", () => {
    expect(moveSelection(0, "ArrowDown", 3)).toBe(1);
    expect(moveSelection(2, "ArrowDown", 3)).toBe(0);
    expect(moveSelection(0, "ArrowUp", 3)).toBe(2);
  });

  it("pages and jumps to the ends without wrapping", () => {
    expect(moveSelection(1, "PageDown", 20)).toBe(9);
    expect(moveSelection(18, "PageDown", 20)).toBe(19);
    expect(moveSelection(3, "PageUp", 20)).toBe(0);
    expect(moveSelection(5, "Home", 20)).toBe(0);
    expect(moveSelection(5, "End", 20)).toBe(19);
  });

  it("keeps the selection inside a list that shrank", () => {
    expect(moveSelection(7, "x", 3)).toBe(2);
    expect(moveSelection(4, "ArrowDown", 0)).toBe(0);
  });
});
