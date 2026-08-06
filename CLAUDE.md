# Phramer

A Windows-only fork of [Flameshot](https://github.com/flameshot-org/flameshot),
a screenshot capture and annotation tool. Repository:
`MichaelNguyen5653/Phramer` (public).

Released as Flameshot v2 through 14.1.2, rebranded to Phramer in 14.1.3.
**Internal identifiers were deliberately left alone** — the CMake target,
the `Flameshot`/`FlameshotDaemon` classes, the source file names and the
`FLAMESHOT_VERSION` variable all still say flameshot. Only what a user can
see was renamed. Do not "finish" the rename in code; it buys nothing and
touches every file.

Most of this tree is upstream code. Treat it as such: fix what a task
requires and leave the rest alone. The fork's own work is listed under
[Fork Features](#fork-features).

## Project Setup

| | |
|---|---|
| Language | C++20 (`cxx_std_20`) |
| UI | Qt 6.9.3 — Widgets, Gui, Network, Svg, LinguistTools |
| Build | CMake ≥ 3.22, Visual Studio generator (multi-config) |
| Packaging | CPack → WiX `.msi` (per-machine) and portable ZIP |
| Target | Windows 10/11 x64 only. macOS and Linux code still compiles but ships nothing. |

Pulled in at configure time via `FetchContent`: **KDSingleApplication**
(single-instance lock and IPC), **QHotkey** (global hotkeys),
**QtColorWidgets** (colour picker).

## Architecture

Two process roles, decided in `main.cpp` by argument count:

- **`argc == 1` → daemon.** Owns the tray icon, global hotkeys, the update
  checker, and hosts pinned screenshots. This is the only mode that matters
  on Windows.
- **`argc > 1` → CLI.** Parses a subcommand and forwards it to the running
  daemon over KDSingleApplication IPC. `phramer-cli.exe` is a separate
  console-subsystem binary that exists only so stdout works.

A capture is one pass through this chain:

```
tray / hotkey
  └─ Flameshot::gui()            singleton, owns the one CaptureWidget
       └─ ScreenGrabber           grabs pixels, picks the monitor
            └─ CaptureWidget      fullscreen overlay; user draws
                 └─ ~CaptureWidget  exports on destruction
                      └─ Flameshot::exportCapture()  save / copy / pin / upload
```

**The capture is exported from `CaptureWidget`'s destructor.** Closing the
widget is what commits the screenshot; `m_captureDone` decides whether that
means `exportCapture()` or a `captureFailed` signal. Nothing else may
trigger an export.

## File Structure

```
src/
  main.cpp                  entry point; daemon vs CLI split
  core/
    flameshot.{h,cpp}       singleton; capture entry points, export
    flameshotdaemon.{h,cpp} tray owner, update checker, IPC receiver
    capturerequest.{h,cpp}  what a capture should do (tasks bitmask)
    qguiappcurrentscreen.*  "screen under the cursor", with edge fixes
  widgets/
    trayicon.{h,cpp}        tray menu, update badge
    capture/
      capturewidget.*       the fullscreen editor. The big one.
      selectionwidget.*     the selection rectangle and its handles
      buttonhandler.*       lays the tool buttons around the selection
      capturetoolobjects.*  the placed annotation objects
      overlaymessage.*      centred help/error text. Static singleton.
  tools/
    capturetool.h           the tool interface every tool implements
    abstract{action,path,twopoint}tool.*   three bases covering all tools
    toolfactory.*           Type → concrete tool
    highlightstyle.h        shared highlight compositing
    <one directory per tool>
  config/
    generalconf.*           Settings → General
    shortcutswidget.*       Settings → Shortcuts
  utils/
    confighandler.*         all settings access
    valuehandler.*          per-key validation and defaults
    screengrabber.*         screen capture and monitor selection
```

## Component Overview

**`Flameshot`** — singleton (`Flameshot::instance()`). Public API mirrors the
CLI verbs: `gui()`, `screen()`, `full()`, `launcher()`, `config()`. Holds
`m_captureWindow`, a single `QPointer<CaptureWidget>`.

**`FlameshotDaemon`** — singleton created by `start()`. Tray icon, update
polling (startup + every 24 h), clipboard hosting, pin hosting, and the
receiving end of IPC from secondary instances.

**`CaptureWidget`** — the fullscreen overlay. Holds `CaptureContext m_context`
(screenshot, selection, colour, tool size), a `SelectionWidget`, a
`ButtonHandler`, `CaptureToolObjects` for placed annotations, and a
`QUndoStack`.

**`ScreenGrabber`** — produces the pixmap. On Windows `windowsScreenshot()`
grabs each `QScreen` separately at native resolution and composites them
into one physical-pixel canvas, then crops to the chosen monitor.

**`ConfigHandler`** — every setting goes through it. Backed by `QSettings`
(INI). Portable builds keep `phramer.ini` beside the exe; installed builds
use `%APPDATA%`.

## The Tool Interface

Every editor tool implements `CaptureTool` (`src/tools/capturetool.h`).
Rather than implement it directly, inherit one of:

| Base | For | Examples |
|---|---|---|
| `AbstractActionTool` | fires once, draws nothing | Copy, Save, Pin, OCR |
| `AbstractTwoPointTool` | drag from A to B | Rectangle, Circle, Arrow, Marker |
| `AbstractPathTool` | freehand point stream | Pencil |

The methods that matter:

- `process(QPainter&, const QPixmap&)` — draw yourself. Called on every
  repaint, for every placed object. Save and restore any painter state you
  change.
- `pressed(CaptureContext&)` — the button was clicked. Action tools do their
  work here and signal via `requestAction()`.
- `copy(QObject*)` — **placed objects are copies.** When the user completes a
  drag, the button's tool instance is cloned. Any new member must be carried
  across in `copyParams()` or it silently reverts to default on placement.
- `configurationWidget()` — optional; shown in the side panel while the tool
  is selected. Return a fresh widget each call.

Tools never touch `CaptureWidget` directly. They emit
`requestAction(Request)` and the widget interprets it
(`REQ_CLOSE_GUI`, `REQ_ADD_EXTERNAL_WIDGETS`, `REQ_CAPTURE_DONE_OK`, …).

### Adding a tool

1. Append to `CaptureTool::Type` — **bottom only.** The numbers are
   persisted in user configs; inserting shifts everyone's toolbar.
2. Register in `ToolFactory::CreateTool`.
3. Add to `CaptureToolButton::iterableButtonTypes` and `buttonTypeOrder`.
   The default visible-button set derives from the first, so a new tool
   appears automatically for default configs.
4. Add an SVG to `data/img/material/{black,white}/` and register both in
   `data/graphics.qrc`.
5. Add a `SHORTCUT("TYPE_YOURS", "K")` row in `confighandler.cpp` if it
   needs a key. Wiring is automatic — shortcuts resolve by the `Q_ENUM`
   name.

## Coordinate Systems

Get this wrong and the bug looks like "the capture is offset" or "the
selection is the wrong size". Three spaces are in play:

- **Physical pixels** — what `QScreen::grabWindow()` returns and what gets
  saved. A 1920×1200 monitor at 125% yields a 1920×1200 pixmap.
- **Logical (device-independent) pixels** — what Qt widget geometry uses.
  That same monitor is 1536×960 logically.
- **Widget-local** — `CaptureWidget` is positioned at its screen's top-left,
  so widget (0,0) is the screen's corner, not the desktop's.

Rules:

- `m_context.screenshot` carries the screen's device pixel ratio. Multiply
  by `devicePixelRatio()` to go from selection geometry to output pixels —
  `extendedRect()` does this.
- `m_context.widgetOffset` (maintained in `moveEvent`/`resizeEvent`) converts
  widget-local back to global.
- **A single window cannot span mixed-DPI monitors.** Qt gives a window one
  scale factor while the desktop has one per screen, so there is no scale
  that maps the logical union of screens onto physical space. A spanning
  overlay renders at the wrong size on every screen but one. This is not
  fixable arithmetic — it is why multi-monitor capture uses one window per
  screen. Do not reintroduce a spanning window.
- Monitors can sit at **negative coordinates** (left of, or above, the
  primary). Never assume the desktop starts at (0,0); translate by the
  virtual geometry's top-left.

## Threading Model

Qt GUI rules apply: widgets and pixmaps are GUI-thread only.

The only worker thread is OCR (`OcrWorker`, `src/tools/ocr/`). It owns its
`OcrEngine`, initialises its own COM apartment, and communicates **only**
through a queued signal. That is deliberate: the results window can be
closed mid-recognition and Qt severs the connection safely, letting the
thread finish and delete itself.

Long WinRT calls block that thread, never the GUI. If you add async work,
follow the same shape — no shared state, results marshalled by signal.

## Persistence

All settings go through `ConfigHandler`. Two macros do the work:

```cpp
// confighandler.cpp — declare the key, its type, and its default
OPTION("rectangleFillMode", BoundedInt( 0, 1, 0 )),

// confighandler.h — generate rectangleFillMode() / setRectangleFillMode()
CONFIG_GETTER_SETTER(rectangleFillMode, setRectangleFillMode, int)
```

Value types live in `valuehandler.h`: `Bool`, `String`, `Color`,
`BoundedInt`, `LowerBoundedInt`, `KeySequence`, `ExistingDir`,
`FilenamePattern`, `ButtonList`, `UserColors`, `SaveFileExtension`, `Region`.
Each validates and supplies a fallback, so a corrupt value degrades to the
default rather than crashing.

Rules:

- **Never remove a key that shipped.** Unrecognized keys in a user's config
  raise a visible error. Deprecate by leaving the `OPTION` in place.
- A key that is absent must produce the previous behaviour. When replacing a
  bool with an enum, derive the enum from the old bool when the new key is
  missing (see `captureRegionMode`).
- Platform-specific keys go inside the matching `#if` in *both* files.

## Fork Features

- **Snip across all monitors** — `captureRegionMode` = 0 select / 1 active /
  2 all. In mode 2 the capture widget binds to the screen under the cursor
  and follows it; other screens get lightweight dimming overlays. The first
  mouse press commits to one screen and destroys the overlays. Every window
  stays on exactly one screen — see [Coordinate Systems](#coordinate-systems).
- **OCR** (`TYPE_OCR`, key `O`) — Windows.Media.Ocr via C++/WinRT, behind an
  abstract `OcrEngine` so another platform can supply its own. Preprocessing
  upscales small text and inverts dark backgrounds; a second pass runs at
  higher resolution when the recognised glyphs are small.
- **Highlighter rectangles** — the rectangle tool's fill mode. Highlight
  compositing (multiply at 0.35 painter opacity) lives in
  `tools/highlightstyle.h` and is shared with the marker. **Use the helper**;
  do not re-specify the constant, and note the opacity is applied to the
  painter, not to the colour's alpha — they differ under multiply.
- **Tray Restart** — sets a flag on `Flameshot` and quits; `main()` relaunches
  after the single-instance lock is released. Spawning before quitting races
  the lock and can leave nothing running.
- **First-run welcome** — offers to free the Print Screen key from Windows'
  snipping tool. Registry access is shared in `utils/printscreenkey`.
- **In-app updates** — see `docs/update-notification-spec.md`.

## Coding Conventions

- **Format with clang-format 11.** CI pins it and newer versions disagree.
  `pip install clang-format==11.1.0`; do not use the one bundled with Visual
  Studio.
- Use `&Class::method` connect syntax, not `SIGNAL`/`SLOT` strings.
- Wrap user-facing strings in `tr()`.
- Guard Windows-only code with `Q_OS_WIN` and keep other platforms
  compiling. They are not shipped but they are still built by
  `build_cmake.yml` on demand.
- Comment the constraint, not the mechanics — why a line must exist, not
  what it does.

### Traps

- **`CaptureWidget` is effectively a singleton.** `OverlayMessage` is a
  static instance parented to it, its destructor exports the capture, and
  `Flameshot::m_captureWindow` is one pointer. Do not instantiate two.
- **Deleting a widget inside its own event handler.** Teardown reachable
  from a child's key handler must use `deleteLater()`.
- **PowerShell does not expand variables in a bare token starting with a
  dash.** `-DFOO=$bar` passes the literal text; write `"-DFOO=$bar"`. This
  broke the release pipeline three times.
- **`cmake --install` deploys debug Qt DLLs.** `src/CMakeLists.txt` picks
  `windeployqt --debug` from `CMAKE_BUILD_TYPE`, which is always empty for
  multi-config generators, and the app then dies with "no Qt platform plugin
  could be initialized". Copy the exe instead, or re-run
  `windeployqt --release`.

## Build & Run

Qt lives at `C:\Qt\6.9.3\msvc2022_64`, installed with `aqtinstall` — the Qt
GUI installer fails on this network with a redirect loop. CMake is the one
bundled with Visual Studio:

```
"C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
```

```bash
cmake .. -G "Visual Studio 18 2026" -A x64 -DCMAKE_PREFIX_PATH="C:\Qt\6.9.3\msvc2022_64"
cmake --build . --config RelWithDebInfo
```

To test, copy the built exe over `build\install\bin\phramer.exe` — that
folder already holds a correct Qt runtime. A running `phramer.exe` locks
the file, so stop the process first.

`-DFLAMESHOT_DEBUG_CAPTURE=ON` makes the capture overlay a normal window
instead of a frameless always-on-top one, which is useful under a debugger.
Never ship it: the overlay depends on those window flags.

## Releasing

The **git tag is the version**. `Windows-release.yml` derives it and passes
`RELEASE_VERSION` to CMake; a configure-time guard rejects anything that is
not `MAJOR.MINOR.PATCH`.

```bash
# bump set(FLAMESHOT_VERSION x.y.z) in CMakeLists.txt first, so
# Windows-pack artifacts agree with the release
git tag v14.1.1 && git push origin v14.1.1
```

That builds the MSI, generates a SHA256 sum, and publishes both as release
assets. The in-app updater consumes exactly those, so:

- **Versions must strictly increase.** Re-tagging leaves clients thinking
  they are current.
- **The checksum asset is load-bearing.** The updater refuses to install
  without it.
- **`CPACK_WIX_UPGRADE_GUID` must never change again.** It is what makes
  `msiexec /i` upgrade in place. It was changed once, deliberately, to stop
  this fork replacing upstream installs. It is also what let the Phramer
  rename ship as an ordinary in-place upgrade rather than an uninstall.
- **Never restart the version series.** The WiX `MajorUpgrade` element
  refuses a downgrade, so a "1.0.0" after 14.1.2 fails to install on every
  existing machine. 14.x continues regardless of what the branding says.

The installer is unsigned, so users get a SmartScreen warning; installation
needs administrator approval because the MSI is per-machine.

### The 14.1.3 migration

Builds through 14.1.2 have `https://api.github.com/repos/MichaelNguyen5653/
Flameshot-v2/releases/latest` compiled in and will never look anywhere else,
so 14.1.3 was published **twice**: normally here, and mirrored onto a release
on the old Flameshot-v2 repo for those clients to find. Nothing after 14.1.3
needs the old repo.

The mirrored release is tagged `14.1.3` with **no `v` prefix**, on purpose.
`Windows-release.yml` triggers on `v*`, and a `v14.1.3` tag there would have
rebuilt the old pre-rename source and overwritten the assets. The updater
strips a leading `v` before parsing, so the bare tag reads the same to it.

One known rough edge in that hop: the old build relaunches
`applicationFilePath()` after installing, and the upgrade has already deleted
that path, so the app does not reopen on its own. Users start Phramer once
from the Start Menu.

`Linux-pack`, `MacOS-pack`, `build_cmake` and `deploy-dev-docs` are
`workflow_dispatch` only. `Windows-pack` and `test-clang-format` run on every
push.
