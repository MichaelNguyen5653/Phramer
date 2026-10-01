# MSIX manifest and toolchain for a Store-ready Phramer (full-trust Qt 6 / C++ Win32)

Scope note: researched 2026-10-01. Primary sources are Microsoft Learn (fetched through the Microsoft Learn MCP server). doc.qt.io, forum.qt.io, develop.kde.org, invent.kde.org and kate-editor.org were blocked by the egress proxy, so Qt/KDE experience is taken from the GitHub mirrors of KDE Craft and qtbase source, and from search snippets where noted.

Repo facts used for tailoring (read from `/home/user/Phramer`, not external sources):
- Two Windows binaries: `phramer.exe` (target `flameshot`, `WIN32_EXECUTABLE`) and `phramer-cli.exe` (target `flameshot-cli`, `/SUBSYSTEM:CONSOLE`), both `install(... RUNTIME DESTINATION bin)` (`src/CMakeLists.txt` lines 82-95, 332-339).
- `windeployqt` runs as a POST_BUILD step: `windeployqt ${BINARIES_TYPE} --no-translations --compiler-runtime --no-system-d3d-compiler --no-quick-import --dir build/windeployqt_stuff phramer.exe`, then `install(DIRECTORY windeployqt_stuff/ DESTINATION bin)`. `BINARIES_TYPE` is `--release` only when `CMAKE_BUILD_TYPE` matches Release, otherwise `--debug` (`src/CMakeLists.txt` 395-449). OpenSSL DLLs are copied from `$OPENSSL_ROOT_DIR/bin` when `ENABLE_OPENSSL`.
- Release CI: `runs-on: windows-2022`, `jurplel/install-qt-action@v4` with Qt 6.9.3 `win64_msvc2022_64` + `qtimageformats`, sets `VCINSTALLDIR` via vswhere, configures `-G "Visual Studio 17 2022" -A x64 -DCMAKE_BUILD_TYPE=Release -DUSE_PORTABLE_CONFIG=OFF "-DRELEASE_VERSION=$version"`, builds `--config Release`, then `cpack -G WIX` (`.github/workflows/Windows-release.yml`).
- Icons available: `data/img/app/Phramer.svg`, `Phramer.png`, `appicon-{16,24,32,48,64,128,256,512}.png`, `appicon.ico`.
- Code paths that collide with MSIX: ms-screenclip registration writes `HKLM\SOFTWARE\Classes`, `HKLM\SOFTWARE\RegisteredApplications` and `HKCU\Software\Classes` via an elevated copy of the app (`src/utils/screenclipprotocol.cpp`, `src/main.cpp:329`); autostart writes `HKCU\...\CurrentVersion\Run` (`src/utils/confighandler.cpp:37,441`); Print Screen toggle writes `HKCU\Control Panel\Keyboard` (`src/utils/printscreenkey.cpp:10`); the in-app updater writes a batch file and runs `msiexec /i` through `cmd` (`src/core/flameshotdaemon.cpp:528-544`).

## 1. AppxManifest.xml for a full-trust Win32 app: Identity, Properties, Dependencies, Resources, Application, capabilities, VisualElements

### Takeaway
A hand-written manifest needs `Identity`, `Properties` (DisplayName, PublisherDisplayName, Logo), `Resources`, `Dependencies/TargetDeviceFamily Name="Windows.Desktop"`, `rescap:Capability Name="runFullTrust"`, and an `Application` with `Executable` plus either `EntryPoint="Windows.FullTrustApplication"` or (when MinVersion ≥ 10.0.19041.0) `uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL"`. The Store version must be `Major.Minor.Build.0`: Phramer's `14.1.x` maps directly to `14.1.x.0`.

