# Phramer codebase impact audit for a Microsoft Store (MSIX) edition

Scope note: every "source" below that is a repository path was read directly in
`/home/user/Phramer` at commit `027e8a2` (HEAD, "Update docs"). Links are
repo-relative with line numbers. External facts (MSIX behaviour, Store rules,
third-party licences) cite Microsoft Learn or the upstream repository.
Nothing in the repository was modified.

## How the Store edition should be selected (build-flag design)

### Takeaway
Use a compile-time CMake option `PHRAMER_STORE_BUILD` (adds a `PHRAMER_STORE_BUILD`
compile definition and implies `DISABLE_UPDATE_CHECKER` and `USE_PORTABLE_CONFIG=OFF`)
to remove code the Store edition must not ship: the self-updater, msiexec, `runas`
elevation and HKLM writes. Add a small runtime helper (`GetCurrentPackageFullName`) to
choose the integration path that actually works when the process has package
identity: StartupTask instead of the Run key, AUMID relaunch, and the Settings-page
handoff. The existing `DISABLE_UPDATE_CHECKER` switch is currently broken: it does not
compile, and it drops a shipped config key. Both problems must be fixed before the
Store build can depend on it. With the option OFF (the default), the MSI and ZIP builds
are byte-for-byte unaffected.

### Cited Findings
- An opt-out switch for the updater already exists: `option(DISABLE_UPDATE_CHECKER "Disable check for updates" OFF)` adds the global definition `DISABLE_UPDATE_CHECKER` — [CMakeLists.txt:114, 121-123](CMakeLists.txt#L114). Only non-Windows packaging uses it today: `flake.nix:86` and `PKGBUILD:37` (grep). No Windows workflow passes it ([.github/workflows/Windows-pack.yml:118-122](.github/workflows/Windows-pack.yml#L118), [Windows-release.yml:83](.github/workflows/Windows-release.yml#L83)).
- **`DISABLE_UPDATE_CHECKER=ON` does not compile today.** `m_notificationFile` is declared inside `#if !defined(DISABLE_UPDATE_CHECKER)` ([src/core/flameshotdaemon.h:131-139](src/core/flameshotdaemon.h#L131), member at line 134). It is used unconditionally in `sendTrayNotification` ([src/core/flameshotdaemon.cpp:309](src/core/flameshotdaemon.cpp#L309)) and in the tray `messageClicked` handler ([src/core/flameshotdaemon.cpp:700-701](src/core/flameshotdaemon.cpp#L700)). That is the "Show in folder" feature from 15.0.
- **`DISABLE_UPDATE_CHECKER=ON` also removes a shipped config key.** `OPTION("checkForUpdates", Bool(true))` is wrapped in `#if !defined(DISABLE_UPDATE_CHECKER)` ([src/utils/confighandler.cpp:83-85](src/utils/confighandler.cpp#L83)). `checkUnrecognizedSettings` flags any key in the file that is not in the recognized map ([src/utils/confighandler.cpp:763-797](src/utils/confighandler.cpp#L763)), and that puts ConfigHandler into its error state ([src/utils/confighandler.cpp:908-923](src/utils/confighandler.cpp#L908)). CLAUDE.md says "Never remove a key that shipped … Deprecate by leaving the `OPTION` in place." The getter/setter is likewise guarded at [src/utils/confighandler.h:116-118](src/utils/confighandler.h#L116). `ignoreUpdateToVersion` is *not* guarded ([confighandler.cpp:147](src/utils/confighandler.cpp#L147), [confighandler.h:144-146](src/utils/confighandler.h#L144)).
- The flag already gates every other updater surface:
  - [src/widgets/CMakeLists.txt:64-71](src/widgets/CMakeLists.txt#L64): `updatenotificationwidget.*`
  - `trayicon.h` lines 14, 28, 47 and `trayicon.cpp` lines 111, 212, 298, 320, 382
  - `capturewidget.h` lines 42, 60, 217 and `capturewidget.cpp` lines 48, 74, 1806
  - `generalconf.h` lines 53, 101, 174 and `generalconf.cpp` lines 55, 163, 238, 612 (grep results).
- Runtime detection: MSIX gives the process *package identity*, and the Learn page on packaged desktop apps describes this as "the purpose of packaging your app" — [Understanding how packaged desktop apps run on Windows](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes).
- The package directory is read-only: "Writes under `C:\Program Files\WindowsApps\<package_full_name>` aren't allowed" — [same page](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes). The portable config writes `phramer.ini` next to the exe ([src/utils/confighandler.cpp:304-306](src/utils/confighandler.cpp#L304)), so the Store build must use `USE_PORTABLE_CONFIG=OFF`. That option defaults ON on Windows ([CMakeLists.txt:88-105](CMakeLists.txt#L88)).

### Inferences
- **Recommended CMake shape**, in the top-level `CMakeLists.txt` next to lines 107-123:
  ```cmake
  option(PHRAMER_STORE_BUILD "Build the Microsoft Store (MSIX) edition" OFF)
  if(PHRAMER_STORE_BUILD)
    if(NOT WIN32)
      message(FATAL_ERROR "PHRAMER_STORE_BUILD is Windows-only")
    endif()
    if(USE_PORTABLE_CONFIG)
      message(FATAL_ERROR "PHRAMER_STORE_BUILD requires -DUSE_PORTABLE_CONFIG=OFF (the package directory is read-only)")
    endif()
    set(DISABLE_UPDATE_CHECKER ON CACHE BOOL "" FORCE)
    add_compile_definitions(PHRAMER_STORE_BUILD)
  endif()
  ```
  Fail loudly instead of silently forcing `USE_PORTABLE_CONFIG`. The cache default is ON on Windows, and CLAUDE.md already records this variable as a trap.
- **Fix `DISABLE_UPDATE_CHECKER` before relying on it.** These changes also fix the Nix and Arch builds that already set it:
  - (a) move `m_notificationFile` out of the guard in `flameshotdaemon.h`;
  - (b) keep `OPTION("checkForUpdates")` and its `CONFIG_GETTER_SETTER` unconditional, so only the behaviour is compiled out. The settings checkbox stays guarded.
- **Runtime helper.** Add `src/utils/packageidentity.{h,cpp}`, Windows-only, in the same style as `printscreenkey.*`, with:
  - `bool isPackaged()`, built on `GetCurrentPackageFullName` returning `APPMODEL_ERROR_NO_PACKAGE` when the process has no identity;
  - `QString packageFamilyName()`;
  - `QString appUserModelId()`, i.e. `<PFN>!<ApplicationId>`.

  Use it for autostart, restart, the ms-screenclip UI and the Print Screen flow. A Store binary run unpackaged during development then still behaves sensibly. The MSI and ZIP builds keep their code paths, because `isPackaged()` is always false for them.
- **Why both a compile flag and runtime detection.** The compile flag keeps the updater, `cmd.exe`/`msiexec` and `runas` code out of the Store binary. The runtime check alone would ship that code and rely on it never running. Runtime detection alone also cannot change the Settings UI (the "Automatic check for updates" checkbox) cleanly.
- **Register the source in CMake.** `src/utils/CMakeLists.txt` lists Windows-only sources under `IF (WIN32)` ([src/utils/CMakeLists.txt:52-63](src/utils/CMakeLists.txt#L52)), so the new helper goes there. It needs `kernel32` (appmodel.h), which is already linked. The WinRT pieces (StartupTask) can use `windowsapp`, which `src/tools/CMakeLists.txt:94` already links for OCR.

### Gaps
- I did not compile anything, so the claim that `DISABLE_UPDATE_CHECKER` is broken comes from reading the code, not from a build log. The two unguarded uses are unambiguous.
- Whether Store policy *requires* compiling out a dormant self-updater, rather than just disabling it, is outside this audit. Store-policy research covers it.

## Self-updater (FlameshotDaemon, tray badge, settings, config keys, spec doc)

### Takeaway
The updater is fully contained in code already guarded by `DISABLE_UPDATE_CHECKER`:
- the GitHub release poll;
- the SHA-256-verified MSI download;
- a `cmd.exe` script that runs `msiexec /i … /passive` and relaunches the app.

The Store edition should compile all of it out and let the Store deliver updates. Both config keys, `checkForUpdates` and `ignoreUpdateToVersion`, must stay declared.

### Cited Findings
- **Feed URL** is compiled in as `GIT_API_URL = https://api.github.com/repos/MichaelNguyen5653/Phramer/releases/latest` ([CMakeLists.txt:31](CMakeLists.txt#L31)) and passed as `FLAMESHOT_APP_VERSION_URL` ([src/CMakeLists.txt:300](src/CMakeLists.txt#L300)).
- **Startup check:** the constructor calls `getLatestAvailableVersion()` when `checkForUpdates()` is true ([src/core/flameshotdaemon.cpp:112-116](src/core/flameshotdaemon.cpp#L112)). The member initializers are at lines 86-90.
- **Polling:** `getLatestAvailableVersion()` GETs the feed and re-arms a 24 h `QTimer::singleShot` ([flameshotdaemon.cpp:328-348](src/core/flameshotdaemon.cpp#L328)).
- **Manual check:** `checkForUpdates()` runs a manual check. On Windows it calls `startUpdateAndRestart()`; elsewhere it calls `QDesktopServices::openUrl` ([flameshotdaemon.cpp:350-372](src/core/flameshotdaemon.cpp#L350)).
- **Windows update flow** ([flameshotdaemon.cpp:374-561](src/core/flameshotdaemon.cpp#L374)):
  - `assetRequest` follows redirects to the CDN (378-384).
  - `startUpdateAndRestart` (387-433) opens a dialog that mentions UAC and offers "Skip this version", which writes `ignoreUpdateToVersion` and clears the badge (420-426).
  - `downloadUpdateInstaller` (435-524) downloads the `.msi.sha256sum` first, then the MSI with a `QProgressDialog`. It verifies SHA-256 and writes `%TEMP%/Phramer-<ver>-win64.msi` (510-512).
  - `applyUpdate` (526-554) writes `%TEMP%/phramer-update.cmd` containing `msiexec /i "<msi>" /passive /norestart`, then `start "" "<applicationFilePath>"`. It runs that script with `QProcess::startDetached("cmd.exe", {"/C", script})` and quits (551-553).
  - `failUpdate` (556-560).
- **Asset discovery:** `handleReplyCheckUpdates` ([flameshotdaemon.cpp:713-771](src/core/flameshotdaemon.cpp#L713)) parses `tag_name`. On Windows it picks assets whose names end in `.msi` and `.msi.sha256sum` (736-745). It emits `newVersionAvailable` (747).
- **Header declarations:** [src/core/flameshotdaemon.h:55-95](src/core/flameshotdaemon.h#L55) declares the slots, the signal and the Windows-only members `m_appLatestMsiUrl`, `m_appLatestShaUrl`, `m_expectedMsiSha256`, `m_updateInProgress` and `m_updateDownloader`.
- **Tray:**
  - [src/widgets/trayicon.cpp:117-150](src/widgets/trayicon.cpp#L117): `showUpdateBadge()` composites an amber dot and sets the tooltip "update available"; there is also `clearUpdateBadge()`.
  - [trayicon.cpp:212-244](src/widgets/trayicon.cpp#L212): the "Check for updates" action, wired to `FlameshotDaemon::checkForUpdates`. On `newVersionAvailable` it is relabelled "Update to version %1 and restart" (Windows) and the badge is shown.
  - [trayicon.cpp:298-300](src/widgets/trayicon.cpp#L298): the action is added to the menu.
  - [trayicon.cpp:320-335](src/widgets/trayicon.cpp#L320): `updateCheckUpdatesMenuVisibility`.
  - [trayicon.cpp:379-385](src/widgets/trayicon.cpp#L379): `startGuiCapture()` calls `showUpdateNotificationIfAvailable`.
- **Capture overlay banner:** `CaptureWidget::showAppUpdateNotification` ([src/widgets/capture/capturewidget.cpp:1806-](src/widgets/capture/capturewidget.cpp#L1806)) creates `UpdateNotificationWidget`. Its Update button opens the release URL in a browser ([src/widgets/updatenotificationwidget.cpp:88-98](src/widgets/updatenotificationwidget.cpp#L88)) and its Ignore button writes `ignoreUpdateToVersion` (lines 80-84).
- **Settings:** the "Automatic check for updates" checkbox is on the General page: `initCheckForUpdates()` is called at [src/config/generalconf.cpp:55-57](src/config/generalconf.cpp#L55), defined at 612-623, with handler `checkForUpdatesChanged` at 238-243 and refresh at 163-165.
- **Config keys:** `checkForUpdates` defaults to true and is guarded ([confighandler.cpp:83-85](src/utils/confighandler.cpp#L83)). `ignoreUpdateToVersion` is an unguarded String default "" ([confighandler.cpp:147](src/utils/confighandler.cpp#L147)) and is documented in [phramer.example.ini:42-43](phramer.example.ini#L42).
- **Spec doc:** [docs/update-notification-spec.md](docs/update-notification-spec.md), 172 lines, has the status "implemented (2026-08-02)". It covers the trigger conditions, including "release has an asset matching `*-win64.msi`" (lines 29-46), the tray badge (47-66), interaction (67-83), the update flow (84-126) and the release-side workflow (127-159). It has no section on Store or packaged installs.
- **Release dependency:** CLAUDE.md and `Windows-release.yml` publish the `.msi` and `.msi.sha256sum` that the updater consumes ([.github/workflows/Windows-release.yml:94-104](.github/workflows/Windows-release.yml#L94)).

### Inferences
- **Store edition plan:**
  - Compile out everything above through `DISABLE_UPDATE_CHECKER`, which `PHRAMER_STORE_BUILD` implies, once that switch is fixed (see the first section).
  - Keep `OPTION("checkForUpdates")` and `OPTION("ignoreUpdateToVersion")` declared. A user who moves from the MSI to the Store edition reads the same `%APPDATA%\phramer\phramer.ini` (see the config-location section), and that file may contain `checkForUpdates=false`.
  - The "Automatic check for updates" checkbox and the tray action disappear.
- **Optional "updates" entry for the Store build:** either open the Store product page (`ms-windows-store://pdp/?ProductId=<StoreId>`), or use `Windows.Services.Store.StoreContext` to query and trigger Store updates. Both are optional; the Store updates packaged apps on its own. Nothing in the codebase does this today, so it would be new code.
- **Side effect worth knowing:** when the Store edition sits next to an older MSI install, the MSI copy's updater keeps working independently. The Store build must not try to detect or remove it.
- **Docs:** `docs/update-notification-spec.md` should gain a short "Store edition" section saying the whole feature is compiled out under `PHRAMER_STORE_BUILD`. CLAUDE.md's "In-app updates" bullet and "Releasing" section need a matching note. README.md:32-40 says "Later updates are offered in the app itself", which is false for the Store edition.

### Gaps
- Whether `StoreContext` works from this Qt Win32 process without extra setup, such as `IInitializeWithWindow` for UI-bearing calls, was not verified. It is out of scope for a code audit.

## ms-screenclip registration (screenclipprotocol, welcome tour, settings, main.cpp)

### Takeaway
The current mechanism cannot work as written inside an MSIX:
- it writes Default Programs keys under **HKLM**;
- it does so through a second copy of the exe elevated with `ShellExecuteExW(..., "runas")`;
- it detects "chosen" by comparing the `UserChoice` ProgId with the literal `"Phramer"`.

The Store edition should declare `ms-screenclip` as a `windows.protocol` extension in the manifest, with parameters `--screenclip "%1"` so that the existing `main.cpp` handling is reused. It should compile out the register/unregister and elevation code, and replace the checkbox and buttons with an "Open Windows Settings" handoff that recognizes the packaged ProgId.

### Cited Findings
- **Registry targets.** `screenclipprotocol.cpp` writes or reads:
  - `HKLM\SOFTWARE\Classes\Phramer` (ProgId and command);
  - `HKLM\SOFTWARE\Phramer\Capabilities`;
  - `HKLM\SOFTWARE\RegisteredApplications`;
  - `HKCU\Software\Classes` (14.x clean-up);
  - `HKCR` (merged read);
  - `HKCU\…\UrlAssociations\ms-screenclip\UserChoice` (read).

  It also cleans up a legacy `HKLM\SOFTWARE\Classes\ms-screenclip` — [src/utils/screenclipprotocol.cpp:22-50](src/utils/screenclipprotocol.cpp#L22). The writes are in `writeRegistration()` (181-221); removal is in `removeRegistration()` (223-256).
- **Elevation.** `runElevated()` starts *this exe* with `lpVerb = L"runas"` and waits up to 120 s ([screenclipprotocol.cpp:52-54, 106-139](src/utils/screenclipprotocol.cpp#L106)). `registerElevated()` and `unregisterElevated()` wrap it (258-276).
- **"Is it the default?"** `isDefault()` compares `UserChoice\ProgId` against `"Phramer"` ([screenclipprotocol.cpp:173-179](src/utils/screenclipprotocol.cpp#L173)). `isRegistered()` requires the HKLM entries ([screenclipprotocol.cpp:156-166](src/utils/screenclipprotocol.cpp#L156)).
- **Settings link.** `openDefaultAppsSettings()` opens `ms-settings:defaultapps?registeredAppMachine=Phramer`, or `ms-settings:defaultapps` when not registered ([screenclipprotocol.cpp:278-290](src/utils/screenclipprotocol.cpp#L278)).
- **Arguments.** The header defines `--register-screenclip`, `--unregister-screenclip` and `--screenclip`, plus `isRecordingRequest()` ([src/utils/screenclipprotocol.h:26-38](src/utils/screenclipprotocol.h#L26)). The registered command shape is `"<exe>" --screenclip "%1"` ([screenclipprotocol.cpp:145-154](src/utils/screenclipprotocol.cpp#L145)).
- **main.cpp:**
  - the elevated-copy entry points run before anything else (`argc==2` register/unregister) — [src/main.cpp:328-341](src/main.cpp#L328);
  - the activation path reads the link before the CLI parser. Recording links go to `SnippingTool::startRecording()`; everything else becomes `gui` — [src/main.cpp:342-359](src/main.cpp#L342).
- **Welcome tour.** `buildScreenClipPage()` has a "Register Phramer as MS-SCREENCLIP (Administrator privileges required)" checkbox ([src/widgets/welcometour.cpp:503-583](src/widgets/welcometour.cpp#L503), text at 538-541). `registerScreenClipIfChecked()` calls `registerElevated()` and handles Succeeded, Cancelled and Failed ([welcometour.cpp:962-1009](src/widgets/welcometour.cpp#L962)). `advance()` blocks on it ([welcometour.cpp:941-952](src/widgets/welcometour.cpp#L941)). The pages are added under `Q_OS_WIN` at [welcometour.cpp:385-388](src/widgets/welcometour.cpp#L385).
- **Settings → Advanced:**
  - `initScreenClipProtocol()` builds a status label, a "Windows Settings" button and a Register/Unregister button with the UAC shield ([src/config/generalconf.cpp:1229-1259](src/config/generalconf.cpp#L1229));
  - `updateScreenClipRow()` (1261-1295);
  - `toggleScreenClipRegistration()` (1297-1345);
  - a stale-state refresh in `changeEvent` (120-130);
  - row creation (89-93).
- **Editor hint.** The empty-editor hint shows "Print Screen" when `PrintScreenKey::isSnippingDisabled() || ScreenClipProtocol::isDefault()` ([src/widgets/editor/editorwindow.cpp:341-352](src/widgets/editor/editorwindow.cpp#L341)).
- **Registry virtualization in MSIX:**
  - "Only keys under *HKLM\Software* are part of the package … Writes to keys or values not part of the package are allowed as long as the user has permission";
  - "All writes under *HKCU* are copied on write to a private per-user, per-app location".

  Source: [Understanding how packaged desktop apps run on Windows](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes).
- **Manifest support.** Packaged apps declare URI handlers with `windows.protocol` / `uap:Protocol Name=…`. `uap3:Protocol` adds a `Parameters` attribute — [uap:Protocol](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-uap-protocol), [uap3:Protocol](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-uap3-protocol).
- **Tests.** The only test in this area is `tests/editor/tst_screenclipuri.cpp`, which tests `isRecordingRequest()` ([tests/editor/tst_screenclipuri.cpp](tests/editor/tst_screenclipuri.cpp)).

### Inferences
- **Store build — compile out:**
  - `writeRegistration`, `removeRegistration`, `runElevated`, `registerElevated` and `unregisterElevated`;
  - the `--register-screenclip` and `--unregister-screenclip` branches in `main.cpp`.

  An HKCU per-user ProgId write would be virtualized and invisible to the shell. An HKLM write needs elevation, which a Store app should not request. Self-elevating a packaged app is also a Store-policy question.
- **Store build — keep** the `--screenclip` activation path in `main.cpp:342-359`. Declare it in the manifest roughly as follows (needs validation):
  ```xml
  <uap3:Extension Category="windows.protocol" Executable="phramer.exe" EntryPoint="Windows.FullTrustApplication">
    <uap3:Protocol Name="ms-screenclip" Parameters="--screenclip &quot;%1&quot;"/>
  </uap3:Extension>
  ```
  The shell would then list "Phramer" as a candidate handler with no registry writes, so `isRegistered()` becomes "always true when packaged".
- **Store build — `isDefault()`.** The literal comparison with `"Phramer"` will fail for a packaged handler, because MSIX protocol ProgIds have the auto-generated `AppX…` form (this needs verification on a test machine). A robust replacement:
  1. call `IApplicationAssociationRegistration::QueryCurrentDefault(L"ms-screenclip", AT_URLPROTOCOL, AL_EFFECTIVE, &progId)`;
  2. read `HKCR\<progId>\Application\AppUserModelID`;
  3. compare it with `PackageIdentity::appUserModelId()`.
- **Store build — Settings link.** Replace `registeredAppMachine=Phramer` with the packaged equivalent, believed to be `ms-settings:defaultapps?registeredAUMID=<AUMID>` (verify), or fall back to `ms-settings:defaultapps`.
- **Store build — welcome tour and settings row.** In the welcome tour, swap the elevation checkbox for a "Choose Phramer in Windows Settings" link. In Settings → Advanced, drop the Register/Unregister button (`m_screenClipButton`) and keep only the status label and the "Windows Settings" button.
- **Side-by-side installs.** With both editions installed, Settings could list two "Phramer" handlers: the MSI's HKLM `RegisteredApplications` entry and the package's protocol declaration. Consider giving the manifest protocol a distinct `DisplayName` such as "Phramer (Store)".

### Gaps
- I found no authoritative Microsoft Learn statement on whether a third-party MSIX may declare an `ms-` prefixed scheme such as `ms-screenclip`, or whether Store certification accepts it. Both searches returned only the generic protocol schema. This is the biggest open risk for the feature.
- The exact query parameter for opening Default apps on a packaged app (`registeredAUMID`) was not verified.

## Print Screen key (printscreenkey and callers)

### Takeaway
`PrintScreenKey::disableSnipping()` writes `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled = 0`. Inside an MSIX that HKCU write is copy-on-write into the package's private hive, so:
- Windows never sees it;
- the app's own `isSnippingDisabled()` then reads its private copy and reports success.

The UI will claim "done" while Print Screen still opens Snipping Tool. The Store edition should instead send the user to the Settings page that holds the toggle.

### Cited Findings
- **Implementation:** `isSnippingDisabled()` reads `PrintScreenKeyForSnippingEnabled` (default 1) and `disableSnipping()` writes 0 then re-reads ([src/utils/printscreenkey.cpp:9-31](src/utils/printscreenkey.cpp#L9)). The header has a doc comment that still says "Flameshot", which is internal and not user-visible ([src/utils/printscreenkey.h:7-22](src/utils/printscreenkey.h#L7)).
- **Callers:**
  - `FlameshotDaemon::showWelcomeMessage()` (Windows-only, first run, `showWelcomeMessage` key) offers "take over that key" and calls `disableSnipping()` ([src/core/flameshotdaemon.cpp:123-127, 130-172](src/core/flameshotdaemon.cpp#L130));
  - `ShortcutsWidget` constructor → `checkPrintScreenForcesSnipping()` → `disablePrintScreenKeyForSnipping()` ([src/config/shortcutswidget.cpp:46-48, 304-347](src/config/shortcutswidget.cpp#L304)). It also adds a "Print Screen" row to the shortcut table when disabled (260-263);
  - the editor hint ([src/widgets/editor/editorwindow.cpp:348](src/widgets/editor/editorwindow.cpp#L348)).
- **Config keys:** `ignorePrntScrForcesSnipping` and `showWelcomeMessage` (Windows-only OPTIONs) at [src/utils/confighandler.cpp:187-196](src/utils/confighandler.cpp#L187). The settings checkbox "Show the first-run welcome dialog again" is at [generalconf.cpp:1213-1227](src/config/generalconf.cpp#L1213).
- **MSIX behaviour:** "All writes under *HKCU* are copied on write to a private per-user, per-app location." Lifting this needs the `unvirtualizedResources` *restricted* capability, which Microsoft says "is designed for certain types of desktop PC games … It is not intended to be used for other scenarios" — [desktop-to-uwp-behind-the-scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes); [App capability declarations, restricted capabilities](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities); [Flexible virtualization](https://learn.microsoft.com/windows/msix/desktop/flexible-virtualization).

### Inferences
- **Store build:**
  - keep the read (`isSnippingDisabled()`). Reads fall through to the real hive when the app has no private copy, so it reports the true state as long as the app never writes.
  - compile out the write (`disableSnipping()`). In `showWelcomeMessage()` and `checkPrintScreenForcesSnipping()`, replace the Yes path with opening Windows Settings → Accessibility → Keyboard, believed to be `ms-settings:easeofaccess-keyboard` (verify). The "restart required" text stays relevant.
- The "registry could not be changed" error paths ([flameshotdaemon.cpp:165-171](src/core/flameshotdaemon.cpp#L165), [shortcutswidget.cpp:327-331](src/config/shortcutswidget.cpp#L327)) become dead code in the Store build.
- Global hotkeys go through QHotkey and `RegisterHotKey` (`GlobalShortcutFilter`, [flameshotdaemon.cpp:687-690](src/core/flameshotdaemon.cpp#L687)). These are not registry-based and should be unaffected by packaging.

### Gaps
- Not verified on a device: whether a packaged full-trust app's HKCU read of `Control Panel\Keyboard` hits the private hive after a write. The documented copy-on-write semantics imply it does.
- The Settings deep-link URI for the Print Screen toggle was not verified.

## Launch at startup (autostart)

### Takeaway
Autostart writes `HKCU\…\CurrentVersion\Run\Phramer = <applicationFilePath>` and an `HKCU\…\App Paths\phramer.exe` entry. In MSIX both writes are virtualized, so they have no effect. The path is also version-specific (`WindowsApps\<PackageFullName>`). On top of that, `applyDefaultStartupLaunch()` turns autostart on silently on first run for installed builds. The Store edition needs a `windows.startupTask` manifest extension plus the `Windows.ApplicationModel.StartupTask` API behind the same `startupLaunch` setting.

### Cited Findings
- **Implementation:** `verifyLaunchFile()` reads `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` value `Phramer` and compares it with `applicationFilePath()` ([src/utils/confighandler.cpp:27-44](src/utils/confighandler.cpp#L27)). `writeLaunchEntry()` on Windows writes the Run value and `App Paths\phramer.exe\Path` ([confighandler.cpp:389-467](src/utils/confighandler.cpp#L389), Windows branch 439-466).
- **Config and setters:** the `startupLaunch` OPTION defaults to **true** ([confighandler.cpp:90-92](src/utils/confighandler.cpp#L90)). The getter and setter are `startupLaunch()` and `setStartupLaunch()` (347-366).
- **Default-on behaviour:** `applyDefaultStartupLaunch()` is compiled only for `Q_OS_WIN && !USE_PORTABLE_CONFIG`, writes the key once and enables the Run entry by default ([confighandler.cpp:368-387](src/utils/confighandler.cpp#L368); declaration [confighandler.h:195-201](src/utils/confighandler.h#L195)). It is called from `FlameshotDaemon::start()` ([flameshotdaemon.cpp:181-183](src/core/flameshotdaemon.cpp#L181)).
- **UI and CLI:**
  - Settings → General: "Launch in background at startup" ([generalconf.cpp:45, 145, 255-258, 671-680](src/config/generalconf.cpp#L671));
  - CLI: `phramer config --autostart true|false` ([src/main.cpp:497-500, 849-852](src/main.cpp#L849));
  - tray greeting "Hello, I'm here!" when `showStartupLaunchMessage` is set ([src/widgets/trayicon.cpp:90-98](src/widgets/trayicon.cpp#L90)).
- **StartupTask:** packaged desktop apps declare `windows.startupTask` with `Executable`, `EntryPoint="Windows.FullTrustApplication"`, `TaskId` and `Enabled`. Microsoft's wording:
  - "If **RequestEnableAsync** is called from a packaged desktop app, no user-consent dialog is shown. Desktop apps can set their startup tasks to **Enabled** in the manifest";
  - "the user must either launch the app at least once …";
  - "If a user disables a task, you can't programmatically re-enable it."

  Sources: [StartupTask class](https://learn.microsoft.com/uwp/api/windows.applicationmodel.startuptask?view=winrt-28000), [Integrate your desktop app with packaging extensions](https://learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-extensions#start-your-application-in-different-ways), [desktop:StartupTask](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-desktop-startuptask).

### Inferences
- **Store build — branch inside `ConfigHandler`.** When `PackageIdentity::isPackaged()`, `writeLaunchEntry` and `verifyLaunchFile` go through `StartupTask::GetAsync(L"PhramerStartup")` with `RequestEnableAsync()`, `Disable()` and `State`. Run the WinRT calls off the GUI STA, or with `.get()` on an MTA worker, mirroring `snippingtool.cpp:39-58`. `applyDefaultStartupLaunch()` should become a no-op when packaged.
- **Store build — manifest default.** Declare `Enabled="true"` only if the default-on behaviour is still wanted. That is a product decision, and it is also visible to Store reviewers and users in Task Manager.
- **Store build — `DisabledByUser`.** Map `StartupTaskState::DisabledByUser` to an unchecked, disabled checkbox with a hint: "Turn it back on in Settings > Apps > Startup". The app cannot re-enable it.
- **Store build — `startupLaunch` key.** Keep the key as the user's intent, never remove it, and read the real state from the StartupTask API.
- **Daemon mode.** The startup task's `Executable` must be `phramer.exe` with no arguments, so that `argc == 1` selects daemon mode ([src/main.cpp:375-376](src/main.cpp#L375)).
- **Side-by-side risk.** If the MSI build is also installed with its Run entry, two copies race for the KDSingleApplication lock at logon (see the single-instance section).

### Gaps
- Whether the `startupLaunch` default (true) plus `Enabled="true"` passes Store review was not researched (policy scope).

## Restart, welcome tour, video handoff, file handoff, and external launches

### Takeaway
- **Restart** relaunches by exe path. That works when packaged, but launching through the AUMID is more robust.
- **Welcome tour** needs its Windows-only screen-clip page reworked. Its video page and the Snipping Tool handoff work as-is, and the documented `ms-screenclip://capture/video` link becomes available because the app now has package identity.
- **File handoff** is unaffected.
- **"Open With"** writes to `%TEMP%` and hands the path to another app, which may break if Temp writes are virtualized.

### Cited Findings
- **Restart:**
  - the tray "&Restart" action sets `requestRestart()` and quits ([src/widgets/trayicon.cpp:246-252](src/widgets/trayicon.cpp#L246));
  - `main()` relaunches with `QProcess::startDetached(QCoreApplication::applicationFilePath(), {})` after the KDSingleApplication scope ends ([src/main.cpp:380-439](src/main.cpp#L380));
  - the flag lives in [src/core/flameshot.cpp:631-639](src/core/flameshot.cpp#L631).
- **Daemon spawn:** `launchDaemonWithCapture()` spawns `phramer.exe` from the same directory with `PHRAMER_CAPTURE_ON_START=1` ([src/main.cpp:84-104](src/main.cpp#L84)); the env var is consumed at 413-420.
- **Foreground rights:** `allowOtherInstancesToForeground()` matches other processes by full exe path ([src/main.cpp:106-144](src/main.cpp#L106)).
- **Welcome tour:**
  - `showIfDue()` shows the tour on a version change ([src/widgets/welcometour.cpp:1104-1128](src/widgets/welcometour.cpp#L1104)), using keys `welcomeTourShownFor` and `welcomeTourDisabled` ([confighandler.cpp:167-173](src/utils/confighandler.cpp#L167));
  - Windows-only pages: `buildScreenClipPage` and `buildVideoPage` ([welcometour.cpp:385-388](src/widgets/welcometour.cpp#L385));
  - it is queued from the daemon constructor ([flameshotdaemon.cpp:118-121](src/core/flameshotdaemon.cpp#L118));
  - the separate first-run `QMessageBox` (`showWelcomeMessage`) is the Print Screen prompt covered above.
- **Video handoff:**
  - `SnippingTool::startRecording/startRecordingAsync` launches `ms-screenclip:capture?mode=default&type=recording&source=Phramer` with `LauncherOptions.TargetApplicationPackageFamilyName("Microsoft.ScreenSketch_8wekyb3d8bbwe")`, falling back to `IApplicationActivationManager::ActivateApplication("Microsoft.ScreenSketch_8wekyb3d8bbwe!App")` ([src/utils/snippingtool.cpp:25-29, 39-58, 85-117](src/utils/snippingtool.cpp#L39));
  - `isAvailable()` uses `FindPackagesByPackageFamily` (63-79);
  - the header notes that the documented `capture/video` link "only answers apps with a package identity" ([src/utils/snippingtool.h:9-20](src/utils/snippingtool.h#L9));
  - callers: [src/core/flameshot.cpp:112-124, 412-438](src/core/flameshot.cpp#L412), [src/widgets/capture/capturemodebar.cpp:67-76](src/widgets/capture/capturemodebar.cpp#L67), [generalconf.cpp:1194-1211](src/config/generalconf.cpp#L1194), [welcometour.cpp:586-649](src/widgets/welcometour.cpp#L586), [main.cpp:353-355](src/main.cpp#L353).
- **File handoff:**
  - `FileHandoff::copyFileToClipboard` puts a `QUrl` list (CF_HDROP) plus "Preferred DropEffect" on the clipboard;
  - `showInFolder` uses `SHOpenFolderAndSelectItems`;
  - the last path is kept in memory only ([src/utils/filehandoff.cpp:33-82](src/utils/filehandoff.cpp#L33));
  - callers: [flameshotdaemon.cpp:276-293, 651-661, 699-702](src/core/flameshotdaemon.cpp#L651), [trayicon.cpp:270-280](src/widgets/trayicon.cpp#L270), [editorwindow.cpp:825-828](src/widgets/editor/editorwindow.cpp#L825), [screenshotsaver.cpp:66, 324](src/utils/screenshotsaver.cpp#L66).
- **Open With:** the "Open With" tool on Windows saves to `QDir::tempPath()` and calls `SHOpenWithDialog` with `OAIF_EXEC` ([src/tools/launcher/openwithprogram.cpp:22-43](src/tools/launcher/openwithprogram.cpp#L22)).
- **Other external launches (`QDesktopServices::openUrl`):**
  - `Flameshot::openSavePath` ([flameshot.cpp:404-410](src/core/flameshot.cpp#L404));
  - `FileHandoff::showInFolder` fallback ([filehandoff.cpp:70](src/utils/filehandoff.cpp#L70));
  - updater URLs ([flameshotdaemon.cpp:367, 396, 752](src/core/flameshotdaemon.cpp#L367));
  - `updatenotificationwidget.cpp:97`;
  - `ms-settings:` links ([screenclipprotocol.cpp:283-289](src/utils/screenclipprotocol.cpp#L283));
  - Imgur only ([uploadlineitem.cpp:42](src/widgets/uploadlineitem.cpp#L42), [imguploaderbase.cpp:161](src/tools/imgupload/storages/imguploaderbase.cpp#L161), [imguruploader.cpp:105](src/tools/imgupload/storages/imgur/imguruploader.cpp#L105)).
- **`QProcess::startDetached` users:** `main.cpp:97-103, 436`, `flameshotdaemon.cpp:551` (updater `cmd.exe`), and the Linux-only launcher (`terminallauncher.cpp:52`, `applauncherwidget.cpp:171`).
- **AppData redirection:** on Windows 10 1903+, "New files and folders created under … Local … Roaming … are redirected to a per-user, per-package private location", while existing files are opened from the real location — [desktop-to-uwp-behind-the-scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes).

### Inferences
- **Restart in the Store build.** Prefer relaunching through the AUMID: `IApplicationActivationManager::ActivateApplication(PackageIdentity::appUserModelId(), …)`. The pattern already exists in `snippingtool.cpp:85-107`. A path-based relaunch is fragile because the package path changes with every version, and the Store may service the package while the app is shutting down. `launchDaemonWithCapture()` (same-directory exe) is fine, because the CLI and GUI live in the same package folder.
- **Video in the Store build.** Optionally try the documented `ms-screenclip://capture/video` first when packaged. The current link is not broken, so this is optional.
- **Open With.** Because `%TEMP%` lies under `AppData\Local`, a newly created temp PNG may be redirected into the package's private store. Explorer and the "Open with" target might then not find it at the path given. Test this; the fallback is to write to a user-visible folder such as the save path.
- **Not affected by packaging:**
  - KDSingleApplication IPC, the tray, global hotkeys;
  - OCR (Windows.Media.Ocr is a WinRT API, so package identity only helps);
  - UI Automation reads and DWM window enumeration (`windowsnap_win.cpp`).

### Gaps
- No primary source was found on whether `%LOCALAPPDATA%\Temp` is excluded from MSIX AppData redirection. The documented table lists "Local" without exceptions.
- Whether a child process started by path from inside the package keeps package identity was not verified.

## Config location, single instance, and phramer-cli.exe

### Takeaway
The installed config path (`QSettings` IniFormat, UserScope → `%APPDATA%\phramer\phramer.ini`) works under MSIX. New files land in the package's private AppData, while an existing MSI user's file is read and written in place. Uninstalling the Store edition deletes a config that the Store edition created.

There are two issues:
- `phramer-cli.exe` has a fixed-size command buffer that is too small for a `WindowsApps` path;
- the CLI needs an `AppExecutionAlias` to be callable at all.

The single-instance key is shared with the MSI build, so the two editions exclude each other.

### Cited Findings
- **Config path:**
  - `ConfigHandler()` uses `QSettings(IniFormat, UserScope, orgName, appName)` unless `USE_PORTABLE_CONFIG`, which instead uses `applicationDirPath()/phramer.ini` ([src/utils/confighandler.cpp:298-307](src/utils/confighandler.cpp#L298));
  - the org and app names are both `phramer` ([src/main.cpp:367-370](src/main.cpp#L367));
  - `migrateLegacyConfig()` copies `flameshot/flameshot.ini` to `phramer/phramer.ini` under AppConfigLocation, or next to the exe when portable. It runs before any ConfigHandler ([src/main.cpp:290-324, 372-373](src/main.cpp#L299)).
- **History** is hard-coded to `QDir::homePath() + "/AppData/Roaming/phramer/history/"` on Windows ([src/utils/history.cpp:13-14](src/utils/history.cpp#L13)). It is only used by the Imgur upload history (ENABLE_IMGUR).
- **Config file exposure:** export and import read and write `configFilePath()` ([generalconf.cpp:260-305](src/config/generalconf.cpp#L260)).
- **Single instance:**
  - KDSingleApplication key `"com.phramer.Phramer"` in `main()` ([src/main.cpp:389-395](src/main.cpp#L389)) and for every IPC send ([flameshotdaemon.cpp:200, 215, 241, 264, 285](src/core/flameshotdaemon.cpp#L200));
  - `guiMutexLock()` uses `QSharedMemory("com.phramer.Phramer-" APP_VERSION)` ([src/main.cpp:176-191](src/main.cpp#L176));
  - KDSingleApplication v1.2.1 on Windows uses a `QLocalServer` named pipe plus a `QLockFile` at `QDir::tempPath()/<socketName>.lock` — [kdsingleapplication_localsocket.cpp @ v1.2.1](https://raw.githubusercontent.com/KDAB/KDSingleApplication/v1.2.1/src/kdsingleapplication_localsocket.cpp).
- **phramer-cli.exe:**
  - a separate console target ([src/CMakeLists.txt:88-101](src/CMakeLists.txt#L88)), installed next to `phramer.exe` (336-339);
  - `windows-cli.cpp` builds the command line with `int cmdSize = 32 + sizeof(directory) + sizeof(args);` — `sizeof` of a `std::wstring` object, not its length — and then calls `swprintf(cmd, cmdSize, L"\"%s\\phramer.exe\" %s", …)` and `_wpopen` ([src/windows-cli.cpp:16-36](src/windows-cli.cpp#L16)).
- **AppExecutionAlias:** packaged apps expose command-line names through `windows.appExecutionAlias`. "You can only specify a single app execution alias for each application in the package" — [Integrate your desktop app with packaging extensions](https://learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-extensions#start-your-application-in-different-ways).

### Inferences
- **The CLI buffer bug is real for MSIX.** With MSVC x64, `sizeof(std::wstring)` is a small constant (32 bytes in typical implementations), so the buffer is on the order of 96 wide chars. `C:\Program Files\WindowsApps\<Name>_<15.0.0.0>_x64__<13-char hash>\` plus `phramer.exe`, quotes and arguments will often exceed that. `swprintf` with a count then fails or truncates, so `phramer-cli.exe` silently does nothing. Fix it before shipping the CLI in MSIX, and it is worth fixing upstream anyway: compute the length from `.size()` or build a `std::wstring` directly.
- **Manifest for two command names.** To expose both `phramer.exe` and `phramer-cli.exe`, declare two `<Application>` elements, each with one alias. The CLI one gets `uap:VisualElements AppListEntry="none"` so it gets no Start menu tile. Alternatively, ship only the GUI alias and document that stdout needs `phramer-cli`.
- **Config persistence for Store users.** For a Store user with no prior install, `phramer.ini` will physically live under `%LOCALAPPDATA%\Packages\<PFN>\LocalCache\Roaming\phramer\` and will be deleted on uninstall. That is expected Store behaviour, but it differs from the MSI build, where the config survives uninstall.
- **Migration from MSI.** A user moving from the MSI keeps their file. The doc says files are "opened from the real AppData location" when no private copy exists, and "no virtualization for that file occurs".
- **Shared single-instance key.** Because the key `com.phramer.Phramer` is shared, the MSI daemon and the Store daemon cannot both run. That is probably desirable, since both would register the same global hotkeys. However, the lock file under a possibly redirected `%TEMP%` might not be shared, while the named pipe is global, so behaviour with both installed needs testing.

### Gaps
- Behaviour of `QLockFile` and `QLocalServer` across a packaged and an unpackaged process of the same user was not tested.

## Registry, network, and privacy-relevant behaviour inventory

### Takeaway
A default Windows build makes exactly two kinds of network request, both for the updater: GitHub API JSON and GitHub release-asset downloads. Imgur upload is compiled out by default, and there is no telemetry. With the updater compiled out, the Store edition makes **no network requests** in its default configuration.

Registry access is limited to four areas:
- autostart (HKCU Run and App Paths);
- Print Screen (HKCU Control Panel\Keyboard);
- ms-screenclip (HKLM/HKCU/HKCR classes and RegisteredApplications);
- `winlnkfileparse` (an HKCU read).

### Cited Findings
- **Network calls.** `QNetworkAccessManager` is created only in `flameshotdaemon.cpp`, for the update check and downloads ([lines 333-340, 437-439](src/core/flameshotdaemon.cpp#L333)), and in `imguruploader.cpp` (25). The Imgur endpoints are `https://api.imgur.com/3/image` and `https://imgur.com/delete/%1` ([src/tools/imgupload/storages/imgur/imguruploader.cpp:88, 105-106](src/tools/imgupload/storages/imgur/imguruploader.cpp#L88)). These come from a grep of `src/`.
- **Imgur is opt-in at build time.** `option(ENABLE_IMGUR … OFF)` ([CMakeLists.txt:115-119](CMakeLists.txt#L115)), and no Windows workflow enables it (grep of `.github/`). The Imgur sources are added only `if (ENABLE_IMGUR)` ([src/tools/CMakeLists.txt:12-22](src/tools/CMakeLists.txt#L12), [src/widgets/CMakeLists.txt:24-33, 54-62](src/widgets/CMakeLists.txt#L24)). The config still declares `uploadClientSecret` with a default Imgur client ID unconditionally ([confighandler.cpp:155](src/utils/confighandler.cpp#L155)).
- **Proxy:** `QNetworkProxyFactory::setUseSystemConfiguration(true)` ([src/main.cpp:370](src/main.cpp#L370)).
- **Telemetry:** no telemetry or analytics code was found (grep of `src/` for `QNetworkAccessManager`, `QNetworkRequest` and URLs).
- **Registry access** (grep for `NativeFormat`, `HKEY_`, `Reg*`):
  - `confighandler.cpp:36-38, 440-447` — Run and App Paths;
  - `printscreenkey.cpp:10, 18, 24`;
  - `screenclipprotocol.cpp:27-49, 69-246`;
  - `winlnkfileparse.cpp:141-146` — HKCU read, used by the app-launcher tool.
- **Local data-handling features** to mention in a privacy policy (none transmit data):
  - OCR via Windows.Media.Ocr ([src/tools/CMakeLists.txt:80-94](src/tools/CMakeLists.txt#L80));
  - UI Automation text reading of the captured window (`ocr/uiatextreader.*`) and its agreement check (CLAUDE.md, "OCR");
  - DWM window enumeration (`utils/windowsnap_win.cpp`);
  - clipboard writes;
  - saving to the user's save path.

### Inferences
- A Store privacy policy for `PHRAMER_STORE_BUILD` can state: no network access, no telemetry, captures stay local unless the user copies, saves or shares them, and OCR runs on-device. The manifest could then omit the `internetClient` capability. Full-trust apps are not sandboxed by capabilities, so this is declarative only.
- If the Store build ever enables Imgur, the privacy policy must change. The Imgur client ID default in config is public upstream data, not a secret.

### Gaps
- None for the code inventory. The Store privacy-policy requirements themselves are outside this audit.

## Build system and CI: what an MSIX packaging step needs

### Takeaway
MSIX packaging should be a separate step that consumes `cmake --install` output, not a CPack generator change. That keeps the WiX and ZIP logic, the `CPACK_WIX_UPGRADE_GUID` and the package names untouched. The plan:
- configure a new `packaging/msix/AppxManifest.xml.in` with the 4-part version `${PROJECT_VERSION}.0`;
- generate visual assets;
- run `makepri` and `makeappx`;
- add a separate CI job (not a new matrix value) so the existing Windows-pack matrix and its hard-coded MSI/ZIP names stay unchanged.

### Cited Findings
- **Version source.** `set(FLAMESHOT_VERSION 15.0.0)` is the single line the workflows grep ([CMakeLists.txt:4-9](CMakeLists.txt#L4)). `RELEASE_VERSION` overrides it (10-12), and a guard requires `MAJOR.MINOR.PATCH` (14-22). `project(flameshot VERSION …)` is at 33-36, and `APP_BINARY_NAME "phramer"` at 41.
- **CPack:**
  - Windows branches: portable gives a ZIP named `${APP_BINARY_NAME}-${PROJECT_VERSION}-win64`; installed gives `WIX ZIP` with `CPACK_PACKAGE_NAME "Phramer"` and `CPACK_WIX_UPGRADE_GUID "42607922-…"` ([CMakeLists.txt:215-250](CMakeLists.txt#L215));
  - `include(InstallRequiredSystemLibraries)` installs the MSVC runtime DLLs (217);
  - the GPL text lives at `packaging/win-installer/LICENSE/GPL-3.0.txt` (242).
- **Targets and install rules:**
  - `flameshot` target, output `phramer`, with `WIN32_EXECUTABLE` ([src/CMakeLists.txt:75-96](src/CMakeLists.txt#L75));
  - `flameshot-cli` (88-95);
  - install rules: `install(TARGETS flameshot … RUNTIME DESTINATION bin)` and `flameshot-cli` (331-339);
  - windeployqt runs as a POST_BUILD step into `${CMAKE_BINARY_DIR}/windeployqt_stuff` with `--release` only when `CMAKE_BUILD_TYPE MATCHES Release`, otherwise `--debug`, plus `--no-translations --compiler-runtime …`. The QM translations are copied in and installed into `bin` ([src/CMakeLists.txt:395-449](src/CMakeLists.txt#L395)). OpenSSL DLLs are installed only `if (ENABLE_OPENSSL)` (431-444).
- **Version resource:** `data/phramer.rc` carries VERSIONINFO built from the `FLAMESHOT_VERSION_*` defines ([CMakeLists.txt:91-96](CMakeLists.txt#L91), [data/phramer.rc](data/phramer.rc)).
- **Windows-pack.yml:**
  - runs on push to master and `fix*`, PRs and workflow_dispatch;
  - one job with matrix `type: [portable, installer]` on `windows-2022`, VS 17 2022, Qt 6.9.3;
  - `VERSION` comes from grepping CMakeLists (line 90);
  - configures with `-DUSE_PORTABLE_CONFIG=${{ contains(matrix.type,'portable') }}` and `-DENABLE_OPENSSL=ON` (114-122);
  - runs `cpack -G WIX` or `cpack -G ZIP` (129-144), then moves files to the hard-coded `Phramer-<ver>-win64.msi` and `phramer-<ver>-win64.zip` names (153-163) and takes SHA256 (165-177).

  Source: [.github/workflows/Windows-pack.yml](.github/workflows/Windows-pack.yml).
- **Windows-release.yml:**
  - triggers on tag `v*` or dispatch with a `tag` input;
  - derives the version from the tag and validates it;
  - configures with the quoted `"-DRELEASE_VERSION=$version"` and `-DUSE_PORTABLE_CONFIG=OFF`; the comment explains the PowerShell dash-token trap;
  - runs `cpack -G WIX`, globs `*.msi`, writes `<msi>.sha256sum` and publishes `dist/*` with `softprops/action-gh-release@v2`.

  Source: [.github/workflows/Windows-release.yml:63-122](.github/workflows/Windows-release.yml#L63).
- **No prior Store work:** no `packaging/msix` directory and no MSIX, AppX or Store references exist anywhere in the repo (grep, `ls packaging`).
- **Store version rule:** "the last (fourth) section of the version number is reserved for Store use and must be left as 0 … The other sections must be set to an integer between 0 and 65535 (except for the first section, which cannot be 0)" — [App package requirements for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-package-requirements#package-version-numbering).
- **Accepted formats:** the Store accepts `.msix`, `.msixbundle` and `.msixupload`, among others — [App package requirements for MSI/EXE app, FAQ](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msi/app-package-requirements#frequently-asked-questions).
- **makepri:** a manual MSIX conversion needs `makepri.exe createconfig` and `makepri new` when target-size assets are added ([Generating MSIX package components](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-manual-conversion#optional-add-target-based-unplated-assets)).

### Inferences
- **Proposed layout**, all new files, nothing existing changed:
  - `packaging/msix/AppxManifest.xml.in`, configured by CMake. It holds:
    - `@PHRAMER_MSIX_IDENTITY_NAME@`, `@PHRAMER_MSIX_PUBLISHER@` (`CN=…` from Partner Center) and `@PHRAMER_MSIX_PUBLISHER_DISPLAY_NAME@`;
    - `@PHRAMER_MSIX_VERSION@` = `${PROJECT_VERSION}.0`;
    - `TargetDeviceFamily Windows.Desktop MinVersion 10.0.17763.0` (or later);
    - `rescap:runFullTrust`;
    - extensions: `windows.startupTask`, `windows.appExecutionAlias` and `windows.protocol ms-screenclip` (subject to the gap above).
  - `packaging/msix/Assets/`, holding pre-generated PNGs (see the next section).
  - `packaging/msix/make_msix.ps1`, which:
    1. runs `cmake --install build --config Release --prefix stage`;
    2. moves `stage/bin/*` to the package root, or sets `Executable="bin\phramer.exe"`;
    3. deletes debug DLLs (`*d.dll`) and any redistributable installer that windeployqt copied;
    4. copies `AppxManifest.xml` and `Assets`;
    5. runs `makepri createconfig` and `new`, then `makeappx pack /d stage /p Phramer-<ver>-x64.msix`;
    6. optionally runs `signtool` with a test certificate, for sideload testing only. The Store signs submitted packages.
  - A CMake `add_custom_target(msix …)`, added only `if(PHRAMER_STORE_BUILD)`, that calls the script. This keeps `cmake --build` for the MSI and ZIP builds unchanged.
- **Keep CPack untouched.** With `PHRAMER_STORE_BUILD=ON` and `USE_PORTABLE_CONFIG=OFF`, `CMakeLists.txt:223-249` would still set up WiX. That is harmless if `cpack` is not run. Optionally wrap it in `if(NOT PHRAMER_STORE_BUILD)` to prevent accidentally producing an MSI with Store-edition binaries, which would have no updater but the same UpgradeCode. **Never change the GUID.**
- **windeployqt trap.** The `--debug` default for multi-config builds (src/CMakeLists.txt:410-414, and a CLAUDE.md trap) would silently put debug Qt DLLs into the MSIX. The Store CI job must pass `-DCMAKE_BUILD_TYPE=Release`, as both existing workflows already do, and the script should assert that no `Qt6*d.dll` is present.
- **CI — Windows-pack.yml.** Add a separate job, e.g. `windows-msix`. Do not add `store` to the existing matrix, because steps 129-197 branch only on `installer` and `portable` and hard-code names. The job:
  - reuses Install Qt and Locate Visual Studio;
  - configures with quoted arguments (PowerShell trap): `cmake -B build -G "Visual Studio 17 2022" -A x64 -DCMAKE_BUILD_TYPE=Release -DUSE_PORTABLE_CONFIG=OFF "-DPHRAMER_STORE_BUILD=ON" "-DPHRAMER_MSIX_PUBLISHER=$env:MSIX_PUBLISHER"`;
  - builds, runs `cmake --build build --config Release --target msix`, and uploads the `.msix` as an artifact.

  Publisher and identity strings are public and belong in repository variables (`vars.*`), not hard-coded.
- **CI — Windows-release.yml.** Add a second job that builds the `.msix` from the same tag with `"-DRELEASE_VERSION=$version"`. Upload it as a workflow artifact for Partner Center submission, or optionally submit with the Microsoft Store CLI. Attaching it to the GitHub release is harmless to the in-app updater, which only matches `.msi` and `.msi.sha256sum` suffixes ([flameshotdaemon.cpp:739-743](src/core/flameshotdaemon.cpp#L739)). Still, it is cleaner not to publish Store binaries publicly.
- **Versioning.** The version series is shared: `15.0.0` maps to MSIX `15.0.0.0`. CLAUDE.md's "Versions must strictly increase" rule now also protects Store updates, and the Store rejects a fourth component other than 0. A Store-only hotfix therefore needs a PATCH bump that also moves the MSI version.
- **Build reproducibility.** QHotkey is fetched at `GIT_TAG master` ([CMakeLists.txt:171-178](CMakeLists.txt#L171)), so Store builds are not reproducible. Pin it to a commit.

### Gaps
- I did not verify which Windows SDK version, with `makeappx.exe` and `makepri.exe`, is present on the `windows-2022` runner image.
- I did not verify whether `windeployqt --compiler-runtime` drops `vc_redist.x64.exe` into the deploy directory with Qt 6.9.3. If it does, exclude it from the MSIX.
- Whether to use the VCLibs framework dependency (`Microsoft.VCLibs.140.00.UWPDesktop`) or app-local CRT DLLs was not researched.

## Visual assets available vs. needed

### Takeaway
The repository has square PNG renders of the Phramer icon at 16, 24, 32, 48, 64, 128, 256 and 512 px, a multi-size `.ico` and a 256 px `Phramer.png` plus `Phramer.svg`. It has none of the MSIX-specific assets. Square44x44 (scale and targetsize variants), Square150x150, Wide310x150 (optional), StoreLogo 50x50 and their scale variants must be generated, ideally from `Phramer.svg`.

### Cited Findings
- **Measured from the PNG headers** in `data/img/app/`:
  - `appicon-{16,24,32,48,64,128,256,512}.png`, at exactly those sizes;
  - `Phramer.png` 256×256;
  - `Phramer.svg`;
  - `appicon.ico` with frames 16/24/32/48/64/128/256 at 32 bpp;
  - leftover upstream art: `flameshot.png` 128, `org.flameshot.Flameshot.png` 128, `org.flameshot.Flameshot-1024.png` 1024, `flameshot.monochrome-1024.png`, `flameshot.ico`, `flameshot.svg`, `flameshot.mask.*`;
  - `printscreen-key.png` 425×327 and `keyboard.svg`.
- **What the app uses:**
  - `graphics.qrc` embeds only `appicon-*.png`, `printscreen-key.png` and `keyboard.svg` from `img/app` ([data/graphics.qrc:3-14](data/graphics.qrc));
  - `GlobalValues::appIcon()` builds from `appicon-16…512` ([src/utils/globalvalues.cpp:24-36](src/utils/globalvalues.cpp#L24));
  - the exe icon resource is `appicon.ico` ([data/phramer.rc:3](data/phramer.rc));
  - the WiX product icon is `appicon.ico` ([CMakeLists.txt:232](CMakeLists.txt#L232)).
- **Required MSIX sizes:**
  - Square44x44Logo: 44, 55, 66, 88, 110, 132, 176 px;
  - Square150x150Logo: 150, 188, 225, 300 … 600 px;
  - Wide310x150Logo: 310×150 … 1240×600;
  - StoreLogo: 50, 63, 75, 100 … 200 px.

  "At minimum, provide assets at 100%, 200%, and 400% scale for the `Square44x44Logo` and `Square150x150Logo` entries, plus the target-size variants". Target-size icons are listed at 16, 24, 32, 48 and 256 px — [Construct your Windows app's icon](https://learn.microsoft.com/windows/apps/design/iconography/app-icon-construction#how-icon-sizes-relate-to-the-msix-manifest).
- **Unplated variants:** `.targetsize-NN_altform-unplated` variants are used for the taskbar and similar surfaces and require regenerating the PRI with makepri — [Generating MSIX package components](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-manual-conversion#optional-add-target-based-unplated-assets).

### Inferences
- **Direct reuse as targetsize variants:** `appicon-16/24/32/48/256.png` map to `Square44x44Logo.targetsize-{16,24,32,48,256}[_altform-unplated].png`.
- **Must be generated:**
  - Square44x44Logo at scale-100 (44), scale-200 (88) and scale-400 (176);
  - Square150x150Logo at scale-100, 200 and 400 (150, 300, 600), with padding (Microsoft suggests the glyph at roughly 50% of the tile);
  - StoreLogo at scale-100, 200 and 400 (50, 100, 200);
  - optionally Wide310x150Logo (310×150, 620×300) and Square71x71Logo.

  Rendering from `Phramer.svg`, for example with Inkscape or rsvg in a script, gives clean results. Downscaling `appicon-512.png` also works. Commit the generated PNGs under `packaging/msix/Assets/` so CI needs no SVG toolchain.
- The Store listing also needs screenshots and a 1:1 store logo (at least 300×300) uploaded in Partner Center. Those are listing assets, not package assets, and none exist in the repo.

### Gaps
- Exact Partner Center listing-image requirements were not researched here.

## Licensing and branding (GPL, third-party, remaining "Flameshot" strings and icons)

### Takeaway
The project is GPL-3.0. All fetched dependencies are GPL-3-compatible:
- KDSingleApplication: MIT;
- QHotkey: BSD-3-Clause;
- QtColorWidgets: LGPLv3+ with an exception allowing inclusion under any GNU licence;
- Qt 6: LGPLv3, dynamically linked.

User-visible "Flameshot" residue is small but real. The German translation of the tray "&About" entry reads "&Über Flameshot", and the `.ts` files are generally stale. Upstream Flameshot art files remain in the repo but are not shipped in the Windows binary.

### Cited Findings
- **Licence files:** `LICENSE` is the GNU GPL v3 text ([LICENSE:1-2](LICENSE)). The GPL text is also at `packaging/win-installer/LICENSE/GPL-3.0.txt`, used as the WiX licence ([CMakeLists.txt:242](CMakeLists.txt#L242)).
- **Dependencies fetched at configure time:**
  - QtColorWidgets from `gitlab.com/mattbas/Qt-Color-Widgets` at commit `5d52e907…` ([CMakeLists.txt:64-79](CMakeLists.txt#L64)). Its README at that commit says "LGPLv3+, See COPYING. As a special exception, this library can be included in any project under the terms of any of the GNU liceses" — [Qt-Color-Widgets README @5d52e907](https://gitlab.com/mattbas/Qt-Color-Widgets/-/raw/5d52e907e50dc88cf969b41cea44665ff6c475b1/README.md). GitLab's API auto-detects a different licence ("GPL-2.0 with classpath exception"), so the README and COPYING take precedence.
  - KDSingleApplication `v1.2.1` from GitHub ([CMakeLists.txt:152-157](CMakeLists.txt#L152)), "available under the terms of the MIT license" — [KDSingleApplication README @v1.2.1](https://raw.githubusercontent.com/KDAB/KDSingleApplication/v1.2.1/README.md).
  - QHotkey from `github.com/flameshot-org/QHotkey` at `master` ([CMakeLists.txt:171-178](CMakeLists.txt#L171)): "Copyright (c) 2016, Felix Barz", BSD 3-clause terms — [QHotkey LICENSE](https://raw.githubusercontent.com/flameshot-org/QHotkey/master/LICENSE).
- **Static linking:** dependencies are built as static libraries (`BUILD_SHARED_LIBS OFF`, `KDSingleApplication_STATIC ON`) — [CMakeLists.txt:50, 146](CMakeLists.txt#L50).
- **Attribution in the binary:** `phramer.rc` LegalCopyright reads "Phramer. Based on Flameshot, Copyright (C) 2017-2020 flameshot.org. GPL-3.0-or-later." ([data/phramer.rc:36](data/phramer.rc)). README.md:13 describes the project as a fork of Flameshot.
- **About window:**
  - `infowindow.ui` sets the window icon to `:/img/app/flameshot.png`, which is not in `graphics.qrc`, so it is a missing resource ([src/widgets/infowindow.ui:17-18](src/widgets/infowindow.ui));
  - the version label's placeholder text is "Flameshot v" (line 102). It is overwritten at runtime by `GlobalValues::versionInfo()` "Phramer …" ([src/widgets/infowindow.cpp:20-21](src/widgets/infowindow.cpp#L20), [src/utils/globalvalues.cpp:18-22](src/utils/globalvalues.cpp#L18)).
- **Translations** (scripted scan of `data/translations/*.ts`):
  - 39 `.ts` files contain "Flameshot" inside `<translation>` text;
  - `Internationalization_en.ts` has 14 `<source>` strings containing "Flameshot" and 0 containing "Phramer", so the catalogues predate the rename and most entries no longer match current `tr()` source strings;
  - the only current, matching source with a "Flameshot" translation found was `"&About"` → `"&Über Flameshot"` in `de_DE` (tray menu, [trayicon.cpp:206](src/widgets/trayicon.cpp#L206)).
  - The scan was a heuristic match on source text, not a runtime check.
- **Internal identifiers** that say flameshot (target, classes, `FLAMESHOT_*` macros) are deliberately kept per CLAUDE.md and are not user-visible. Leftover upstream art in `data/img/app/flameshot*` and `org.flameshot.*` is referenced only by Linux and macOS install rules ([src/CMakeLists.txt:41-52, 341-393](src/CMakeLists.txt#L41)), not by the Windows build.

### Inferences
- **GPL source availability.** Store distribution of a GPL-3.0 binary needs the corresponding source offer, for example a link to the GitHub repo and tag in the Store description and the About box. It also needs the licence text, which can be shipped in the package (copy `GPL-3.0.txt` into the MSIX root) and linked from the listing.
- **Third-party notices.** QtColorWidgets is statically linked into a GPL-3 program, which its exception permits. Ship a `THIRD-PARTY-NOTICES.txt` with the MIT and BSD notices for KDSingleApplication and QHotkey, plus Qt's LGPL notice and the OpenSSL notice if OpenSSL is ever shipped. The BSD and MIT licences require reproducing the notices in binary distributions. No such file exists in the repo today.
- **Branding clean-up before submission:**
  - (a) fix or remove the stale `de_DE` "&About" translation, or regenerate the `.ts` catalogues with `GENERATE_TS` ([src/CMakeLists.txt:167-171](src/CMakeLists.txt#L167));
  - (b) point `infowindow.ui`'s window icon at `appicon-*.png` or drop it;
  - (c) keep "Based on Flameshot" attribution, which GPL notices and honesty require, but make sure the Store name, logo and screenshots contain no Flameshot marks.
- **Reproducibility.** Pin QHotkey's `GIT_TAG` to a commit for reproducible Store builds.

### Gaps
- Whether "Flameshot" is a registered trademark, or whether its project has a trademark policy, was not researched (legal scope).
- The OpenSSL licence version for the vcpkg build used by Windows-pack (`ENABLE_OPENSSL=ON` at Windows-pack.yml:120) was not checked. The release workflow does not enable OpenSSL.

## Tests that exist and what they cover

### Takeaway
No automated test covers the updater, autostart, the Print Screen registry, config persistence or packaging. The only test touching an affected area checks `ScreenClipProtocol::isRecordingRequest()`. The tests are opt-in CMake targets and no CI workflow builds them.

### Cited Findings
- **Test targets:**
  - `tests/editor/`: `tst_canvasgeometry`, `tst_flowpacking`, `tst_hintplacement`, `tst_screenclipuri`, `tst_windowsnap`;
  - `tests/tools/`: `tst_exportsignals`, `tst_frostedfill`, `tst_toolmove`, `tst_toolpicking`, `tst_toolsizewheel`;
  - `tests/ocr/tst_ocr.cpp`;
  - shell scripts `tests/action_options.sh` and `tests/path_option.sh` (directory listing).
- **Screen-clip test:** `tst_screenclipuri.cpp` tests only which `ms-screenclip` links count as recording requests ([tests/editor/tst_screenclipuri.cpp](tests/editor/tst_screenclipuri.cpp)).
- **Build switches:** tests build only with `-DBUILD_OCR_TESTS=ON`, `-DBUILD_TOOL_TESTS=ON` or `-DBUILD_EDITOR_TESTS=ON` ([CMakeLists.txt:182-202](CMakeLists.txt#L182)).
- **CI:** only `build_cmake.yml`, which is workflow_dispatch-only per CLAUDE.md, runs `ctest` ([.github/workflows/build_cmake.yml:73-74](.github/workflows/build_cmake.yml#L73)), and it does not pass any `BUILD_*_TESTS` flag (grep).

### Inferences
- **Cheap, valuable tests to add with the Store work:**
  - (a) a ConfigHandler test asserting that `checkForUpdates` and `ignoreUpdateToVersion` stay recognised under every flag combination, which guards the "never remove a shipped key" rule;
  - (b) a unit test for the `windows-cli.cpp` command-line builder with a long `WindowsApps`-style path;
  - (c) a configure-only CI check that `-DPHRAMER_STORE_BUILD=ON -DUSE_PORTABLE_CONFIG=ON` fails, and that `-DDISABLE_UPDATE_CHECKER=ON` compiles. Today it does not, as described in the first section.
- **Manual test matrix:** packaged behaviour (StartupTask, protocol activation, virtualized HKCU, side-by-side with the MSI) can only be tested on a Windows machine with a sideloaded, test-signed MSIX. Plan a checklist rather than automation.

### Gaps
- None beyond the absence of tests noted above.

## Docs that must change

### Takeaway
Six documents describe behaviour the Store edition changes:
- CLAUDE.md
- README.md
- docs/update-notification-spec.md
- docs/Releasing.md (still the upstream checklist)
- phramer.example.ini (its comments on updates)
- SECURITY.md (no change needed)

### Cited Findings
- **CLAUDE.md** describes:
  - the in-app updater ("In-app updates" bullet);
  - the release flow and the MSI/ZIP naming traps;
  - the USE_PORTABLE_CONFIG trap;
  - autostart via `applyDefaultStartupLaunch`, which is implied by the daemon section;
  - ms-screenclip registration ("ms-screenclip handler — registered as `phramer.exe --screenclip "%1"`").

  Source: [CLAUDE.md](CLAUDE.md), the Fork Features and Releasing sections.
- **README.md** "Installation" (lines 32-40) says to download the `.msi` and that "Later updates are offered in the app itself" ([README.md:32-40](README.md#L32)).
- **docs/Releasing.md** is still the upstream Flameshot checklist, referencing Debian, RPM, flatpak and snap ([docs/Releasing.md](docs/Releasing.md)).
- **docs/update-notification-spec.md** has no Store section (see above).

### Inferences
- **CLAUDE.md:**
  - add a "Store edition" subsection under Build & Run and Releasing covering the `PHRAMER_STORE_BUILD` flag, the forced `USE_PORTABLE_CONFIG=OFF`, the MSIX target and the 4-part version;
  - add a Trap entry on HKCU virtualization ("registry writes from the Store build are invisible to Windows");
  - add a Trap entry on the CLI buffer.
- **README.md:** add a Microsoft Store install option.
- **docs/update-notification-spec.md:** add a note that the Store edition compiles the feature out.

### Gaps
- None.
