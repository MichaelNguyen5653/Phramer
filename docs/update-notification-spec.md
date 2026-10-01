# Update Notification System — design spec

**Status:** implemented (2026-08-02) as specified, with one consolidation:
the confirmation of section 5 step 3 is merged into the initial dialog of
section 4, so the user answers one prompt, not two. The UAC warning lives
there.

Goal: when a new version exists, make that visible on the tray icon and let
the user update from the tray menu, instead of being sent to a browser to
download and run an installer by hand.

## 1. What exists today

Worth stating, because most of the machinery is already there and this
proposal is mostly UI plus an install step.

- `FlameshotDaemon::getLatestAvailableVersion()` GETs the releases feed at
  startup and every 24 hours, gated on the `checkForUpdates` config flag.
- `handleReplyCheckUpdates()` parses `tag_name`, compares it against
  `APP_VERSION`, and on a newer version emits `newVersionAvailable` and
  stores the `.msi` asset URL in `m_appLatestUrl`.
- `TrayIcon` listens for that signal and relabels its menu entry to
  "Download version X"; clicking it opens the URL in a browser.
- `ignoreUpdateToVersion` lets a user permanently dismiss one version.

So detection, scheduling, the config flag and the dismissal mechanism need
no work. What is missing is the visual indicator and doing the install.

## 2. Trigger condition

An update is *available* when all of these hold:

1. `checkForUpdates` is enabled, and
2. a check returned a `tag_name` parsing to a version greater than
   `APP_VERSION`, and
3. that version is greater than `ignoreUpdateToVersion`, and
4. the release has an asset matching `*-win64.msi`.

Condition 4 is new and matters: without an installer asset there is nothing
to apply, and the UI should stay silent rather than offer an action that
cannot complete.

State lives on `FlameshotDaemon` (it already holds `m_appLatestVersion` and
`m_appLatestUrl`) and is published through the existing
`newVersionAvailable` signal, extended to carry the asset URL and size.

## 3. Tray icon indicator

A yellow badge composited onto the bottom-right quadrant of the existing
tray icon, drawn at runtime rather than shipped as a second icon set, so it
stays correct if the logo changes again.

- Render the normal icon into a `QPixmap` at the tray's device pixel ratio.
- Draw a filled circle of diameter ~40% of the icon, inset to the
  bottom-right corner, in a fixed amber (`#FFC107`) with a thin outline in
  the window background colour so it reads on both light and dark taskbars.
- Set the composed pixmap with `QSystemTrayIcon::setIcon`.
- Update the tooltip to "Flameshot v2 — update available (X.Y.Z)".

The badge is applied when the update becomes available and cleared if the
user dismisses the version. It must be recomputed when the tray icon is
rebuilt, so the composition belongs in `TrayIcon`, not at the call site.

Rejected: a separate pre-badged `.ico`. It doubles the artwork to maintain
and would silently go stale.

## 4. Interaction

Right-click menu gains one entry directly above **About**, visible only
when an update is available:

```
Update to X.Y.Z and restart
```

Replacing today's "Download version X" entry. Left-clicking the tray icon
keeps its current behaviour; the badge is informational there.

Choosing it opens a small modal with the version, the download size, and a
link to the release notes, plus **Update now** / **Later** / **Skip this
version**. "Skip this version" writes `ignoreUpdateToVersion` and clears the
badge — reusing what already exists.

## 5. Update flow

Five steps, each of which can fail without leaving the app broken.

1. **Download.** `QNetworkAccessManager` fetches the `.msi` to
   `QStandardPaths::TempLocation`. A modal shows a progress bar with a
   cancel button, driven by `downloadProgress`. Failure or cancel deletes
   the partial file and returns to idle.
2. **Verify.** The release also carries a `.msi.sha256sum` — the release
   workflow already produces one. Download it, compute `QCryptographicHash`
   over the installer, compare. On mismatch, delete the file and report a
   failure; do not offer to run it. This is the step that makes automatic
   updating defensible without code signing.