### Cited Findings
- Microsoft's manual-packaging template: `<Package xmlns=".../foundation/windows10" xmlns:uap=".../uap/windows10" xmlns:uap10=".../uap/windows10/10" xmlns:rescap=".../foundation/windows10/restrictedcapabilities">` with `Identity Name/Version/Publisher/ProcessorArchitecture`, `Properties` (DisplayName, PublisherDisplayName, Description, Logo), `Resources/Resource Language`, `TargetDeviceFamily Name="Windows.Desktop" MinVersion MaxVersionTested`, `<rescap:Capability Name="runFullTrust"/>`, and `<Application Id Executable uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL">` with `uap:VisualElements DisplayName Description Square150x150Logo Square44x44Logo BackgroundColor`. — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- "For desktop apps that you create a package for, always set the `Name` attribute to `Windows.Desktop`." `ProcessorArchitecture` may be `x64`, `x86`, `arm`, `arm64`, or `neutral`. — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- "For a package that contains one or more full-trust apps, you'll need to declare the `runFullTrust` restricted capability." — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- If you reserved the name in the Store, take `Name` and `Publisher` from Partner Center; for sideloading, the Publisher must match the signing certificate subject. — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- The manifest "must include some specific info about your account and your app", found under Product management → View app identity details. "Values in the manifest are case-sensitive. Spaces and other punctuation must also match." — [App package requirements for MSIX app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- Version rule: "the last (fourth) section of the version number is reserved for Store use and must be left as 0 when you build your package… The other sections must be set to an integer between 0 and 65535 (except for the first section, which cannot be 0)." The Store always serves the highest applicable version, and you can submit packages in any order. — [App package requirements for MSIX app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- Store package limits: 25 GB maximum per .msix; block map hashes must be SHA2-256. — [App package requirements for MSIX app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- The `uap10` namespace (`uap10:RuntimeBehavior`, `uap10:TrustLevel`) was introduced in Windows 10 2004 (build 19041). With `MinVersion="10.0.19041.0"` or higher you "should use the uap10 attributes in preference". `Executable, EntryPoint="windows.fullTrustApplication"` is equivalent to `packagedClassicApp` + `mediumIL`. Specifying both forms is redundant, and "it's an error if they contradict". Extension elements inherit activation attributes from the parent `Application` when they don't set their own. — [Application element remarks](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-f-application#remarks)
- `packagedClassicApp` means "a WinUI app (Windows App SDK) or a Desktop Bridge app (Centennial)". `win32App` is for other Win32 apps, including packages with external location. `mediumIL` is full trust. — [Understanding how packaged desktop apps run](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-behind-the-scenes#types-of-desktop-app)
- "Console apps will always be multi-instanced and must explicitly declare **SupportsMultipleInstances**." — [Application element remarks](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-f-application#remarks)
- `VisualElements` is required even for a hidden app. `AppListEntry="none"` keeps an Application out of the installed-apps list (shown in the external-location identity package guidance). — [Grant package identity by packaging with external location](https://learn.microsoft.com/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps#create-a-package-manifest-for-the-identity-package)
- KDE Craft's AppX template (used to ship Kate, Okular and other apps in the Store) uses `EntryPoint="Windows.FullTrustApplication"`, `TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.17763.0"` with the comment "Qt requires 1809 which maps to 17763", `<Resource Language="en-us" />`, `BackgroundColor="transparent"`, and `uap:DefaultTile` with Wide310x150/Square310x310. — [KDE Craft AppxManifest.xml template](https://raw.githubusercontent.com/KDE/craft/master/bin/Packager/AppxManifest.xml)
- The GitHub windows-2025 runner has "Windows Software Development Kit 10.1.26100.7705". — [actions/runner-images Windows2025-Readme](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)
- Supported Store language codes include `en`, `en-us`, etc. Codes not on the list "may cause delays or failures in certification". — [App package requirements for MSIX app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)

### Complete example AppxManifest.xml tailored to Phramer

Placeholders `@...@` are filled by CI, for example with CMake `configure_file` or a PowerShell `-replace`. `IDENTITY_NAME`, `PUBLISHER` and `PUBLISHER_DISPLAY_NAME` must be copied exactly from Partner Center → Product identity.

```xml
<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap3="http://schemas.microsoft.com/appx/manifest/uap/windows10/3"
  xmlns:uap5="http://schemas.microsoft.com/appx/manifest/uap/windows10/5"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:desktop="http://schemas.microsoft.com/appx/manifest/desktop/windows10"
  xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap uap3 uap5 uap10 desktop desktop4 rescap">

  <!-- Version: FLAMESHOT_VERSION / RELEASE_VERSION + ".0" (4th field must be 0 for the Store) -->
  <Identity Name="@IDENTITY_NAME@"
            Publisher="@PUBLISHER@"
            Version="@MAJOR@.@MINOR@.@PATCH@.0"
            ProcessorArchitecture="x64" />

  <Properties>
    <DisplayName>Phramer</DisplayName>
    <PublisherDisplayName>@PUBLISHER_DISPLAY_NAME@</PublisherDisplayName>
    <Description>Screenshot capture and annotation</Description>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>

  <Dependencies>
    <!-- 19041 = first build with uap10; Windows 10 22H2 is 19045. 26100 = Windows 11 24H2 = runner SDK. -->
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" />
    <!-- Only if you choose the framework CRT instead of app-local DLLs (see section 3):
    <PackageDependency Name="Microsoft.VCLibs.140.00.UWPDesktop" MinVersion="14.0.24217.0"
      Publisher="CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US" /> -->
  </Dependencies>

  <Resources>
    <Resource Language="en-us" />
  </Resources>

  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>

  <Applications>
    <!-- Main GUI / daemon -->
    <Application Id="Phramer" Executable="phramer.exe"
                 uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL">
      <uap:VisualElements DisplayName="Phramer"
                          Description="Screenshot capture and annotation"
                          BackgroundColor="transparent"
                          Square150x150Logo="Assets\Square150x150Logo.png"
                          Square44x44Logo="Assets\Square44x44Logo.png">
        <uap:DefaultTile Wide310x150Logo="Assets\Wide310x150Logo.png" ShortName="Phramer" />
      </uap:VisualElements>
      <Extensions>
        <!-- Launch at login (replaces HKCU\...\Run). Enabled="false": the app enables it via StartupTask API -->
        <desktop:Extension Category="windows.startupTask"
                           Executable="phramer.exe" EntryPoint="Windows.FullTrustApplication">
          <desktop:StartupTask TaskId="PhramerStartup" Enabled="false" DisplayName="Phramer" />
        </desktop:Extension>

        <!-- ms-screenclip handler (replaces HKLM ProgID + RegisteredApplications). See the risk notes in section 2. -->
        <uap3:Extension Category="windows.protocol">
          <uap3:Protocol Name="ms-screenclip" Parameters="--screenclip &quot;%1&quot;">
            <uap:DisplayName>Phramer</uap:DisplayName>
          </uap3:Protocol>
        </uap3:Extension>

        <!-- "phramer.exe" on PATH-like lookup (Win+R, terminals). One alias per Application. -->
        <uap3:Extension Category="windows.appExecutionAlias"
                        Executable="phramer.exe" EntryPoint="Windows.FullTrustApplication">
          <uap3:AppExecutionAlias>
            <desktop:ExecutionAlias Alias="phramer.exe" />
          </uap3:AppExecutionAlias>
        </uap3:Extension>
      </Extensions>
    </Application>

    <!-- Console CLI: separate Application so it can have its own alias; hidden from Start -->
    <Application Id="PhramerCli" Executable="phramer-cli.exe"
                 uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL"
                 desktop4:SupportsMultipleInstances="true">
      <uap:VisualElements DisplayName="Phramer CLI"
                          Description="Phramer command-line interface"
                          BackgroundColor="transparent"
                          Square150x150Logo="Assets\Square150x150Logo.png"
                          Square44x44Logo="Assets\Square44x44Logo.png"
                          AppListEntry="none" />
      <Extensions>
        <uap5:Extension Category="windows.appExecutionAlias" Executable="phramer-cli.exe"
                        EntryPoint="Windows.FullTrustApplication">
          <uap5:AppExecutionAlias desktop4:Subsystem="console">
            <uap5:ExecutionAlias Alias="phramer-cli.exe" />
          </uap5:AppExecutionAlias>
        </uap5:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
```

### Inferences
- `MinVersion="10.0.19041.0"` fits a Windows 10/11 x64-only app: Windows 10 22H2 (19045) is above it, and it allows the uap10 attributes. Using `EntryPoint="Windows.FullTrustApplication"` instead and `MinVersion="10.0.17763.0"` (Craft's choice) is equally valid. Pick one form, not both.
- `MaxVersionTested="10.0.26100.0"` matches the SDK on the GitHub runner. Raise it as you test on newer builds.
- The CMake guard already rejects non-`MAJOR.MINOR.PATCH` tags, so mapping `RELEASE_VERSION` → `X.Y.Z.0` is always valid (14 ≠ 0; all fields ≤ 65535). Store versions are independent of the WiX `MajorUpgrade` rule, but should keep increasing in step with the MSI.
- Keep the `PackageDependency` on VCLibs commented out if you ship app-local CRT (section 3).
- `uap:DefaultTile`'s `ShortName` and `Wide310x150Logo` are optional. Microsoft's icon guidance says Medium tile at 100% is the minimum the Store requires.

### Gaps
- I could not fetch the Application element's full attribute table to confirm that `desktop4:SupportsMultipleInstances` and `AppListEntry` are spelled and placed exactly as above for a `packagedClassicApp`. Validate with `makeappx pack` (schema validation runs by default) and `Add-AppxPackage -Register` before the first submission.
- The "Application Count" WACK test text describes bundle and package counts, not multiple `<Application>` elements. I found no source that says two Application elements fail certification, nor one that confirms they pass for a Store desktop app. Many Store apps ship several, but I could not cite one.

## 2. Manifest extensions: ms-screenclip protocol, startup task, execution aliases for phramer.exe and phramer-cli.exe

### Takeaway
Use `uap3:Protocol` with `Parameters` for ms-screenclip, `desktop:StartupTask` driven by the WinRT `StartupTask` API in place of the Run key, and one execution alias per Application. That means a second, hidden `Application` for `phramer-cli.exe`, with `desktop4:Subsystem="console"`. ms-screenclip is the riskiest item for Store certification, and the existing HKLM/elevation code must be compiled out of a Store build.

### Cited Findings
- Protocol extension: `xmlns:uap3=".../uap/windows10/3"`, `<uap3:Extension Category="windows.protocol"><uap3:Protocol Name="myapp-cmd" Parameters="/p &quot;%1&quot;" />`. "Parameters are supported only for packaged, full-trust apps." Wrap values that may contain paths in quotes. — [Desktop packaging extensions: start by protocol](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions)
- `uap:Protocol` children: `uap:Logo`, `uap:DisplayName`, `MigrationProgIds`, `ProgId`. Name length 2-2048. Minimum OS 1511. — [uap:Protocol](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap-protocol)
- "The Name must be in all lower case letters." The DisplayName and Logo identify the scheme in Default Programs. "We recommend that you only register for a URI scheme name if you expect to handle all URI launches for that type of URI scheme." — [Handle URI activation](https://learn.microsoft.com/windows/apps/develop/launch/handle-uri-activation)
- MakeAppx semantic validation rejects only these protocols: "SMB, FILE, MS-WWA-WEB, MS-WWA". — [Create an MSIX package with MakeAppx.exe](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
- Startup task: namespace `http://schemas.microsoft.com/appx/manifest/desktop/windows10`, `<desktop:Extension Category="windows.startupTask" Executable="..." EntryPoint="Windows.FullTrustApplication"><desktop:StartupTask TaskId="..." Enabled="true" DisplayName="..."/>`. "The user has to start your application at least one time to register this startup task." "If a user disables a task, you can't programmatically re-enable it." — [Desktop packaging extensions: startup task](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions)
- `desktop:StartupTask` (min OS 1607) attributes: TaskId, Enabled, DisplayName, and `rescap5:ImmediateRegistration`, which "requires the Microsoft.nonUserConfigurableStartupTasks_8wekyb3d8bbwe custom capability". — [desktop:StartupTask](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-desktop-startuptask)
- "If RequestEnableAsync is called from a packaged desktop app, no user-consent dialog is shown." Once enabled, the user controls it from Settings → Startup or Task Manager. States are `Disabled`, `DisabledByUser`, `Enabled`, `DisabledByPolicy`, and `EnabledByPolicy`. — [StartupTask class](https://learn.microsoft.com/uwp/api/windows.applicationmodel.startuptask?view=winrt-28000); [StartupTaskState](https://learn.microsoft.com/uwp/api/windows.applicationmodel.startuptaskstate?view=winrt-28000)
- Alias: `<uap3:Extension Category="windows.appExecutionAlias" Executable="..." EntryPoint="Windows.FullTrustApplication"><uap3:AppExecutionAlias><desktop:ExecutionAlias Alias="x.exe"/>`. "It must always end with the '.exe' extension. You can only specify a single app execution alias for each application in the package. If multiple apps register for the same alias, the system will invoke the last one that was registered." — [Desktop packaging extensions: alias](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions)
- `uap5:AppExecutionAlias` has `desktop4:Subsystem` / `uap10:Subsystem` = `console` | `windows` ("whether the app uses the Windows subsystem or the console subsystem"). Min OS 1709. — [uap5:AppExecutionAlias](https://learn.microsoft.com/uwp/schemas/appxpackage/uapmanifestschema/element-uap5-appexecutionalias)
- Microsoft's winapp CLI says a console app "is launched through an execution alias instead, which inherits your terminal's stdin/stdout/stderr". AUMID activation of a console app "runs without a console and prints nothing". — [Using winapp CLI with .NET](https://learn.microsoft.com/windows/apps/dev-tools/winapp-cli/guides/dotnet#4-initialize-project-with-winapp-cli)
- Users can turn aliases off in Settings → App execution aliases. — [Desktop packaging extensions: transition users](https://learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-extensions#transition-users-to-your-app)
- KDE Craft's packager emits `windows.startupTask`, `windows.appExecutionAlias` (uap3), `uap10:FileType` wildcards, and a `windows.comServer` + `windows.toastNotificationActivation` pair for SnoreToast. — [KDE Craft AppxPackager.py](https://raw.githubusercontent.com/KDE/craft/master/bin/Packager/AppxPackager.py)
- Packaged apps "may only use the AUMID generated for it" (no custom `SetCurrentProcessExplicitAppUserModelID`). Creating or opening an HKLM key for modification "will result in an access-denied failure". — [Prepare to package a desktop application](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-prepare)
- Store policy 10.2.8: "You are required to use supported methods and must obtain user consent to change any user's Windows settings, preferences, settings UI, or modify the user's Windows experience in any way. Unsupported methods include… undocumented or unsupported APIs." — [Microsoft Store Policies](https://learn.microsoft.com/windows/apps/publish/store-policies#product-policies)
- WACK UAC test: "An app cannot request admin elevation or UIAccess per Microsoft Store policy." — [Windows Desktop Bridge app tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests#current-required-tests)
- HKCU at run time in a package: "Writes go to a separate per-app, per-user private hive… Keys in the virtualized hive are only visible to the app." Opting out needs the `unvirtualizedResources` restricted capability plus `desktop6:RegistryWriteVirtualization=disabled` (whole HKCU, Win10 1903+) or `virtualization:ExcludedKey` (specific HKCU keys, Windows 11 only). — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization); contradicted in part by [MSIX troubleshooting guide](https://learn.microsoft.com/windows/msix/msix-troubleshooting-guide#runtime-and-virtualization-behavior), which says writes to `HKCU\Software\<AppName>` "are **not** virtualized and persist".

### Inferences
- **ms-screenclip**: declare it in the manifest (snippet above). Windows Snipping Tool also owns the scheme, so the user picks the default in Settings → Default apps. The current `ScreenClipProtocol` code writes HKLM through an elevated relaunch, which fails (HKLM write denied) and violates the UAC rule, so it must be compiled out of the Store build. Keep the "open Default apps settings" UI. Snipping Tool video handoff (`SnippingTool::startRecording`) launches with `TargetApplicationPackageFamilyName`, so it keeps working even when Phramer is the registered handler.
- **ms-screenclip certification risk**: it is a Microsoft `ms-` scheme handled by an inbox app, and policy 10.2.8 limits changing Windows experiences. Reviewers may object. Plan a Store build where the protocol is optional (a separate manifest variant) if certification rejects it, and explain it in "Notes for certification".
- **Autostart**: in a packaged build, replace the `HKCU\...\Run` write with `winrt::Windows::ApplicationModel::StartupTask::GetAsync(L"PhramerStartup")` → `RequestEnableAsync()` / `Disable()`, and read `.State()` for the settings checkbox (`DisabledByUser` → show "enable in Settings"). Phramer already links C++/WinRT for OCR, so this adds no new dependency. A Run-key entry would be virtualized (per the flexible-virtualization doc), and the WindowsApps path changes with every update.
- **Print Screen toggle** (`HKCU\Control Panel\Keyboard`): under the default MSIX behavior this write lands in the private hive, so Windows never sees it. Options: (a) declare `unvirtualizedResources` with `virtualization:ExcludedKey HKEY_CURRENT_USER\Control Panel\Keyboard` (Windows 11 only; restricted capability needing Store justification), or (b) in the packaged build, deep-link the user to Settings (`ms-settings:easeofaccess-keyboard`) instead of writing the key. Option (b) is safer for 10.2.8. Test the actual behavior, because the two Microsoft pages disagree about HKCU virtualization.
- **CLI**: KDSingleApplication IPC between `phramer-cli.exe` and the daemon should keep working because both run inside the same package. This is untested.
- **Restart and relaunch**: `main()` relaunches `applicationFilePath()`. Inside a package that path is under `C:\Program Files\WindowsApps\...`. Test that a child launched by path still gets package identity. If not, relaunch via the `phramer.exe` alias or the AUMID.

### Gaps
- No authoritative list of OS-reserved URI schemes that packaged apps may not declare (beyond MakeAppx's four). The former UWP page "reserved-uri-scheme-names" now redirects to the generic URI-activation article. Whether Store certification accepts `ms-screenclip` is unknown.
- I could not confirm whether `desktop4:Subsystem` must also go on the `Application` element, or only on `uap5:AppExecutionAlias`, for a full-trust console exe. The PE subsystem is already CONSOLE, and the alias attribute is the documented one.

## 3. MSVC runtime: Microsoft.VCLibs.140.00.UWPDesktop vs app-local CRT

### Takeaway
Both work. Microsoft's guidance is inconsistent: an old KB says "must" use the framework, while the newer GDK guidance says that for VS 2015-2022 the two forms are identical and either may be used. For a VS 2022 v143 or VS 2026 v145 build, app-local `vcruntime140*.dll` / `msvcp140*.dll` in the package root load first, because the app's own package comes first in the package graph. That avoids version lag in the framework. Never ship `vc_redist.x64.exe`, which is what `windeployqt --compiler-runtime --release` copies today.

### Cited Findings
- Old KB 3176696: desktop apps "must specify the corresponding version of the C++ Runtime framework package… instead of just redistributing the C++ Runtime libraries"; apps "cannot use the C++ Runtime libraries that are included with Visual Studio or VCRedist". Manifest: `<PackageDependency Name="Microsoft.VCLibs.140.00.UWPDesktop" MinVersion="14.0.24217.0" Publisher="CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US" />`. The packages ship with VS 2026's UWP workload ("C++ (v145) Universal Windows Tools") under `%ProgramFiles(x86)%\Microsoft SDKs\Windows Kits\10\ExtensionSDKs\Microsoft.VCLibs.Desktop\14.0`. — [C++ Runtime framework packages for Desktop Bridge](https://learn.microsoft.com/troubleshoot/developer/visualstudio/cpp/libraries/c-runtime-packages-desktop-bridge)
- Contradicting, for VC 2015/2017/2019/2022: "The traditional redistributable and framework package forms are identical, and you can choose either. However, we recommend using framework packages as a best practice." — [GDK: Framework package dependencies](https://learn.microsoft.com/gaming/gdk/docs/features/common/packaging/packaging-framework-packages#visual-studio-c++-runtime)
- Store installs framework dependencies automatically. Sideloads do not, so you must `Add-AppxPackage .\Microsoft.VCLibs.x64.14.00.Desktop.appx` first. The appinstaller example uses `Version="14.0.30704.0"` and `Uri="https://aka.ms/Microsoft.VCLibs.x64.14.00.Desktop.appx"`. — [MSIX troubleshooting guide: missing dependencies](https://learn.microsoft.com/windows/msix/msix-troubleshooting-guide#missing-dependencies)
- Microsoft's own Windows App currently depends on `Microsoft.VCLibs.140.00.UWPDesktop` ≥ `14.0.30035.0`, and its offline bundle carries `Microsoft.VCLibs.140.00.UWPDesktop_14.0.33728.0`. — [Windows App troubleshooting](https://learn.microsoft.com/windows-app/troubleshoot-basic)
- DLL search order for packaged apps: … 5 Known DLLs; 6 "The package dependency graph of the process. This is the application's package plus any dependencies specified as `<PackageDependency>`… searched in the order they appear in the manifest"; 7 the exe's folder; 8 system32. — [Dynamic-link library search order](https://learn.microsoft.com/windows/win32/dlls/dynamic-link-library-search-order#search-order-for-packaged-apps)
- `windeployqt --compiler-runtime` for MSVC release builds bundles the installer (`// Release: Bundle vcredist<>.exe`, finding `vc_redist.x64.exe` via `VCINSTALLDIR`). It copies DebugCRT DLLs only for debug builds. — [qtbase windeployqt main.cpp](https://raw.githubusercontent.com/qt/qtbase/dev/src/tools/windeployqt/main.cpp)
- WACK 'blocked executables' failures exist for packaged Win32 apps: "It detects when an application attempts to launch external programs (like command prompt or PowerShell)". — [Microsoft Q&A answer](https://learn.microsoft.com/answers/a/12314384) (forum, not policy)

### Inferences
- Recommended: **app-local CRT, no VCLibs dependency.** Copy `vcruntime140.dll`, `vcruntime140_1.dll`, `msvcp140.dll`, `msvcp140_1.dll`, `msvcp140_2.dll`, `msvcp140_atomic_wait.dll`, `msvcp140_codecvt_ids.dll` and `concrt140.dll` (only if imported) from `<VS>\VC\Redist\MSVC\<ver>\x64\Microsoft.VC14x.CRT\` into the package root next to `phramer.exe`. Delete `vc_redist.x64.exe` from the staging folder. The package graph puts the app's own files first, so they win over any framework, which avoids crashes when a newer toolset's STL meets an older framework. Sideloading then needs no dependency install. KDE Craft's packager has no VCLibs handling, which is consistent with app-local.
- Alternative: declare the `Microsoft.VCLibs.140.00.UWPDesktop` dependency and ship no CRT DLLs. This is simpler for Store users, but sideload testing then needs the framework .appx, and the MinVersion must cover the toolset in use.
- A cheaper repo change than editing the shared `windeployqt` call: in the MSIX staging step, delete `vc_redist*.exe` and copy the CRT folder. This leaves the MSI path untouched.

### Gaps
- No current Microsoft statement says Store certification rejects app-local CRT for desktop packages. The KB's "must" language dates to 2016, while the GDK page (2022+) says either form works.
- I could not confirm which `Microsoft.VCLibs.140.00.UWPDesktop` version corresponds to the VS 2022 17.14 (14.44) or VS 2026 (14.5x) STL. I found no published mapping, which is one more reason for app-local.

## 4. Qt deployment inside MSIX

### Takeaway
The MSIX payload is the `cmake --install` / `windeployqt --release` folder flattened to the package root: `phramer.exe`, `phramer-cli.exe`, `Qt6*.dll`, `platforms\qwindows.dll`, `styles\`, `imageformats\`, `iconengines\`, `tls\`, `translations\`, plus the CRT DLLs. Main pitfalls: debug binaries (`--debug` when `CMAKE_BUILD_TYPE` is not Release), `vc_redist.x64.exe` in the payload, a missing `platforms` folder, and writes beside the exe.

### Cited Findings
- `windeployqt` options (help text): `--release` "Assume release binaries."; `--compiler-runtime` "Deploy compiler runtime (Desktop only)."; `--no-compiler-runtime`; `--no-opengl-sw` "Do not deploy the software rasterizer library."; `--no-system-d3d-compiler`; `--no-system-dxc-compiler` "Skip deployment of the system DXC (dxcompiler.dll, dxil.dll)."; `--skip-plugin-types` "(qmltooling,generic)"; `--exclude-plugins` "(qsvg,qpdf)"; `--no-translations`; `--no-ffmpeg`. — [qtbase windeployqt main.cpp](https://raw.githubusercontent.com/qt/qtbase/dev/src/tools/windeployqt/main.cpp)
- "The Windows platform plugin qwindows.dll must be copied to a subdirectory named platforms." Store deployment needs "Local deployment" of C++ runtime files, while windeployqt's default does "Central deployment" (vcredist installer). — search-result snippets of [Qt for Windows - Deployment](https://doc.qt.io/qt-6/windows-deployment.html) (page itself blocked; snippet only)
- Forum reports of MSIX packages failing because `qwindows.dll` was missing from the packaging project ("can't find entry point" thread title). — search snippets of [Qt forum: MSIX can't find entry point](https://forum.qt.io/topic/163528/msix-for-windos-app-store-can-t-find-entry-point) and [Qt forum: Preparing app for Microsoft Store (Qt 6 + CMake)](https://forum.qt.io/topic/147272/preparing-app-for-microsoft-store-qt-6-cmake) (blocked; snippets only)
- Packaged apps: writes to the install directory are not supported ("the folder is protected"). The working directory differs from the .lnk's. `C:\Program Files\WindowsApps\<pkg>` is read-only. — [Know your installer](https://learn.microsoft.com/windows/msix/packaging-tool/know-your-installer); [Behind the scenes](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-behind-the-scenes#file-system)
- AppData at run time (Windows 10 1903+): newly created files under `AppData\Local` / `Roaming` go to a per-user, per-package private location. Modifications to *existing* AppData files happen on the real files. Reads try the private location first. — [Flexible virtualization](https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization); [Behind the scenes](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-behind-the-scenes#file-system)
- WACK Debug configuration test: "apps must not be compiled for debug and they must not reference debug versions of an executable file." — [Windows Desktop Bridge app tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests#current-optional-tests)
- KDE ships Kate, Okular and KDE Connect in the Store via Craft-built APPX/MSIX (Kate since 2019; roughly half a million Okular installs). Craft "is capable of automatically signing packages for publication to official stores like Microsoft Store." — search snippets of [Kate & Co. in the Microsoft Store](https://kate-editor.org/post/2025/2025-06-03-kate-and-co-in-the-microsoft-store/) and [Building KDE software on Windows with Craft](https://develop.kde.org/docs/getting-started/building/craft/) (blocked; snippets only)

### Inferences
- Stage with `cmake --install build --config Release --prefix stage`, then use `stage\bin\*` as the package root. Releases already configure with `-DCMAKE_BUILD_TYPE=Release`, so windeployqt runs with `--release`. CLAUDE.md warns that a multi-config build without `CMAKE_BUILD_TYPE` silently deploys debug Qt and fails WACK.
- `USE_PORTABLE_CONFIG` must be OFF for MSIX, as for the MSI. Portable mode writes `phramer.ini` beside the exe, which is read-only in WindowsApps. With OFF, QSettings writes `%APPDATA%\...\phramer.ini`. An MSI user switching to the Store build keeps their settings, because existing AppData files are modified in place. A fresh Store-only install gets a private copy that is removed on uninstall.
- Translations: the build copies `.qm` files into `translations\`, and Qt finds them relative to `applicationDirPath()`, which still works read-only. No `qt.conf` is needed, because windeployqt's default layout (plugins beside the exe) is what Qt searches.
- `--no-opengl-sw` saves about 20 MB if Phramer does not need software OpenGL (it uses widgets, not Quick).
- OpenSSL: only bundle it if `ENABLE_OPENSSL` is on. Qt 6's `tls\qschannelbackend.dll` covers HTTPS on Windows without OpenSSL.
- The in-app updater (`msiexec` via `cmd` batch) must be disabled in the packaged build. Updates come through the Store, a package cannot `msiexec` itself, and WACK may flag `cmd.exe` launches. A compile-time option is safer than a runtime check, because WACK scans binaries. At runtime, `GetCurrentPackageFullName()` returning `APPMODEL_ERROR_NO_PACKAGE` is the standard way to detect "unpackaged". (This API detail is from general knowledge, not fetched this session.)
- Policy note: current Store policy 10.2.5 says "installed and updated only through the Store" for games and Xbox products. Older versions (7.11-7.13) applied it to all products. Either way, a self-updating Store package is inadvisable. — [Microsoft Store Policies](https://learn.microsoft.com/windows/apps/publish/store-policies#product-policies); [Store Policies 7.12](https://learn.microsoft.com/windows/apps/publish/store-policy-archive/store-policy-7-12#product-policies)

### Gaps
- doc.qt.io, forum.qt.io, the Qt bug tracker and KDE's own pages were blocked by the network proxy, so I could not read Qt's official MSIX notes or KDE's Store packaging guide in full. Only search snippets are cited above.
- Not verified: whether Qt 6.9's `QStandardPaths::AppDataLocation` resolves differently under package identity. It should not, since it uses the Known Folder API, which returns the real path while redirection happens underneath.

## 5. Tooling: makeappx, makepri, signtool, .msixupload, Windows SDK on GitHub runners, CMake/CI options

### Takeaway
CPack has no MSIX generator, so the practical path is a hand-written manifest plus `makepri` plus `makeappx pack /h SHA256`, then zip with an optional `.appxsym` into `.msixupload`. No signing is needed for Store upload (the Store re-signs). Sign only the test/sideload package. Microsoft's new **winapp CLI** (public preview, `microsoft/setup-WinAppCli` action) wraps the same steps: manifest, assets, PRI, pack, cert and sign.

### Cited Findings
- MakeAppx location: `C:\Program Files (x86)\Windows Kits\10\bin\<build>\<arch>\makeappx.exe` (also under `App Certification Kit\`). Usage: `MakeAppx pack /v /h SHA256 /d "C:\My Files" /p MyPackage.msix`. `/o` overwrites; `/nv` skips semantic validation; `/f` takes a mapping file (`[Files]` then `"src" "dest"` lines). Validation checks that "All files referenced in the package manifest are included". "Packages built by MakeAppx.exe are not guaranteed to be installable." — [Create an MSIX package with MakeAppx.exe](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
- "MakeAppx.exe does not create an app package upload file (.appxupload or .msixupload), which is the recommended type." — [MakeAppx.exe](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
- Manual .msixupload: put the .msix (or bundle) and an optional `.appxsym` in a folder, zip them, and rename `.zip` → `.msixupload`. `.appxsym` "is a compressed .pdb file containing public symbols… You can omit this file, but if you do, no crash analytic or debugging information will be available." — [Package a desktop or UWP app in Visual Studio](https://learn.microsoft.com/windows/msix/package/packaging-uwp-apps#generate-an-app-package-upload-file-for-store-submission); [App package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- Accepted Store formats: .msix, .msixbundle, .msixupload, .appx, .appxbundle, .appxupload. Partner Center "recommend[s] uploading the .msixupload… rather than .msix", but plain .msix is accepted. — [App package requirements FAQ](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements); [Upload MSIX app packages](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/upload-app-packages#upload-your-app)
- Signing for the Store: packages "don't have to be signed with a certificate rooted in a trusted certificate authority… The Microsoft Store will automatically re-sign your MSIX/AppX packages with a Microsoft certificate." Sideload or enterprise distribution needs your own signature. — [App package requirements: Code signing](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- SignTool: `SignTool sign /a /v /fd SHA256 /f certFileName file.msix`. "The hashAlgorithm must match the hash algorithm used to create the blockmap"; MakeAppx's default is SHA256. The certificate subject must equal the manifest Publisher. — [MakeAppx.exe: sign with SignTool](https://learn.microsoft.com/windows/win32/appxpkg/make-appx-package--makeappx-exe-#using-app-packager)
- Self-signed test cert: `New-SelfSignedCertificate -Type Custom -KeyUsage DigitalSignature -Subject "CN=MyPublisher" -CertStoreLocation "Cert:\CurrentUser\My" -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")`. — [Sign your MSIX package: end-to-end guide](https://learn.microsoft.com/windows/msix/package/sign-msix-package-guide#development-sign-for-local-testing)
- MakePri: `makepri.exe createconfig /cf priconfig.xml /dq en-US`, then `makepri.exe new /pr <root> /cf <root>\priconfig.xml`. Needed "if you create target-based assets… or you modify any of the visual assets". — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- WACK resource checks include "There is no default resource specified in the 'resources.pri' file", image files "must be smaller than 204800 bytes", and the PRI "must not contain a reverse map section" (run makepri without `/m`). — [Windows Desktop Bridge app tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests#current-required-tests)
- Before packaging you can test the loose layout with `Add-AppxPackage -Register AppxManifest.xml`. Bump Version to re-register changed binaries. — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- winapp CLI ("public preview… Features and commands may change"): `winget install Microsoft.winappcli`; CI via the `setup-WinAppCli` action. Commands include `winapp manifest generate`, `manifest add-alias`, "Update Manifest Assets: Generate all required app icon assets from a single source image", `cert generate --manifest … --install`, `pack <folder> --manifest … --cert … [--executable] [--skip-pri]`, and `sign`. pack "Validates and processes Package.appxmanifest", resolves `$targetnametoken$`, and generates the PRI unless `--skip-pri`. — [winapp CLI overview](https://learn.microsoft.com/windows/apps/dev-tools/winapp-cli/); [winapp CLI usage](https://learn.microsoft.com/windows/apps/dev-tools/winapp-cli/usage); [VS Code tools for Windows development](https://learn.microsoft.com/windows/apps/develop/ai-assisted/vs-code-tools)
- Microsoft's CI troubleshooting suggests `winget install --id Microsoft.WindowsSDK.10.0.22621` if SignTool is missing, or the winapp CLI, or Azure Artifact Signing (`azure/trusted-signing-action`) for production non-Store signing. — [MSIX troubleshooting guide: SignTool not found in CI/CD](https://learn.microsoft.com/windows/msix/msix-troubleshooting-guide#signtool-not-found-in-ci-cd)
- GitHub runners (Sept 2026): `windows-latest` = `windows-2025` = `windows-2025-vs2026` (Windows Server 2025); `windows-2022` still offered; `windows-11-vs2026-arm` for Arm64. "we only support the latest 2 versions of an OS". — [actions/runner-images README](https://github.com/actions/runner-images). The windows-2025 image (version 20260922.270.2) lists "Windows Software Development Kit 10.1.26100.7705" and "Visual Studio Enterprise 2022" 17.14.37710.0. — [Windows2025-Readme](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)

### Concrete GitHub Actions job (fits after the existing Build step in `Windows-release.yml`)

```yaml
      - name: Stage MSIX payload
        shell: pwsh
        run: |
          $stage = "${{ github.workspace }}\msix"
          cmake --install build --config Release --prefix "${{ github.workspace }}\stage"
          New-Item -ItemType Directory -Force $stage | Out-Null
          Copy-Item "${{ github.workspace }}\stage\bin\*" $stage -Recurse -Force
          Remove-Item "$stage\vc_redist*.exe" -ErrorAction SilentlyContinue   # windeployqt --compiler-runtime puts the installer here
          # App-local CRT (see section 3)
          $vs  = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath
          $crt = Get-ChildItem "$vs\VC\Redist\MSVC\*\x64\Microsoft.VC14*.CRT" -Directory | Sort-Object FullName | Select-Object -Last 1
          Copy-Item "$($crt.FullName)\*.dll" $stage
          # Manifest + assets
          $version = ("${{ github.ref_name }}" -replace '^v','') + '.0'
          (Get-Content packaging\msix\AppxManifest.xml.in -Raw) `
            -replace '@VERSION@', $version `
            -replace '@IDENTITY_NAME@', '${{ vars.MSIX_IDENTITY_NAME }}' `
            -replace '@PUBLISHER@', '${{ vars.MSIX_PUBLISHER }}' `
            -replace '@PUBLISHER_DISPLAY_NAME@', '${{ vars.MSIX_PUBLISHER_DISPLAY_NAME }}' |
            Set-Content "$stage\AppxManifest.xml" -Encoding utf8
          Copy-Item packaging\msix\Assets "$stage\Assets" -Recurse

      - name: Pack MSIX
        shell: pwsh
        run: |
          $sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64" -Directory | Sort-Object Name | Select-Object -Last 1
          $stage = "${{ github.workspace }}\msix"
          Push-Location $stage
          & "$sdk\makepri.exe" createconfig /cf priconfig.xml /dq en-US /o
          & "$sdk\makepri.exe" new /pr $stage /cf "$stage\priconfig.xml" /of "$stage\resources.pri" /o
          Remove-Item priconfig.xml
          Pop-Location
          New-Item -ItemType Directory -Force dist | Out-Null
          & "$sdk\makeappx.exe" pack /v /o /h SHA256 /d $stage /p "dist\Phramer_x64.msix"
          # .msixupload = zip(.msix [+ .appxsym]) renamed
          Compress-Archive -Path dist\Phramer_x64.msix -DestinationPath dist\Phramer_x64.zip -Force
          Rename-Item dist\Phramer_x64.zip Phramer_x64.msixupload
```

Note: the `-replace` chain above uses backtick line continuations, which `Windows-release.yml` deliberately avoids because of CRLF issues. Collapse it to one line, or use `configure_file` in CMake. The makepri `/of` and `/o` flags come from general makepri usage and were not on the fetched page. Check `makepri new /?` on the runner.

### Inferences
- Use `windows-2022` (current) or `windows-2025`. Both have a 10.0.26100 SDK with makeappx, makepri and signtool under `Windows Kits\10\bin\10.0.26100.0\x64`. Resolve the path with a glob, as above, rather than hardcoding the build number. `windows-2025-vs2026` matters only if you move the build to the "Visual Studio 18 2026" generator.
- **CMake-native**: no CPack MSIX generator exists (none found; KDE uses its own Craft packager). Keep the manifest as `packaging/msix/AppxManifest.xml.in` and either `configure_file` it with `PROJECT_VERSION`, or template it in the workflow. A small `add_custom_target(msix ...)` that calls makepri/makeappx is optional.
- `.appxsym`: the Release config emits no PDBs by default. To get Partner Center crash stacks, add `/Zi` + `/DEBUG` (or build RelWithDebInfo) and zip `phramer.pdb`/`phramer-cli.pdb` into `Phramer_x64.appxsym`. That file is "a compressed .pdb", so a renamed zip of the PDBs follows the documented description, but I found no spec for its internal layout.
- Alternatives: the winapp CLI (`winapp pack stage --manifest AppxManifest.xml`) handles PRI and assets automatically but is preview. Advanced Installer and the MSIX Packaging Tool are GUI or repackaging-oriented; I did not research them in depth.
- The Store submission itself can be automated later (Microsoft Store Developer CLI / submission API); out of scope here.

### Gaps
- No official confirmation on whether WACK (`appcert.exe`) is installed on GitHub runner images. The runner README does not mention it.
- No primary source on the exact `.appxsym` internal structure.

## 6. Visual assets: required logos, scales, generation

### Takeaway
The manifest strictly requires `Properties/Logo` (StoreLogo, 50 px base), `Square150x150Logo` and `Square44x44Logo`. Microsoft recommends at least scale-100/200/400 variants plus `Square44x44Logo.targetsize-{16,24,32,48,256}[_altform-unplated]`, otherwise the taskbar icon gets a backplate. Every image must be under 200 KB, and a PRI must be generated so qualifiers resolve.

### Cited Findings
- Manifest base sizes per scale (100/125/150/200/250/300/400 %): Square44x44Logo 44/55/66/88/110/132/176; Square150x150Logo 150/188/225/300/375/450/600; Wide310x150Logo 310×150 … 1240×600; Square310x310Logo 310 … 1240; Square71x71Logo 71 … 284; StoreLogo 50/63/75/100/125/150/200. "At minimum, provide assets at 100%, 200%, and 400% scale for the Square44x44Logo and Square150x150Logo entries, plus the target-size variants." — [Construct your Windows app's icon](https://learn.microsoft.com/en-us/windows/apps/design/style/iconography/app-icon-construction)
- Target-size app-list icons: 16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256. Each also as `_altform-unplated` (dark) and `_altform-lightunplated` (light). "If you do not include the targetsize-*-altform-unplated assets above your icon will scale to a smaller size and will get an undesirable backplate behind the icon on Taskbar and Start." — [Construct your Windows app's icon](https://learn.microsoft.com/en-us/windows/apps/design/style/iconography/app-icon-construction)
- "Windows 11 does not use the tile assets, but currently at minimum the Medium tile assets at 100% are required to publish to the Microsoft Store." StoreLogo scale-100…400 is "required to publish". SplashScreen and BadgeLogo are optional for desktop apps. — [Construct your Windows app's icon](https://learn.microsoft.com/en-us/windows/apps/design/style/iconography/app-icon-construction)
- Manual-packaging recipe: copy the 44x44 to `<name>.targetsize-44_altform-unplated.png`, set `BackgroundColor` for transparent icons, then regenerate the PRI. — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- WACK: images must be < 204800 bytes. WACK "Branding validation" fails default template or SDK images. — [Windows Desktop Bridge app tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests#current-required-tests)
- Protocol handlers: "Include a 44x44 icon… Match the look of the app tile logo… Test your icons on white backgrounds." — [Handle URI activation](https://learn.microsoft.com/windows/apps/develop/launch/handle-uri-activation)

### Concrete asset set for Phramer (from `data/img/app/Phramer.svg`)

```
Assets/StoreLogo.scale-{100,125,150,200,400}.png                    50,63,75,100,200
Assets/Square44x44Logo.scale-{100,125,150,200,400}.png              44,55,66,88,176
Assets/Square44x44Logo.targetsize-{16,20,24,30,32,36,40,48,60,64,72,80,96,256}.png
Assets/Square44x44Logo.targetsize-{same}_altform-unplated.png
Assets/Square44x44Logo.targetsize-{same}_altform-lightunplated.png
Assets/Square150x150Logo.scale-{100,125,150,200,400}.png            150,188,225,300,600
Assets/Wide310x150Logo.scale-{100,200,400}.png                      310x150,620x300,1240x600 (icon centred)
```

The manifest references the unqualified names (`Assets\Square44x44Logo.png`). makepri indexes the qualified files.

```pwsh
# ImageMagick 7 (choco install imagemagick, or use Inkscape/rsvg-convert for SVG rasterization)
$svg = "data\img\app\Phramer.svg"
foreach ($s in 16,20,24,30,32,36,40,48,60,64,72,80,96,256) {
  magick -background none -density 1024 $svg -resize "${s}x${s}" "Assets\Square44x44Logo.targetsize-$s.png"
  Copy-Item "Assets\Square44x44Logo.targetsize-$s.png" "Assets\Square44x44Logo.targetsize-${s}_altform-unplated.png"
  Copy-Item "Assets\Square44x44Logo.targetsize-$s.png" "Assets\Square44x44Logo.targetsize-${s}_altform-lightunplated.png"
}
@{100=44;125=55;150=66;200=88;400=176}.GetEnumerator() | % { magick -background none -density 1024 $svg -resize "$($_.Value)x$($_.Value)" "Assets\Square44x44Logo.scale-$($_.Key).png" }
@{100=150;125=188;150=225;200=300;400=600}.GetEnumerator() | % { magick -background none -density 1024 $svg -resize "$([int]($_.Value*0.66))x" -gravity center -extent "$($_.Value)x$($_.Value)" "Assets\Square150x150Logo.scale-$($_.Key).png" }
@{100=50;125=63;150=75;200=100;400=200}.GetEnumerator() | % { magick -background none -density 1024 $svg -resize "$($_.Value)x$($_.Value)" "Assets\StoreLogo.scale-$($_.Key).png" }
@{100=@(310,150);200=@(620,300);400=@(1240,600)}.GetEnumerator() | % { $w,$h=$_.Value; magick -background none -density 1024 $svg -resize "x$([int]($h*0.66))" -gravity center -extent "${w}x${h}" "Assets\Wide310x150Logo.scale-$($_.Key).png" }
```

### Inferences
- Generate the assets once and commit them under `packaging/msix/Assets` rather than in CI, so ImageMagick is not a CI dependency. Alternatively, `winapp manifest update-assets` generates them from one image.
- If the icon is light-on-transparent or dark-on-transparent, give the `_altform-lightunplated` set a variant with contrast on light taskbars.
- Padding (about 66% fill) on Square150/Wide follows common tile practice. It is a judgment call, not from the sources.

### Gaps
- Microsoft's icon page labels the AppList targetsize sets "Required" but names them `AppList.*`. The manifest-driven name is whatever `Square44x44Logo` points at. I found no explicit statement that the Store rejects packages missing the light/dark unplated sets, only that the icon gets a backplate.

## 7. WACK for full-trust desktop packages: tests, common Qt failures, running it

### Takeaway
WACK's desktop-bridge workflow checks manifest and resources, platform-appropriate binaries (all x64), the absence of services and drivers, no UAC elevation, no debug builds, and banned or private-key files (.pfx/.snk). Optional tests warn about unsigned PE files. Run it with `appcert.exe test -appxpackagepath <msix> -reportoutputpath <xml>` in an elevated, interactive session.

### Cited Findings
- Required tests include: file extensions and protocols (limits "the number of file extensions"); framework dependency rule ("fails if the app refers to any 'preview' versions of the framework dlls"); IPC verification (fails with `ActivatableClassAttribute` `DesktopApplicationPath`); app manifest checks; registry checks (fail if the package "installs or updates any new services or drivers"); platform-appropriate files ("If the Target Processor Architecture for your app is x64… If the package contains Arm binary… or *only* contains x86 binaries… it will fail"); supported API test ("C++ apps that are built in a debug configuration will fail this test"); UAC ("cannot request admin elevation or UIAccess"); Windows Runtime metadata; banned file analyzer; Private Code Signing (fails on `.pfx` / `.snk` in the package). — [Windows Desktop Bridge app tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests#current-required-tests)
- Optional (informational, "will not be used to evaluate your app during Microsoft Store onboarding"): digitally signed file test ("A warning will be generated if any of the PE files is not signed"), file association verbs, debug configuration, package sanity (archive files). — [Windows Desktop Bridge app tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests#current-optional-tests)
- Command line (admin prompt), default path `C:\Program Files (x86)\Windows Kits\10\App Certification Kit\`: `appcert.exe reset`, then `appcert.exe test -appxpackagepath [package path] -reportoutputpath [report file name]` (or `-packagefullname` for an installed package). "The Windows App Certification Kit must be run within the context of an active user session." It "can be run from a service, but the service must initiate the kit process within an active user session and cannot be run in Session0." The kit ships in the Windows SDK, and the device must be in developer mode. — [Windows App Certification Kit](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-app-certification-kit#validate-your-windows-app-using-the-windows-app-certification-kit-from-a-command-line); the same page also says "The kit can now be integrated into an automated testing where no interactive user session is available" ([WACK overview](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-app-certification-kit)), which contradicts the active-session requirement.
- A reported Store failure for a packaged Win32 app: WACK "Detects if there are references to process launch APIs and blocked executables" (launching cmd or PowerShell). The answer advises testing on a clean VM to reproduce "cannot find path" certification errors. — [Microsoft Q&A](https://learn.microsoft.com/answers/a/12314384) (community answer)

### Inferences
- Likely Phramer-specific failures and their fixes:
  1. Debug Qt DLLs from a mis-configured `windeployqt` → always build with `-DCMAKE_BUILD_TYPE=Release`.
  2. `vc_redist.x64.exe` in the payload → delete it in staging.
  3. The updater's `cmd`/`msiexec` launch and the elevated ms-screenclip helper → compile out in the Store build.
  4. Any 32-bit helper DLLs in OpenSSL or plugins → ensure all files are x64.
  5. Images over 200 KB (for example a 1240×600 Wide tile) → compress them with `oxipng` or `pngquant`.
  6. A stray `.pfx` test certificate copied into the staging folder → keep certificates outside it.
- Unsigned PE files only produce an optional warning, and the Store re-signs the package, so Authenticode-signing `phramer.exe` is not needed for Store submission.
- CI: running WACK on GitHub-hosted runners is uncertain, because the docs disagree on whether an interactive session is required. Plan to run it locally (or on a self-hosted runner with auto-logon) before each Store submission. In CI, run `makeappx` validation plus an `Add-AppxPackage` smoke install.

### Gaps
- No primary source lists WACK failures specific to Qt apps. The forum threads were blocked.
- The "process launch APIs and blocked executables" test is not described on the current Desktop Bridge tests page, so its exact detection method (static import or string scan vs. runtime) is unknown.

## 8. Local testing: install with a test certificate, inspect virtualized registry and AppData

### Takeaway
Register the loose folder with `Add-AppxPackage -Register AppxManifest.xml` (developer mode) for quick iteration. Otherwise sign the .msix with a self-signed certificate whose subject equals the manifest Publisher, trust it in `LocalMachine\TrustedPeople`, and `Add-AppxPackage`. Redirected files live under `%LocalAppData%\Packages\<PackageFamilyName>\LocalCache\`, and HKCU writes live in a per-package private hive.

### Cited Findings
- `Add-AppxPackage –Register AppxManifest.xml` deploys the unpackaged layout for testing. To update, replace files, bump Version and re-run. — [Generating MSIX package components](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- Self-signed flow: `New-SelfSignedCertificate -Type Custom -KeyUsage DigitalSignature -Subject "CN=…" -CertStoreLocation "Cert:\CurrentUser\My" -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3","2.5.29.19={text}")`, export to PFX, sign with SignTool. Or use `winapp cert generate --manifest .\appxmanifest.xml --output .\devcert.pfx --install` and `winapp sign MyApp.msix --cert .\devcert.pfx`. "Self-signed packages can only be installed on machines where the certificate is explicitly trusted." — [Sign your MSIX package: end-to-end guide](https://learn.microsoft.com/windows/msix/package/sign-msix-package-guide#development-sign-for-local-testing)
- Sideloads do not pull framework dependencies. Install VCLibs first (`Add-AppxPackage -Path .\Microsoft.VCLibs.x64.14.00.Desktop.appx`) if you declare it. — [MSIX troubleshooting guide](https://learn.microsoft.com/windows/msix/msix-troubleshooting-guide#missing-dependencies)
- Redirected file writes: `%LocalAppData%\Packages\<PackageFamilyName>\LocalCache\Local\VFS\…`. Inspect `%LocalAppData%\Packages\<PackageFamilyName>\LocalCache\`. Persistent per-user data belongs in `…\LocalState\`. HKLM\Software writes go "to a per-package registry hive". — [MSIX troubleshooting guide: runtime and virtualization](https://learn.microsoft.com/windows/msix/msix-troubleshooting-guide#runtime-and-virtualization-behavior)
- On uninstall, everything under `C:\Program Files\WindowsApps\<package_full_name>` and the captured AppData/registry writes are removed. — [Behind the scenes: uninstallation](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-behind-the-scenes#registry)

### Inferences
- Test recipe for Phramer (admin PowerShell):
  ```pwsh
  $pub = "CN=Phramer Test"   # must equal Identity/@Publisher in the TEST manifest
  $c = New-SelfSignedCertificate -Type Custom -KeyUsage DigitalSignature -Subject $pub -CertStoreLocation Cert:\CurrentUser\My -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3","2.5.29.19={text}")
  Export-PfxCertificate -Cert $c -FilePath test.pfx -Password (ConvertTo-SecureString pass -AsPlainText -Force)
  Import-PfxCertificate -FilePath test.pfx -CertStoreLocation Cert:\LocalMachine\TrustedPeople -Password (ConvertTo-SecureString pass -AsPlainText -Force)
  & "$sdk\signtool.exe" sign /fd SHA256 /a /f test.pfx /p pass dist\Phramer_x64.msix
  Add-AppxPackage dist\Phramer_x64.msix
  Get-AppxPackage *Phramer* | Select Name, PackageFamilyName, InstallLocation
  Invoke-CommandInDesktopPackage -PackageFamilyName <PFN> -AppId Phramer -Command regedit.exe   # view the app's merged registry
  ```
  The Store manifest's Publisher (`CN=<GUID>` from Partner Center) will not match a test cert, so either keep a separate test Publisher string in CI or sign the Store-identity package with a cert whose subject is that `CN=<GUID>`.
- `Invoke-CommandInDesktopPackage` (runs a process inside the package's container so you see the virtualized registry and files) is a standard Appx PowerShell cmdlet, but it was not on a page fetched this session. Verify its parameters with `Get-Help`.
- Check after install: tray icon, global hotkeys (`RegisterHotKey` works for full-trust), OCR (Windows.Media.Ocr gains package identity), the startup task appearing in Settings → Startup, `phramer-cli.exe gui` from a fresh terminal (alias), and `ms-screenclip:` once Phramer is chosen in Default apps. Also confirm that `%APPDATA%\phramer.ini` changes are visible outside the package when the file pre-existed, and stay private when it did not.

### Gaps
- No primary source fetched on `Invoke-CommandInDesktopPackage` or `Get-AppxPackage` output fields. They are standard but not cited.
- Behavior of `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` and `HKCU\Control Panel\Keyboard` writes inside a package must be verified empirically, because the Microsoft pages conflict (see section 2).
