# Runtime behaviour of Phramer as a packaged full-trust (mediumIL, packagedClassicApp) MSIX app

Scope: how each Phramer feature behaves once the existing Qt 6 Win32 binary runs with package identity under MSIX (`uap10:RuntimeBehavior="packagedClassicApp"`, `uap10:TrustLevel="mediumIL"`, `runFullTrust`). Windows 10 21H2+ and Windows 11 x64, as of October 2026.

Repo files read: `src/utils/printscreenkey.cpp`, `src/utils/screenclipprotocol.cpp`, `src/utils/snippingtool.cpp`, `src/main.cpp` (restart, `--screenclip`, `migrateLegacyConfig`, `launchDaemonWithCapture`, `allowOtherInstancesToForeground`), `src/utils/confighandler.cpp` (config path, Run-key autostart), `src/utils/filehandoff.cpp`, `src/core/flameshotdaemon.cpp` (MSI updater), `src/core/globalshortcutfilter.cpp`, `src/utils/screengrabber.cpp`, `src/utils/windowsnap_win.cpp`, `src/tools/ocr/`.

Legend: **[MS]** = official Microsoft documentation; **[Community]** = community source only; **[Verify]** = needs on-device verification.

### Summary verdict table (detail and sources in the sections below)

| Feature | Under MSIX | Supported replacement |
|---|---|---|
| Print Screen key (`HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`) | Officially documented as virtualized (all HKCU). One community report says writes outside `HKCU\Software` reach the real registry. **[Verify]** | Open `ms-settings:easeofaccess-keyboard` and let the user flip the toggle, or verify on device that the write takes effect |
| ms-screenclip registration (HKLM writes via self-elevation) | Breaks or is not Store-acceptable | `uap:Protocol Name="ms-screenclip"` in the manifest. User picks Phramer in Default apps (`ms-settings:defaultapps?registeredAUMID=…`) |
| Default-handler detection (UserChoice ProgId == "Phramer") | Works differently: a packaged ProgId is not "Phramer" | `AssocQueryString(ASSOCSTR_APPID, "ms-screenclip")` compared with your own AUMID |
| Settings INI in `%APPDATA%\phramer` | Works differently: new files are redirected to a per-package location | Accept it, or exclude the directory (restricted capability). Plan the MSI-to-MSIX migration |
| Autostart (HKCU `…\CurrentVersion\Run` + `App Paths`) | Breaks: the HKCU\Software write is virtualized | `windows.startupTask` + `Windows.ApplicationModel.StartupTask` |
| Restart (`QProcess::startDetached(applicationFilePath())`) | Probably works (a child keeps identity) **[Verify]** | `RegisterApplicationRestart`, `AppInstance::Restart` (Windows App SDK), or activate own AUMID |
| In-app MSI updater | Must be disabled in the packaged build | Store updates. `RegisterApplicationRestart` relaunches after an update |
| Global hotkeys, tray, GDI capture, DWM bounds, UIA, Windows.Media.Ocr | No documented MSIX restriction for mediumIL **[Verify]** | No change |
| Snipping Tool recording via `LaunchUriAsync` + TargetApplicationPackageFamilyName | Works. Same API, documented callable from desktop apps | No change (the caller-visibility rule applies) |
| phramer-cli.exe | Needs an execution alias (`desktop4:Subsystem="console"`) to run with identity and keep stdout | `uap5:AppExecutionAlias` |
| Detecting packaged vs not | `GetCurrentPackageFullName` | — |

---

## 1. Registry: HKCU write virtualization and the Print Screen key (`printscreenkey.cpp`)

### Takeaway
Microsoft's documentation says **all** HKCU writes from a packaged full-trust app go to a private per-package hive. That would make the `PrintScreenKeyForSnippingEnabled=0` write invisible to Windows: the toggle reads back as "disabled" inside Phramer (merged view), while the shell still gives PrtScn to Snipping Tool. Real-world evidence suggests only `HKCU\Software` is virtualized and other HKCU keys (e.g. `HKCU\Environment`) reach the real registry. That is unconfirmed, so it **must be verified on device**. The robust, Store-safe design is to stop writing the value and deep-link the user to `ms-settings:easeofaccess-keyboard`.