3. **Confirm.** Warn that Flameshot will close, and that Windows will show a
   UAC prompt (the MSI installs per-machine). Cancelling here is safe: the
   verified file is kept for the next attempt.
4. **Apply.** Launch `msiexec /i <path> /passive /norestart` detached, then
   quit, reusing the restart plumbing already added for the tray Restart
   entry — the intent flag on `Flameshot`, with the relaunch performed in
   `main()` after the single-instance lock has been released. `/passive`
   shows progress but needs no interaction beyond the UAC prompt.
5. **Restart.** The MSI's `CPACK_PACKAGE_EXECUTABLES` shortcut is not
   launched automatically; instead the quitting process schedules the new
   binary at its installed path. If the install is declined at the UAC
   prompt, nothing has changed and the app simply restarts at the old
   version.

**Failure handling.** Every step reports through the existing tray
notification. No step leaves the application in a state where it is
uninstalled but not reinstalled: `msiexec` performs an in-place upgrade,
which is atomic from the user's point of view.

**Non-admin users — resolved.** The MSI installs per-machine and therefore
requires elevation. The alternatives were examined and rejected: a
per-user-scope MSI needs no elevation, but a per-user package cannot
upgrade the per-machine installs already deployed (upgrade GUIDs do not
cross install scopes), so it would strand every existing user with two
copies; anything else amounts to bypassing the OS security boundary.
Decision: proceed per-machine. The update dialog states up front that
Windows will ask for administrator approval and that administrator
credentials can be entered at the UAC prompt by accounts without admin
rights (over-the-shoulder elevation, standard Windows behaviour).

## 6. Release-side workflow

What has to change when publishing a version, given the client above.

**Unchanged.** Tag `vX.Y.Z`, push, `Windows-release.yml` builds the MSI,
generates the checksum and publishes both as release assets. The client
reads `tag_name`, `assets[].browser_download_url` and the checksum asset —
all of which that workflow already produces. **No new manifest file is
needed**; the GitHub releases API is the manifest.

**New requirements on the release process:**

1. **Version must be a strict increase.** The client compares with
   `QVersionNumber`. Re-tagging an existing version, as happened during the
   v14.1.0 work, would leave clients believing they are current.
2. **The checksum asset must be present**, and named
   `<msi-name>.sha256sum`. Step 2 above treats a missing checksum as a
   verification failure and refuses to install. The workflow already emits
   it; it must not be dropped.
3. **Release notes become user-visible.** `generate_release_notes: true` is
   already set; the modal links to `html_url`. Anything written there is
   read by users, so it should be a short human summary rather than raw
   commit subjects. Consider a `CHANGELOG.md` excerpt instead.
4. **Do not publish drafts or prereleases** unless they should reach
   everyone: `/releases/latest` skips drafts and prereleases, so marking a
   release as prerelease is the mechanism for staging one without notifying
   clients.
5. **The upgrade GUID must stay fixed.** `CPACK_WIX_UPGRADE_GUID` is what
   makes `msiexec /i` upgrade in place rather than install a second copy.
   Changing it, as was necessary once when forking away from upstream's
   GUID, breaks the update path for everyone already installed and requires
   a manual uninstall.

## 7. Open questions

- **Code signing.** Unsigned installers trigger SmartScreen even when
  launched by the app. Checksum verification protects against corruption
  and tampering in transit, but not against the warning. Now that the
  repository is public, SignPath's OSS programme would remove it.
- **Silent updating.** Doing this without any user interaction needs
  `/quiet` plus elevation the user has already granted, which realistically
  means a scheduled task or service. Out of scope here.
- **Frequency.** The current 24-hour timer only fires while the daemon is
  running. A machine that reboots daily may check far less often than that
  implies. Worth a check shortly after startup, which is already the
  behaviour.

## Store edition

The Microsoft Store build (`PHRAMER_STORE_BUILD`) compiles this whole
feature out through `DISABLE_UPDATE_CHECKER`: the Store delivers its updates,
and a package cannot run an MSI. The `checkForUpdates` and
`ignoreUpdateToVersion` keys stay declared, so a configuration carried over
from the MSI build loads without an unrecognized-setting error.