### Cited Findings
- "All writes under *HKCU* are copied on write to a private per-user, per-app location." Writes under HKCU are "Copied on write to a per-user, per-app private location." **[MS]** — [Understanding how packaged desktop apps run on Windows](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- On the same page, the registry section is headed "applies only to virtualized apps", and only `HKLM\Software` (registry.dat) is part of the package: "Only keys under *HKLM\Software* are part of the package; keys under *HKCU* or other parts of the registry are not." **[MS]** — [same page](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- The default-behaviour table for HKCU at run time: "Writes go to a separate per-app, per-user private hive. For reading, this hive is merged with the unvirtualized HKCU … Keys in the virtualized hive are only visible to the app." For install-time `user.dat`, it speaks specifically of "**HKCU\Software** entries". **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)
- "any entries that your application writes to the HKEY_CURRENT_USER registry hive are placed into a private per-user, per-app location." **[MS]** — [Prepare to package a desktop application](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- Counter-evidence: an MSIX-packaged desktop app (Claude desktop, Windows 11 26200) reported that "HKCU\Environment writes from inside a session are **not** virtualized, so the env vars took effect after an app restart", while AppData writes *were* redirected to `LocalCache`. **[Community]** — [anthropics/claude-code#93152](https://github.com/anthropics/claude-code/issues/93152)
- A WindowsAppSDK discussion participant: "with runFullTrust, my HKEY_CURRENT_USER writes seem to be virtualized". A Microsoft maintainer (jonwis) called virtualization "all-or-nothing" for packaged apps. **[Community/MS staff, GitHub discussion]** — [microsoft/WindowsAppSDK#1096](https://github.com/microsoft/WindowsAppSDK/discussions/1096)
- Tim Mangan (TMurgent) has a post "MSIX: RegistryWriteVirtualization" on this topic. The page could not be retrieved: the proxy blocks tmurgent.com and appdeploynews.com. — [tmurgent.com/TmBlog/?p=4087](https://www.tmurgent.com/TmBlog/?p=4087)

**unvirtualizedResources / ExcludedKeys**
- Windows 11 lets a package declare specific unvirtualized locations: "You can declare only Registry locations that are within **HKCU**", via `<virtualization:RegistryWriteVirtualization><virtualization:ExcludedKeys><virtualization:ExcludedKey>HKEY_CURRENT_USER\…`. It requires `<rescap:Capability Name="unvirtualizedResources"/>`. On pre-Windows 11 only the all-or-nothing `desktop6:RegistryWriteVirtualization=disabled` applies. **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)
- `RegistryWriteVirtualization=disabled`: "Writes to **HKCU** go to the unvirtualized location, are visible to other processes outside the package, and are not cleaned up on app uninstall." **[MS]** — [same](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)
- Store stance on `unvirtualizedResources`: "This capability is designed for certain types of desktop PC games that are published by Microsoft and our partners. It's also needed for apps packaged with external location … It is not intended to be used for other scenarios, because it could compromise the system's ability to uninstall cleanly." **[MS]** — [App capability declarations – restricted capabilities](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- Restricted capabilities need justification on the Partner Center Submission options page and are "highly restricted and subject to additional Store onboarding policy and review". A sideload needs no approval. **[MS]** — [same](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- Community report: AppInstaller refuses to install packages declaring `unvirtualizedResources`, so PowerShell plus developer mode is needed instead. This may be dated. **[Community]** — [WindowsAppSDK#1096](https://github.com/microsoft/WindowsAppSDK/discussions/1096)

**ms-settings deep link for the PrtScn toggle**
- `ms-settings:easeofaccess-keyboard` is the documented URI for the "Keyboard" page under Ease of access / Accessibility. No version restriction is noted. **[MS]** — [Launch Windows Settings – ms-settings URI reference](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings#ms-settings-uri-scheme-reference)
- On Windows 11 the toggle is at Settings > Accessibility > Keyboard, labelled "Use the Print screen key to open screen capture". The label varies across builds: older text says "…open screen snipping". **[MS Q&A, community answers]** — [Microsoft Q&A a/12779044](https://learn.microsoft.com/answers/a/12779044), [Q&A a/8269889](https://learn.microsoft.com/answers/a/8269889), [Q&A a/2293056](https://learn.microsoft.com/answers/a/2293056)
- The value behind the toggle is `PrintScreenKeyForSnippingEnabled` (DWORD, 0 = off) under `HKCU\Control Panel\Keyboard`, which matches `printscreenkey.cpp`. **[MS Q&A answer]** — [Q&A a/12274905](https://learn.microsoft.com/answers/a/12274905)

### Inferences
- `PrintScreenKey::isSnippingDisabled()` reads through the merged view. If the write is virtualized, Phramer will **believe** it succeeded (`disableSnipping()` returns true) while Windows ignores it. The first-run welcome would then report success falsely. This is the worst failure mode, so the packaged build should not trust a read-back.
- `HKCU\Control Panel` is outside `HKCU\Software`. The community data point on `HKCU\Environment` suggests the write may reach the real registry. If it does, it is also **not cleaned up on uninstall**, which leaves PrtScn taken from Snipping Tool after Phramer is removed. The Store may view this as a system-setting change. **[Verify]**
- Recommended packaged behaviour: replace the write with a button that launches `ms-settings:easeofaccess-keyboard` and explains the toggle. Optionally, read the value to show its state; a read of a key that is not virtualized sees the real value. Requesting `unvirtualizedResources` for a non-game app is unlikely to be approved.

### Gaps
- No Microsoft document states explicitly whether HKCU keys **outside** `HKCU\Software` (such as `HKCU\Control Panel`) are virtualized at run time. The official text says "all HKCU". The only counter-evidence is community. On-device test: from the packaged app, write the value, then read it with `reg query` from a normal (unpackaged) cmd and press PrtScn.
- Whether `ms-settings:easeofaccess-keyboard` scrolls to or highlights the PrtScn toggle on Windows 10 21H2/22H2 and Windows 11 is not documented. **[Verify]**

---

## 2. HKLM writes and self-elevation (`screenclipprotocol.cpp` `runElevated` / `writeRegistration`)

### Takeaway
The current ms-screenclip registration elevates its own exe with `ShellExecuteEx(runas)` and writes `HKLM\SOFTWARE\Classes\Phramer`, `HKLM\SOFTWARE\Phramer\Capabilities` and `HKLM\SOFTWARE\RegisteredApplications`. This is not viable for a Store MSIX. Microsoft says the Store won't accept apps that need elevation for any functionality. Self-elevation needs the restricted `allowElevation` capability, which is approved only "under strict criteria". The documentation also conflicts on whether a packaged app's HKLM writes succeed at all. Replace the whole mechanism with a manifest `uap:Protocol` declaration (section 3).

### Cited Findings
- "If you plan on publishing your app to the Microsoft Store, apps that require elevation for any part of their functionality won't be accepted into the Store." **[MS]** — [Prepare to package a desktop application](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- "Any attempt by your application to create an HKLM key, or to open one for modification, will result in an access-denied failure … You will need to find another way … like writing to HKEY_CURRENT_USER (HKCU) instead." **[MS]** — [same](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- Conflicting: "Writes under **HKLM** are allowed as long as a corresponding key/value doesn't exist in the package hive and the user has the correct access permissions (which effectively means this is only available to a Centennial app running elevated)." **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization). "Writes outside the package: Ignored by the OS. Allowed if the user has permissions." **[MS]** — [Behind the scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- `allowElevation`: "enables apps developed by Microsoft partners or enterprise organizations to maintain existing desktop functionality that depends on auto-elevation, either at launch or during runtime. For Microsoft Store submissions, this capability is subject to approval under strict criteria. If you intend to use this capability, contact reportapp@microsoft.com in advance with a detailed justification." **[MS]** — [App capability declarations](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- A Microsoft Q&A answer suggests that with `allowElevation` a packaged app can use the `runas` verb for an on-demand elevated helper, and must explain it in certification notes. **[MS Q&A, not normative]** — [Q&A a/12866242](https://learn.microsoft.com/answers/a/12866242)

### Inferences
- In the packaged build, `registerElevated()`, `unregisterElevated()`, `writeRegistration()`, `removeRegistration()` and the `RegisterArgument`/`UnregisterArgument` branches in `main()` should be compiled out or short-circuited. Their job moves to the manifest.
- Even if HKLM writes succeeded elevated, a registration pointing at `C:\Program Files\WindowsApps\<full name>\phramer.exe` would break on every update, because the package full name contains the version.

### Gaps
- The two Microsoft pages disagree on whether elevated HKLM writes from a packaged app succeed. This is not resolved; it is moot if the manifest route is used.

---

## 3. ms-screenclip protocol under package identity

### Takeaway
`ms-screenclip` is **not** on Microsoft's reserved URI scheme list, so a packaged app can declare `<uap:Protocol Name="ms-screenclip">` (Windows ignores registrations of reserved names; this one is not reserved). Windows does not let an app make itself the default. The user picks Phramer in Settings > Default apps; on Windows 11 the app can deep-link to its own page with `ms-settings:defaultapps?registeredAUMID=<AUMID>`. Detect "am I default" with `AssocQueryString(ASSOCSTR_APPID)`, not the UserChoice ProgId compare in `isDefault()`: a packaged app's ProgId is not "Phramer".

### Cited Findings
- "If your app registers for a reserved association, that registration will be ignored." The reserved URI list includes `ms-settings`, many `ms-settings:*` pages, `ms-appx`, `ms-appdata`, `ms-windows-store`, `mailto`, `http(s)`, `file`, and others. **`ms-screenclip` does not appear.** Doc dated 2025-02-11. **[MS]** — [Reserved file and URI scheme names](https://learn.microsoft.com/en-us/windows/apps/develop/launch/reserved-uri-scheme-names) (source markdown: [MicrosoftDocs/windows-dev-docs](https://github.com/MicrosoftDocs/windows-dev-docs/blob/docs/hub/apps/develop/launch/reserved-uri-scheme-names.md))
- Packaged apps declare protocols in the manifest (`<uap:Extension Category="windows.protocol"><uap:Protocol Name="…">`). "We recommend that you only register for a URI scheme name if you expect to handle all URI launches for that type of URI scheme." The doc also says any app can invoke the scheme, so all parameters must be treated as untrusted. **[MS]** — [Handle URI activation](https://learn.microsoft.com/en-us/windows/apps/develop/launch/handle-uri-activation)
- `uap10:Protocol` has a `Parameters` attribute: "An optional string between 1 and 32767 characters". This is how a full-trust app shapes its activation command line. **[MS]** — [uap10:Protocol](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-uap10-protocol)
- "In general, your app can't select the app that is launched. The user determines which app is launched. More than one app can register to handle the same URI scheme." **[MS]** — [Launch the default app for a URI](https://learn.microsoft.com/windows/apps/develop/launch/launch-default-app)
- `ms-settings:defaultapps?registeredAUMID=<Uri-escaped AUMID>` opens the app's own Default Apps page "when the app was registered with Package Manager using a manifest declaring that the app handles File Types … or URI schemes ([uap:Protocol])". It requires Windows 11 21H2/22H2 with the 2023-04 CU, or later. After an OS upgrade the app "may need to increment its TargetDeviceFamily…MaxVersionTested" (≥ `10.0.22000.1817` / `10.0.22621.1555`) for the deep link to work. **[MS]** — [Launch the Default Apps settings page](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-default-apps-settings)
- `ASSOCSTR_APPID`: "Introduced in Windows 10. The AppUserModelID of the app associated with the file type or URI scheme. This is configured by users in their default program settings." **[MS]** — [ASSOCSTR enumeration](https://learn.microsoft.com/windows/win32/api/shlwapi/ne-shlwapi-assocstr#constants)
- Own AUMID: `GetCurrentApplicationUserModelId` in appmodel.h. **[MS]** — [appmodel.h](https://learn.microsoft.com/windows/win32/api/appmodel/#functions)

### Inferences
- Manifest sketch: `<uap10:Extension Category="windows.protocol"><uap10:Protocol Name="ms-screenclip" Parameters="--screenclip &quot;%1&quot;" /></uap10:Extension>`. This keeps `main()`'s existing `--screenclip "<link>"` parsing unchanged. Alternatively, read the URI from `AppInstance::GetActivatedEventArgs()` (Windows.ApplicationModel, 1809+). Exactly how `%1` substitution behaves in `Parameters` for a protocol is **[Verify]**.
- `isDefault()` reads `UrlAssociations\ms-screenclip\UserChoice\ProgId` == "Phramer". Under MSIX the ProgId is a generated `AppX…` hash, so this always returns false. Replace it with `AssocQueryStringW(ASSOCF_IS_PROTOCOL, ASSOCSTR_APPID, L"ms-screenclip", …)` compared with `GetCurrentApplicationUserModelId()`. `isRegistered()` / `isRegisteredByPhramer()` become trivially true when packaged (the manifest always declares it). **[Verify the ASSOCF flag and output on device]**
- `openDefaultAppsSettings()` should use `registeredAUMID=` when packaged, falling back to plain `ms-settings:defaultapps` on Windows 10 and older Windows 11.
- A protocol declared in the manifest is removed automatically with the package (see section 12). That removes the need for the HKLM cleanup code.
- Two MSI-era registrations can coexist on one machine: HKLM ProgId "Phramer" from an MSI install, plus the MSIX's AppX ProgId. The user could select the MSI copy in Default apps. The packaged build should not try to remove the MSI registration (HKLM, elevation).

### Gaps
- Whether Windows' own PrtScn / Win+Shift+S pipeline resolves `ms-screenclip:` through the user's default-handler choice when that handler is a third-party *packaged* app was not documented anywhere I found. It works for the MSI build today per CLAUDE.md, but must be **[Verify]** for MSIX.
- Whether Microsoft Store certification objects to an app claiming a Windows first-party scheme (`ms-screenclip`) is not covered by any policy I found. Microsoft's guidance only asks that the app "provide the end user with the functionality that is expected".

---

## 4. AppData, %TEMP% and file system (QSettings INI, migration, screenshots, CF_HDROP)

### Takeaway
On Windows 10 1903+ a packaged app's **newly created** files under `%APPDATA%`/`%LOCALAPPDATA%` go to `%LOCALAPPDATA%\Packages\<PFN>\LocalCache\{Roaming,Local}\…`, and only the app sees them. Writes to **existing** files go to the real file. Reads try the private copy first, then the real location. Qt 6.9's QSettings saves atomically through `QSaveFile` (temp file + rename), so Phramer's INI behaviour is a likely trap and needs on-device testing. A fresh MSIX install's `phramer.ini` will be private: invisible to the user at `%APPDATA%\phramer\phramer.ini` and deleted on uninstall. Screenshots saved to Pictures, Desktop or user-chosen folders are **not** virtualized and are visible to Teams/Outlook. `%TEMP%` is reportedly not redirected (community).

### Cited Findings
- "New files and folders created under the following directories are redirected to a per-user, per-package private location: Local, Local\Microsoft, Roaming, Roaming\Microsoft, Roaming\Microsoft\Windows\Start Menu\Programs. In response to a file open command, the OS will open the file from the per-user, per-package location first. If that location doesn't exist, then the OS will attempt to open the file from the real `AppData` location. If the file is opened from the real `AppData` location, then no virtualization for that file occurs." **[MS]** — [Behind the scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- "Modifications to existing **AppData** files is done on the unvirtualized files … Files in the virtualized location are visible only to the app … Apart from **AppData**, the app can write to any location where the user has write access, including other parts of `%userprofile%`." **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)
- "State separation also allows packaged desktop apps to pick up where an unpackaged version of the same app left off." **[MS]** — [Behind the scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- "Writes to files/folders in the app package aren't allowed" (`C:\Program Files\WindowsApps\…` is read-only). **[MS]** — [same](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- Windows 11 can exclude specific AppData directories: `<virtualization:ExcludedDirectory>$(KnownFolder:RoamingAppData)\…`. This needs `unvirtualizedResources` (restricted; section 1). **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)
- Real-world: an MSIX app's writes to `%APPDATA%`/`%LOCALAPPDATA%` "silently redirected to `%LOCALAPPDATA%\Packages\Claude_…\LocalCache\{Roaming,Local}\…` … `Test-Path` succeeds inside the session, but a normal terminal sees nothing." **[Community]** — [anthropics/claude-code#93152](https://github.com/anthropics/claude-code/issues/93152)
- %TEMP%: "When an executable needs to write data to the user temp folder or machine temp folder, it will not be redirected and will be placed directly into the selected location." This is from Advanced Installer's MSIX temp-folder article; I saw only the search snippet because the page is blocked by the proxy. **[Community/vendor]** — [How does MSIX handle the temp folder?](https://www.advancedinstaller.com/msix-temp-folder-access.html)
- Qt 6.9.3 `QSettings` sync writes via `QSaveFile sf(confFile->name); sf.setDirectWriteFallback(!atomicSyncOnly);`, i.e. a new temporary file in the same directory, committed by rename. **[Qt source]** — [qtbase 6.9.3 qsettings.cpp](https://github.com/qt/qtbase/blob/6.9.3/src/corelib/io/qsettings.cpp)
- Repo: the installed build uses `QSettings(IniFormat, UserScope, org, app)`, giving `%APPDATA%\phramer\phramer.ini`. `migrateLegacyConfig()` copies `%APPDATA%\flameshot\flameshot.ini` when `phramer.ini` is absent. A `QFileSystemWatcher` watches the INI. (`src/utils/confighandler.cpp`, `src/main.cpp`)

### Inferences
- **Fresh MSIX install (no MSI history):** `%APPDATA%\phramer\` does not exist, so the directory and INI are created in `LocalCache\Roaming\phramer\phramer.ini`. Users or docs that say "edit `%APPDATA%\phramer\phramer.ini`" will find nothing in Explorer. The real path is `%LOCALAPPDATA%\Packages\<PFN>\LocalCache\Roaming\phramer\`. An "Open config folder" action inside Phramer would work, because the app sees the merged view, but Explorer launched from it shows the real folder. **[Verify]**
- **Upgrade from an MSI install:** the real `%APPDATA%\phramer\phramer.ini` exists and is read via fallback, so settings carry over (matching "pick up where an unpackaged version left off"). But `QSaveFile` creates a *new* temp file in that directory, and new files are redirected. Whether the final rename lands in the real directory or leaves a private copy that shadows the real file is not documented. Expected outcome: after the first save the packaged app works on a private copy and the MSI file goes stale. That is harmless for the packaged app but confusing for users who edit the real file. **[Verify]** If the private copy is a problem, call `QSettings::setAtomicSyncRequired(false)` so writes modify the existing file in place.
- `migrateLegacyConfig()` creates `phramer.ini` new, so the result is private to the package. That is acceptable.
- The `QFileSystemWatcher` on the INI may watch the real path and miss writes to the private copy. **[Verify]**
- **Screenshots** saved to Pictures, Desktop or user-chosen folders: not under AppData, so not virtualized. They stay after uninstall and are visible to every process.
- **Copy-as-file (CF_HDROP, `filehandoff.cpp`)**: it hands over the absolute path of the file just saved to the save path. Teams and Outlook run outside the package and see only real paths, so this works as long as the save path is not under `%APPDATA%`/`%LOCALAPPDATA%`. If a user points the save path into AppData, the new file is private and the paste fails silently in the other app. **[Verify]** `valuehandler.cpp` falls back to `TempLocation` when Pictures/Home are unavailable. If %TEMP% is not redirected (community claim above), it is still visible.
- **Updater temp files** (`flameshotdaemon.cpp` writes the MSI and `phramer-update.cmd` to %TEMP% and runs `cmd.exe` + `msiexec`): see section 9. This must be disabled in the packaged build regardless.

### Gaps
- No Microsoft document describes how a rename-over-existing (`ReplaceFileW`/`MoveFileEx`, as used by QSaveFile) behaves under AppData redirection, nor whether `%LOCALAPPDATA%\Temp` is in the redirected set. The Microsoft list names only "Local", which literally covers `Local\Temp`; the vendor claim conflicts with that reading. Test on device.

---

## 5. Launch at login (current Run-key implementation and the StartupTask replacement)

### Takeaway
Today, autostart writes `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\Phramer` = exe path, plus `HKCU\…\App Paths\phramer.exe\Path` (`ConfigHandler::writeLaunchEntry`). `verifyLaunchFile()` reads it back. `applyDefaultStartupLaunch()` enables it once by default for installed builds. Under MSIX these writes are under `HKCU\Software`, so they are certainly virtualized. Explorer never sees them, so autostart silently stops working, and `verifyLaunchFile()` still reports "enabled" from the merged view. Replace it with a `windows.startupTask` declaration and the `Windows.ApplicationModel.StartupTask` API.

### Cited Findings
- Writes under HKCU are virtualized (see section 1). Install-time `user.dat` covers "**HKCU**\**Software** entries". **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)
- Manifest for packaged desktop apps: `<uap5:Extension Category="windows.startupTask" Executable="MyDesktopApp.exe" EntryPoint="Windows.FullTrustApplication"><uap5:StartupTask TaskId="MyStartupId" Enabled="false" DisplayName="My Desktop App" /></uap5:Extension>`. `Enabled` "May be set to `true` for packaged desktop apps to indicate that the app is enabled for startup without first needing to call RequestEnableAsync." **[MS]** — [StartupTask class](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask)
- "If **RequestEnableAsync** is called from a packaged desktop app, **no user-consent dialog is shown**." (UWP apps get a dialog; desktop apps do not.) **[MS]** — [same](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask)
- "to enable startup functionality, the user must either launch the app at least once, or they must enable startup functionality for the app on the **Startup** page in **Settings**. Once enabled, the user is in control and can change the enabled state … via the Startup page in Settings or the Startup tab in Task Manager." **[MS]** — [same](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask)
- `RequestEnableAsync`: "If the task was disabled by the user using Task Manager, this method will not override their choice and the user must re-enable the task manually." States: `Disabled`, `DisabledByUser`, `DisabledByPolicy`, `Enabled` (also `EnabledByPolicy` exists in the enum). `Disable()` turns it off programmatically. **[MS]** — [same](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask)
- "Users can manually disable your app's startup task by using Task Manager. If a user disables a task, you can't programmatically re-enable it." "The user has to start your application at least one time to register this startup task." **[MS]** — [Integrate your desktop app with packaging extensions](https://learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-extensions#start-your-application-in-different-ways)
- Settings page: `ms-settings:startupapps`. **[MS]** — [ms-settings URI reference](https://learn.microsoft.com/windows/apps/develop/launch/launch-settings#ms-settings-uri-scheme-reference)

### Inferences
- Packaged implementation: in `setStartupLaunch(true)` call `StartupTask::GetAsync(L"PhramerStartup")`, then `RequestEnableAsync()`; for false call `Disable()`. In `startupLaunch()` / `verifyLaunchFile()` read `State()`. When the state is `DisabledByUser` (or `DisabledByPolicy`), the General settings checkbox should be unchecked and disabled, with text pointing to Task Manager or `ms-settings:startupapps`. The `startupLaunch` INI key then only mirrors the state. Per CLAUDE.md the key must not be removed; leave the OPTION in place.
- `applyDefaultStartupLaunch()`'s "on by default" becomes either `Enabled="true"` in the manifest or a first-run `RequestEnableAsync()` (no dialog for desktop apps). Either way it only takes effect after the first launch, per Microsoft.
- The `App Paths` write has no packaged equivalent and is not needed: the startup task's `Executable` is package-relative, and the working directory is not the package dir (see "uses the current working directory" in [Prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)).
- The C++/WinRT calls must not block the GUI thread (STA). Follow the existing `snippingtool.cpp` pattern of a worker thread in an MTA, or use `co_await`.
- An MSI copy's real `HKCU\…\Run\Phramer` entry (pointing to `C:\Program Files\Phramer\…`) remains on machines that had the MSI. If both are installed, both start at login (see section 10).

### Gaps
- Whether a startup task with `Enabled="true"` auto-activates for Store installs on first install, before any launch, is described inconsistently ("must launch at least once"). **[Verify]**

---

## 6. Global hotkeys, system tray, low-level hooks

### Takeaway
I found no MSIX-specific restriction on `RegisterHotKey` (QHotkey, and `globalshortcutfilter.cpp`'s `RegisterHotKey(NULL, …, VK_SNAPSHOT)`), `Shell_NotifyIcon`, or `SetWindowsHookEx` for a **mediumIL** full-trust packaged process. These are ordinary Win32 calls at medium integrity. AppContainer restrictions do not apply. The repo does not use low-level hooks (grep found no `SetWindowsHookEx`).

### Cited Findings
- A full-trust app "has a process that runs with an integrity level of *medium*". `runFullTrust` is what allows such a package to install. **[MS]** — [App capability declarations](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- `uap10:TrustLevel="mediumIL"` vs `appContainer` "determines whether or not your packaged app's process runs inside an app container". **[MS]** — [Behind the scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- If a process calls `SetCurrentProcessExplicitAppUserModelID`, "it may only use the AUMID generated for it by the application model … You can't define custom AUMIDs." **[MS]** — [Prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)

### Inferences
- Hotkeys and tray need no code change. One tray risk: the install path changes with every version (`WindowsApps\<name>_<version>_…`). If Qt or Phramer ever registers the tray icon by `guidItem` (NIF_GUID), Windows ties that GUID to the exe path, so an update could make registration fail. Qt's `QSystemTrayIcon` uses `uID` by default, so this is probably not an issue. **[Verify]**
- Toasts and tray notifications under package identity will be attributed to the package (its display name and logo), not to the exe. Any explicit AUMID calls in the code must be removed. The repo did not show any, but a check is advisable.

### Gaps
- No Microsoft page explicitly lists "RegisterHotKey / Shell_NotifyIcon work unchanged in MSIX". This is inferred from the mediumIL model. **[Verify]**: QHotkey with Print Screen while Snipping Tool also wants it is unchanged from the MSI build.

---

## 7. UI Automation, Windows.Media.Ocr, screen capture and DWM enumeration

### Takeaway
None of these need a capability for a mediumIL packaged app. `QScreen::grabWindow` (GDI BitBlt), `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` enumeration (`windowsnap_win.cpp`), `IUIAutomation` (`uiatextreader.cpp`) and `Windows.Media.Ocr` (`windowsocrengine.cpp`) behave as unpackaged. The `graphicsCapture*` capabilities apply only to `Windows.Graphics.Capture`, which Phramer does not use.

### Cited Findings
- `graphicsCapture`: "required to use the Windows.Graphics.Capture.GraphicsCapturePicker object". `graphicsCaptureProgrammatic` is required to create a `GraphicsCaptureItem` from a WindowId/DisplayId. `graphicsCaptureWithoutBorder` is required for `IsBorderRequired`. These are general-use (not restricted) capabilities. **[MS]** — [App capability declarations – general-use](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#general-use-capabilities)
- The restricted **uiAutomation** capability "allows a UI automation client, such as Narrator, to connect to a UI Automation server or provider. This capability is required to use some APIs in the Windows.Xbox.Media.Capture.Broadcaster namespace". It is aimed at app-container clients and Xbox. **[MS]** — [App capability declarations – restricted](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- "App capabilities … are relevant mostly to *packaged apps that run in an AppContainer*." For Medium IL apps, the main exception is `runFullTrust`, plus privacy-sensitive device capabilities. **[MS]** — [Windows apps: packaging, deployment, and process](https://learn.microsoft.com/windows/apps/get-started/intro-pack-dep-proc#app-capabilities)

### Inferences
- `uiAutomation` is not needed for a mediumIL UIA client reading other windows. UIA's UIPI rules are unchanged: it still cannot read elevated (high-IL) windows, exactly as the MSI build.
- `Windows.Media.Ocr` needs no capability. OCR language packs are system-wide. The OCR worker's own COM/WinRT apartment is unaffected by packaging.
- The GDI/DWM capture path is unchanged. No `graphicsCapture` declaration is needed unless Phramer adopts Windows.Graphics.Capture later.
- Privacy prompts: Windows 11 has `ms-settings:privacy-graphicscaptureprogrammatic` and `ms-settings:privacy-graphicscapturewithoutborder` pages ([ms-settings reference](https://learn.microsoft.com/windows/apps/develop/launch/launch-settings#ms-settings-uri-scheme-reference)). These govern only WGC, not BitBlt.

### Gaps
- No Microsoft page states explicitly that `IUIAutomation` and `Windows.Media.Ocr` need no capability in a packaged mediumIL process. This is inferred from the capability model. **[Verify]** with a sideloaded build.

---

## 8. Launching Snipping Tool's recording overlay (`snippingtool.cpp`) from a packaged app

### Takeaway
It works unchanged. `Launcher::LaunchUriAsync(uri, options)` with `TargetApplicationPackageFamilyName = Microsoft.ScreenSketch_8wekyb3d8bbwe` is documented as callable from desktop apps. Targeting by PFN means a packaged Phramer that is the default `ms-screenclip` handler will not get the link back. The fallback `IApplicationActivationManager::ActivateApplication(AUMID)` also works under identity. One documented caveat: "The calling app must be visible to the user" for LaunchUriAsync.

### Cited Findings
- "This API may also be called from a Windows desktop application … Unless you are calling this API from a Windows desktop application, this API must be called from within an ASTA thread." **[MS]** — [Launcher.LaunchUriAsync](https://learn.microsoft.com/uwp/api/windows.system.launcher.launchuriasync)
- "The calling app must be visible to the user when the API is invoked … When the launch fails for any of the above reasons, the API will succeed and return FALSE." **[MS]** — [same](https://learn.microsoft.com/uwp/api/windows.system.launcher.launchuriasync)
- `TargetApplicationPackageFamilyName`: "The package family name of the target package that should be used to launch a file or URI." In the cases where it is required, "The calling app cannot launch any app that happens to be the default for that URI protocol." **[MS]** — [LauncherOptions.TargetApplicationPackageFamilyName](https://learn.microsoft.com/uwp/api/windows.system.launcheroptions.targetapplicationpackagefamilyname)
- "If Windows attempts to show this [switch apps] prompt for a desktop app, the launch will fail." Some `LauncherOptions` (e.g. `TreatAsUntrusted`) only work in UWP apps. **[MS]** — [Launch the default app for a URI](https://learn.microsoft.com/windows/apps/develop/launch/launch-default-app#call-launchuriasync-to-launch-a-uri)

### Inferences
- The tray menu path (no visible Phramer window) may trip the "calling app must be visible" rule in either the MSI or the MSIX build. The existing `|| launch()` fallback (`ActivateApplication`) covers it. **[Verify]**: whether package identity changes the foreground/visibility check.
- The short-lived ms-screenclip process (`startRecording()`) for Win+Shift+R: under MSIX, protocol activation starts a packaged process with identity. Bouncing to Snipping Tool by PFN cannot loop back to Phramer.

### Gaps
- No Microsoft document addresses a *packaged* caller using `TargetApplicationPackageFamilyName` for a scheme the caller itself is registered for. The documented semantics (target by PFN) imply no loop. **[Verify]**

---

## 9. Restart / relaunch and the in-app updater

### Takeaway
`main()` relaunches `QCoreApplication::applicationFilePath()` (the `C:\Program Files\WindowsApps\…\phramer.exe` path) with `QProcess::startDetached` after the event loop exits. Under MSIX the safer relaunch is one that goes through activation. Options: Windows App SDK `AppInstance::Restart()`, which supports packaged and unpackaged apps; `CoreApplication::RequestRestartAsync`, which requires the app to be foreground and visible; or activating the app's own AUMID through `IApplicationActivationManager` / `shell:AppsFolder\<PFN>!App`, or an execution alias. Whether a plain CreateProcess of the package exe keeps identity is documented inconsistently and must be verified. The MSI-based updater (`applyUpdate`: download MSI to %TEMP%, `cmd.exe` + `msiexec`) must be disabled in the packaged build. Store updates replace it, and `RegisterApplicationRestart` relaunches the app after a Store update.

### Cited Findings
- "**Important:** Simply launching the exe in the build folder will **not** give it identity. The app must be started via AUMID activation or its execution alias … identity is tied to the activation path, not the exe file." This is about a loose-layout registration. **[MS]** — [Debugging with Package Identity (winapp CLI)](https://learn.microsoft.com/windows/apps/dev-tools/winapp-cli/debugging#debugging-scenarios)
- Conversely: "You cannot directly launch executables in your package from a jump list (**with the exception of the absolute path of an app's own .exe**). Instead, register an app execution alias … and set the link target path to the alias." **[MS]** — [Prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- Child processes: `PROC_THREAD_ATTRIBUTE_DESKTOP_APP_POLICY` controls whether children run "inside of the desktop app runtime environment" (`…BREAKAWAY_DISABLE_PROCESS_TREE` 0x02, `…BREAKAWAY_OVERRIDE` 0x04) or outside (`…BREAKAWAY_ENABLE_PROCESS_TREE` 0x01, "the default for processes for which no policy has been set"). **[MS]** — [UpdateProcThreadAttribute](https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute#remarks)
- Real-world: a detached daemon spawned (DETACHED_PROCESS) from an MSIX-packaged client "inherits the package's security identity", which blocked the package's update ("Another program is currently using this file"). **[Community, issue dated 2026-09-28]** — [jordiboehme/crystalline#115](https://github.com/jordiboehme/crystalline/issues/115). Another write-up says children *outside* the package (e.g. `dismhost.exe`) lose identity unless `DESKTOP_APP_POLICY` 0x06 is used. **[Community]** — [HotCakeX wiki](https://github.com/HotCakeX/Harden-Windows-Security/wiki/DISM-API-in-Packaged-Apps-and-the-Mechanics-of-Child-Process-Identity)
- "Any packaged or unpackaged desktop app can terminate and restart itself on command, and have access to an arbitrary command-line string for the restarted instance using the `AppInstance.Restart()` API … a lifted and synchronous version of the UWP `RequestRestartAsync()`." **[MS]** — [Windows App SDK 1.1 release notes](https://learn.microsoft.com/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-1-1#version-11); [Restart API](https://learn.microsoft.com/windows/apps/windows-app-sdk/applifecycle/applifecycle-restart)
- `CoreApplication.RequestRestartAsync`: "The app must be visible and foreground when it calls this API." **[MS]** — [RequestRestartAsync](https://learn.microsoft.com/uwp/api/windows.applicationmodel.core.coreapplication.requestrestartasync)
- Store updates: "If your application is open when users install an update to it, the application closes. If you want that application to restart after the update completes, call the RegisterApplicationRestart function in every process that you want to restart … your application has 30 seconds to close." **[MS]** — [Integrate your desktop app with packaging extensions](https://learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-extensions#start-your-application-in-different-ways). WER only auto-restarts on crash/hang after 60 s of uptime. **[MS]** — [RegisterApplicationRestart](https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-registerapplicationrestart#remarks)
- "Avoid starting command utilities such as PowerShell and Cmd.exe … This could block your application from submission to the Microsoft Store because all apps submitted to the Microsoft Store must be compatible with Windows 10 S." **[MS]** — [Prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)

### Inferences
- The current relaunch happens *after* `qApp->exec()` returns, from the still-packaged process. Its child is the package's own exe, so per the community evidence it most likely inherits identity. **[Verify]**: call `GetCurrentPackageFullName` in the relaunched process.
- The most robust packaged relaunch, without adding the Windows App SDK runtime dependency, is to activate the app's own AUMID with `IApplicationActivationManager::ActivateApplication`. `snippingtool.cpp` already has the code shape; pass `GetCurrentApplicationUserModelId()`. Do it after the single-instance lock is released, preserving the CLAUDE.md ordering. `AppInstance::Restart()` terminates synchronously, so it would bypass Phramer's "quit first, release lock, then relaunch" sequence; it is a worse fit. `RequestRestartAsync` needs a foreground window, which a tray app usually lacks.
- `launchDaemonWithCapture()` (phramer-cli starts phramer.exe from the same directory) and `allowOtherInstancesToForeground()` (matching by full path) still work inside the package: same directory, same canonical path.
- `FlameshotDaemon::applyUpdate` and the update checker must be gated off when `GetCurrentPackageFullName` succeeds. In-place `msiexec` cannot update an MSIX, and the cmd.exe use is a certification risk.

### Gaps
- The direct-launch identity question (CreateProcess of `WindowsApps\…\phramer.exe` from outside the package, e.g. by an old Run key or a user's .lnk) has conflicting Microsoft statements (the winapp CLI doc vs the jump-list exception). **[Verify]**

---

## 10. Single-instance IPC (KDSingleApplication), MSI side-by-side, and phramer-cli.exe

### Takeaway
A mediumIL packaged process does not get AppContainer object-namespace isolation. KDSingleApplication's local-socket/named-pipe lock (`com.phramer.Phramer`) is therefore shared with any unpackaged Phramer in the same session. Packaged daemon and packaged CLI can talk, but **an MSI copy and the MSIX copy would also treat each other as the same instance**: the second one forwards its command to whichever started first. phramer-cli.exe should be exposed through a `uap5:AppExecutionAlias` with `desktop4:Subsystem="console"` (or `uap10:Subsystem`), which runs it with identity and keeps the calling terminal's stdout.

### Cited Findings
- `uap5:AppExecutionAlias` has `desktop4:Subsystem` / `uap10:Subsystem` = `console` | `windows`. **[MS]** — [uap5:AppExecutionAlias](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-uap5-appexecutionalias)
- An app launched through an execution alias "inherits this terminal's stdin/stdout/stderr. You still get package identity". AUMID-activated console apps get no console and "print nothing". **[MS]** — [winapp CLI usage](https://learn.microsoft.com/windows/apps/dev-tools/winapp-cli/usage#shell-completion)
- Alias declaration: `<uap3:Extension Category="windows.appExecutionAlias" EntryPoint="Windows.FullTrustApplication"><uap3:AppExecutionAlias><desktop:ExecutionAlias Alias="…exe" /></…>`. "If multiple apps register for the same alias, the system will invoke the last one that was registered". Users can disable an alias in Settings > App execution aliases. **[MS]** — [Integrate your desktop app with packaging extensions](https://learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-extensions#transition-users-to-your-app)
- "If you have two apps in the same package, you can do inter-process communication between them." In-process extensions loaded into processes outside the package are not supported. **[MS]** — [Prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- Repo: the Unix code path has a KDSingleApplication note in `main.cpp`. The Windows instance name is `com.phramer.Phramer` (`main.cpp` line ~390).

### Inferences
- Alias plan: `phramer.exe` (windows subsystem) and `phramer-cli.exe` (console subsystem) as two `<Application>` entries, or one Application with two aliases. A user's existing scripts calling `phramer-cli` keep working if the alias name matches. The Store alias takes precedence over the MSI's `C:\Program Files\Phramer\bin` on PATH depending on PATH order (aliases live in `%LOCALAPPDATA%\Microsoft\WindowsApps`). **[Verify]**
- Side-by-side MSI + MSIX is a support hazard: the same lock name, two Run/StartupTask entries, two registered ms-screenclip handlers, and two configs (real vs private INI). Plan to detect an MSI install and tell the user to uninstall it, rather than coexist. The packaged app cannot uninstall a per-machine MSI without elevation. Alternatively, derive a different lock name when packaged. Then both would run, both would register PrtScn hotkeys, and only one would win `RegisterHotKey`.
- When the alias-launched CLI starts the daemon (`launchDaemonWithCapture`), the daemon is a child of an identity-bearing process inside the package and should inherit identity. **[Verify]**

### Gaps
- I did not read KDSingleApplication's Windows naming code; it is fetched at configure time and not in the tree. Whether the name includes session/user only, or also the exe path, is unconfirmed. If it includes the path, the MSI and MSIX copies would not collide. **[Verify in `_deps/kdsingleapplication-src`]**

---

## 11. Detecting at runtime whether the app is packaged

### Takeaway
Use `GetCurrentPackageFullName()`. It returns `APPMODEL_ERROR_NO_PACKAGE` (15700) when the process has no identity. One binary can then branch for config, autostart, the protocol, the updater, and restart. `snippingtool.cpp` already includes `<appmodel.h>`.

### Cited Findings
- "When a desktop application is running as an unpackaged application without package identity, this function returns an error … If the function succeeds, it means: Your app is packaged in an MSIX package [and] running on Windows 10, version 1709 (build 16299) or later." **[MS]** — [Detect package identity and runtime context](https://learn.microsoft.com/windows/msix/detect-package-identity)
- Return codes: `APPMODEL_ERROR_NO_PACKAGE` "The process has no package identity", and `ERROR_INSUFFICIENT_BUFFER`. **[MS]** — [GetCurrentPackageFullName](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getcurrentpackagefullname)
- Microsoft points to the Inside MSIX post "Is This a Packaged Process?" for canonical samples. **[MS]** — [Packaging overview](https://learn.microsoft.com/windows/apps/package-and-deploy/packaging/)

### Inferences
- Call it with a zero-length buffer and test for `ERROR_INSUFFICIENT_BUFFER` (packaged) vs `APPMODEL_ERROR_NO_PACKAGE` (unpackaged). Cache the result in one helper (e.g. `utils/packageidentity`). Pair it with `GetCurrentPackageFamilyName` / `GetCurrentApplicationUserModelId` for the protocol default check (section 3) and self-activation (section 9).
- The CMake option `USE_PORTABLE_CONFIG` stays orthogonal. An MSIX build must use the installed-config branch (`USE_PORTABLE_CONFIG=OFF`), because `applicationDirPath()` is read-only under WindowsApps.

### Gaps
- None material.

---

## 12. Uninstall cleanliness and what is left behind

### Takeaway
Removing the package deletes the package files, the private AppData (`LocalCache`), the private HKCU hive, and manifest-declared extensions such as the protocol, startup task and aliases. Anything Phramer wrote to **real** locations survives: screenshots in Pictures or other folders, any HKCU value that is not virtualized (possibly `PrintScreenKeyForSnippingEnabled=0`, section 1), and any pre-existing MSI-era files and registry it modified in place (e.g. the real `%APPDATA%\phramer\phramer.ini` if it was edited directly).

### Cited Findings
- "When a package is uninstalled by the user, all files and folders located under `C:\Program Files\WindowsApps\<package_full_name>` are removed, as well as any redirected writes to `AppData` or the registry that were captured during the packaging process." **[MS]** — [Behind the scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- "All writes are kept during package upgrade, and deleted only when the app is removed entirely." **[MS]** — [same](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)
- For HKCU and AppData at run time: "When the app is uninstalled, the virtualized entries are removed." With virtualization disabled, writes "are not cleaned up on app uninstall". Writes outside AppData ("other parts of `%userprofile%`") are real and not tracked. **[MS]** — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization)

### Inferences
- If the PrtScn write reaches the real registry, uninstalling Phramer leaves PrtScn not opening Snipping Tool, with no Phramer to answer it. That is another reason to delegate the toggle to Settings (section 1), where the user owns the change.
- Writes to existing files and keys in the real AppData are not undone on uninstall: modifications to existing AppData files go to the real file. So an MSI user's original `phramer.ini` remains, possibly modified by the packaged app (section 4).
- Settings are lost on uninstall/reinstall of the MSIX (private INI deleted). Users who expect settings to survive a reinstall will be surprised. Consider an export/import, or document it.

### Gaps
- Whether Windows removes the user's Default-apps choice for `ms-screenclip` cleanly, or leaves a dangling `UserChoice` for an AppX ProgId that makes PrtScn fail until reset, is not documented. **[Verify]**
