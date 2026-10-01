# Microsoft Store policy, certification and Partner Center submission for Phramer (free GPL-3.0 full-trust Win32 MSIX, individual developer, October 2026)

Scope note: research done 1 October 2026. Microsoft Store Policies **7.20** was published 15 Sep 2026 and takes effect **22 Oct 2026**; until then 7.19 (Sep 2025) applies. The 7.20 changes are almost all XBOX/game-related; the two that touch Phramer are 11.11 (age rating must stay accurate for the product's lifetime) and 11.12 (user-generated content). Several sites could not be fetched from this environment (apps.microsoft.com, flameshot.org, gimp.org, hexastrike.com, the App Developer Agreement PDF on go.microsoft.com/cdn-dynmedia). For those, the notes rely on search-result snippets and label them that way.

Phramer facts used below come from the repo: CLAUDE.md, README.md and `src/`. It is a tray app with global hotkeys. It has an Imgur uploader (`src/tools/imgupload/storages/imgur/imguruploader.cpp`, which posts to `https://api.imgur.com/3/image` with a default Imgur client ID `313baf0c7b4d3ff` in `src/utils/confighandler.cpp:155`, and `uploadWithoutConfirmation` defaults to false). It has an in-app updater that polls GitHub releases (`FlameshotDaemon::checkForUpdates`), a Print Screen toggle that writes `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` (`src/utils/printscreenkey.cpp`), an ms-screenclip handler that writes HKLM/HKCU Classes and `RegisteredApplications` (`src/utils/screenclipprotocol.cpp`), a Snipping Tool video handoff (`src/utils/snippingtool.cpp`), a first-run welcome tour (`src/widgets/welcometour.*`), and 48 translation files in `data/translations`.

---

## 1. Account: free individual registration, Partner Center basics, reserving "Phramer", publisher display name, package identity values

### Takeaway
An individual account is now free. You sign up only through storedeveloper.microsoft.com with a personal Microsoft account (MSA) and verify with a government ID plus a selfie, and once verified you go straight to Partner Center. After you reserve "Phramer" under Apps & games → New product → MSIX or PWA app, the **Product identity** page gives you the three exact strings the manifest must contain: Package/Identity/Name, Package/Identity/Publisher (a `CN=<GUID>` value) and Package/Properties/PublisherDisplayName.

### Cited Findings
- New individual onboarding has **no registration fee** ("The $19 registration fee is waived in the new flow"). It uses **government-issued ID + selfie** verification, auto-fills the profile from the ID, and gives "Instant access to Partner Center" once verified. It is "available in nearly 200 markets worldwide". — [Free developer registration for individual developers](https://learn.microsoft.com/windows/apps/publish/whats-new-individual-developer)
- Flow: go to https://storedeveloper.microsoft.com → "Get started for free" → choose **Individual developer** → sign in with an MSA (or create one) → ID + selfie captured on mobile "in good lighting with original documents" → review the auto-filled profile → "Go to Partner Center dashboard". If the Apps & Games tile doesn't appear, "Wait ~5 minutes and refresh". — [Steps to open a developer account](https://learn.microsoft.com/windows/apps/publish/partner-center/open-a-developer-account#step-by-step-flow)
- storedeveloper.microsoft.com is "the only supported entry point for the new flow. Other paths (e.g. direct via Partner Center, Xbox, or Visual Studio) will show the legacy flow." The ID is used only for verification. Microsoft "may retain non-PII data like Publisher name and country". Onboarding-only help: storesupport@service.microsoft.com. Everything else goes through a support ticket at aka.ms/windowsdevelopersupport. — [Free developer registration FAQ](https://learn.microsoft.com/windows/apps/publish/whats-new-individual-developer#need-help--contact-us)
- "Individual developer accounts must use a personal Microsoft account". Entra ID work accounts are allowed only for company accounts. — [Steps to open a developer account (company tab)](https://learn.microsoft.com/windows/apps/publish/partner-center/open-a-developer-account#step-by-step-flow)
- Individual accounts are for "Independent developers whose distribution of apps through the Store is **not in relation to their business, trade, or profession**", "Small scale creators producing content for non-commercial purposes" and "hobbyist, amateur, school, or personal project[s]". — [Get started with Microsoft Store](https://learn.microsoft.com/windows/apps/publish/get-started)
- During setup you "Enter a Publisher Display Name – this is the name shown to customers in the Store" and accept the App Developer Agreement. — [Get started FAQ](https://learn.microsoft.com/windows/apps/publish/get-started#frequently-asked-questions)
- Policy 10.14: "A company account is required if your product requires financial information for primary functionality ... or **if a reasonable consumer would interpret your application or publisher name to be that of a business entity**." — [Microsoft Store Policies 7.20, 10.14](https://learn.microsoft.com/windows/apps/publish/store-policies)
- Name reservation: Partner Center → New product → **MSIX or PWA app** → enter name → Check availability → Reserve product name. Reservations last **three months** and are removed if unused. A name can be unavailable even with no visible listing because "another developer has reserved the name". If you hold rights to a name someone else has, contact Microsoft. — [Reserve your app name for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/reserve-your-apps-name)
- Naming guidance: up to 256 characters but keep it short; "Do not use trademarked names ... could lead to your app being removed"; no emoji or special characters; no trailing version numbers. — [Reserve your app name FAQ](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/reserve-your-apps-name#frequently-asked-questions)
- The Product identity page (Product management → Product identity) lists the values the manifest must contain: **Package/Identity/Name**, **Package/Identity/Publisher**, **Package/Properties/PublisherDisplayName**. It also shows the Package Family Name, Package SID, Store ID and the Store links. — [View product identity details](https://learn.microsoft.com/windows/apps/publish/view-app-identity-details)
- Microsoft's step-by-step for a non-Visual-Studio package: reserve the name, start the submission until you reach Packages, then copy those three values from Product identity into the manifest. — [Publish a Command Palette extension to the Store (PowerToys docs)](https://learn.microsoft.com/windows/powertoys/command-palette/publish-extension-store#set-up-microsoft-store)
- Error if the package DisplayName isn't reserved: "The name found in the package is not one of your reserved app names". Fix it on Product identity or under Manage app name reservations. — [Resolve submission errors for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/resolve-submission-errors#name-identity-errors)
- A Store-associated package shows a `CN=<GUID>`-style Publisher when sideloaded. In the Store the friendly publisher display name is shown instead (Microsoft Q&A, community answer). — [Why does my Microsoft Store MSIX show CN=GUID as the Publisher?](https://learn.microsoft.com/answers/a/12838708)
- In the manifest, `Properties/DisplayName` "is the name of your application that you reserve in the Store". — [Generating MSIX package components](https://learn.microsoft.com/windows/msix/desktop/desktop-to-uwp-manual-conversion#fill-in-the-package-level-elements-of-your-file)

### Inferences
- An individual account fits Phramer: one hobbyist developer, free app, no financial data. The risk is 10.14's "reasonable consumer" test. A publisher display name like "Phramer Software" or "Phramer Labs" could read as a business. Use the developer's personal name or a plain personal handle, and keep "Phramer" as the product name only.
- The publisher display name entered at signup must match `PublisherDisplayName` in the manifest exactly, because Product identity hands it back as one of the three required values. Decide on it before building the first `.msix`.
- Reserve "Phramer" as soon as the account exists, because the reservation lapses after 3 months without a submission. If the name is already taken, contact Microsoft via the infringement/contact link. Phramer's public GitHub history since 14.1.3 is evidence of prior use.
- The `Publisher` value (`CN=<GUID>`) is not something you sign with. The Store re-signs the package (see section 5), so a local test build needs a self-signed certificate whose subject equals that exact CN string.

### Gaps
- No official SLA was found for how long ID verification takes. Docs only say "instant access ... once verified" and "wait ~5 minutes". Community reports of manual-review delays were not found.
- Whether "Phramer" is free to reserve could not be checked, because apps.microsoft.com was blocked from this environment.

---

## 2. Store Policies 7.20 clauses that apply to Phramer and how to satisfy each

### Takeaway
The clauses that matter are:

- **10.1.1 / 10.1.3**: distinct name and icon, accurate source, and no "Flameshot" in keywords.
- **10.2.2 / 10.1.5**: do not install or acquire software outside the Store, so disable the GitHub/MSI updater in the Store build.
- **10.2.8**: change Windows settings only by supported methods with consent. The Print Screen registry write and the ms-screenclip default are the risk points.
- **10.2.7**: clean uninstall.
- **10.5.1**: a privacy policy URL is mandatory for any Win32/Desktop Bridge product.
- **10.5.2**: opt-in consent before uploading to Imgur.
- **10.6**: declare only capabilities you use.
- **10.4.2 / 10.1.1**: launches promptly, and the value proposition is clear at first run.
- **10.7**: localization consistency.
- **11.11 / 11.12**: age rating and user-generated content.

No policy forbids competing with, or handing off to, a built-in Windows app such as Snipping Tool.

### Cited Findings
**10.1 Accurate representation / similarity**
- "Your product and its associated metadata ... must accurately and clearly reflect the **source**, functionality, and features of your product." — [Store Policies 7.20, 10.1](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.1.1: "must not in any way attempt to mislead customers as to its actual features, functionality, or **relationship to other products**"; "title or name must be unique and must not contain marketing or descriptive text"; "**must not use a name, images, or any other metadata that is the same as that of other products unless the product is also published by you**"; "The **value proposition** of your product must be clear during the **first run experience**"; list in the most appropriate category. — [Store Policies 7.20, 10.1.1](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.1.2: "Your product must be fully functional". 10.1.3: search terms must "Not exceed seven unique terms", be relevant, contain no pricing terms, and "Not use other product titles unless those products are also published by you." 10.1.4: "distinct and informative metadata ... active presence in the Store". — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.1.5: the product may, with consent, enable acquisition of "Other products published by you as long as the other products are also distributed through the Store and the acquisition of those products is through the Store." — [Store Policies 7.20, 10.1.5](https://learn.microsoft.com/windows/apps/publish/store-policies)

**10.2 Security (self-updating, dependencies, uninstall, Windows settings)**
- 10.2.2: "must not attempt to fundamentally change or extend its described functionality ... through any form of **dynamic inclusion of code**", for example by downloading and running a remote script. — [Store Policies 7.20, 10.2.2](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.2.3: no malware, and "must not offer to install secondary software that is not developed by you". — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.2.4: "may depend on non-integrated software ... to deliver its primary functionality if you **disclose the dependency at the beginning of the description**". Non-Microsoft drivers or NT services generally aren't allowed. — [Store Policies 7.20, 10.2.4](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.2.5 (Store-only install and update) applies to "All game products ... and any products offered on XBOX consoles". It does **not** cover non-game PC apps. — [Store Policies 7.20, 10.2.5](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.2.7: "must clearly communicate and enable a user's ability to **cleanly uninstall** and remove your product". — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.2.8: "You are required to use **supported methods** and must obtain **user consent** to change any user's Windows settings, preferences, settings UI, or modify the user's Windows experience in any way. Unsupported methods include ... use of accessibility APIs or undocumented or unsupported APIs in unsupported ways." It links to the 2023 pinning/defaults blog. — [Store Policies 7.20, 10.2.8](https://learn.microsoft.com/windows/apps/publish/store-policies)
- That blog says users control defaults through "consistent, clear and trustworthy Windows provided system dialogs and settings". Apps "may only offer features to lead users to the appropriate dialog or setting". Windows added a Settings deep-link URI for changing defaults. — [A principled approach to app pinning and app defaults in Windows (Windows Experience Blog, 17 Mar 2023)](https://blogs.windows.com/windowsexperience/2023/03/17/a-principled-approach-to-app-pinning-and-app-defaults-in-windows/)
- Registering a protocol: "only register for a URI scheme name if you expect to handle all URI launches for that type of URI scheme ... you must provide the end user with the functionality that is expected". Packaged apps declare `windows.protocol` in the manifest. — [Handle URI activation](https://learn.microsoft.com/windows/apps/develop/launch/handle-uri-activation)

**10.3 / 10.4 Testable and usable**
- 10.3: the product must be testable. Give demo credentials if a login is needed, and any required server must be up. — [Store Policies 7.20, 10.3](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.4.1: must detect an incompatible device at launch and show requirements. 10.4.2: "must start up promptly, continue to run and remain responsive ... shut down gracefully". — [Store Policies 7.20, 10.4](https://learn.microsoft.com/windows/apps/publish/store-policies)

**10.5 Personal information / privacy policy**
- 10.5.1: if the product "accesses, collects or transmits Personal Information ... you must maintain a privacy policy" and enter its URL in Partner Center. It must cover "the Personal Information accessed, collected or transmitted ..., how that information is used, stored and secured, ... the types of parties to whom it is disclosed ... the controls that users have ... and how they may access their information", and be kept up to date. "**Product types that inherently have access to Personal Information must always have privacy policies. These include ... Desktop Bridge and Win32 products.**" — [Store Policies 7.20, 10.5.1](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.5.2: publishing personal information to an outside service needs **opt-in consent** given in the product UI, after explaining use and recipients, plus a way to rescind it later. 10.5.4: transmit securely with modern cryptography. — [Store Policies 7.20, 10.5.2/10.5.4](https://learn.microsoft.com/windows/apps/publish/store-policies)
- On the Properties page: "If it doesn't [collect PI], the URL is optional—however, Microsoft may still require one based on the capabilities declared in your app package. Failure to include a required privacy policy may result in certification failure." — [Enter app properties for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/enter-app-properties)

**10.6 Capabilities, 10.7 Localization, 10.9 Notifications**
- 10.6: "The capabilities you declare must legitimately relate to the functions of your product ... You must not circumvent operating system checks for capability usage." — [Store Policies 7.20, 10.6](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 10.7: "You must localize your product for all languages that it supports. The text of your product's description must be localized in each language that you declare." — [Store Policies 7.20, 10.7](https://learn.microsoft.com/windows/apps/publish/store-policies)
- Store listing languages default to the languages in your packages, but "you have flexibility to remove languages for which you don't wish to provide a Store listing". — [Add and edit Store listing info for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info)
- 10.9: "must respect system settings for notifications and remain functional when they are disabled". — [Store Policies 7.20, 10.9](https://learn.microsoft.com/windows/apps/publish/store-policies)

**10.8 Financial / 10.10 Ads**
- 10.8 applies only if there are in-product purchases, subscriptions or financial data. 10.8.3: "Products from individual accounts cannot require financial information for primary functionality". The "Financial information" definition includes "**API secret keys**". 10.10 applies only if the product shows ads. — [Store Policies 7.20, 10.8.3](https://learn.microsoft.com/windows/apps/publish/store-policies)
- The 7.16.1 change (18 Jul 2022) removed the June-2022 language aimed at charging for open-source software. — [Change history for Microsoft Store Policies](https://learn.microsoft.com/windows/apps/publish/store-policies-change-history)

**11.x Content**
- 11.1: metadata must merit PEGI 12 / ESRB E10+ or lower. 11.2: all content must be original, licensed, used with permission, or otherwise lawful, and IP owners can report infringement. 11.11.1: obtain an IARC rating at submission. 11.11.2 (updated in 7.20): keep it "accurate and current throughout the product lifecycle". 11.11.3: content that could merit a higher rating needs an opt-in filter. — [Store Policies 7.20, 11.x](https://learn.microsoft.com/windows/apps/publish/store-policies)
- 11.12 defines UGC as "content that users contribute to a product and which can be viewed or accessed by other users in an online state". UGC products need terms or content guidelines, an in-product reporting route, and so on. 7.20 adds duties when integrating a third-party UGC platform. — [Store Policies 7.20, 11.12](https://learn.microsoft.com/windows/apps/publish/store-policies); [Change history 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies-change-history)
- 11.16: live generative AI must be disclosed in metadata and in Partner Center. This does not apply: Phramer's OCR is Windows.Media.Ocr, not generative AI. — [Store Policies 7.20, 11.16](https://learn.microsoft.com/windows/apps/publish/store-policies)

**Built-in Windows features / Snipping Tool**
- The full 7.20 text has no clause restricting apps that replace or compete with built-in Windows features. The only related clauses are 10.2.8 (supported methods and consent for Windows settings and defaults) and 10.2.4 (disclosing a dependency on other software). Screenshot tools already in the Store include ShareX (GPLv3). — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies); [ShareX on Microsoft Store](https://www.microsoft.com/en-us/p/sharex/9nblggh4z1sp); [ShareX GitHub](https://github.com/ShareX/ShareX)
- WACK's "blocked executables" test flags launching programs such as cmd or PowerShell (Microsoft Q&A answer about a Win32 MSIX failure). — [Microsoft Q&A: reproduce certification errors](https://learn.microsoft.com/answers/a/12314384)

### Inferences
How each clause maps to Phramer (these are recommendations, not Microsoft statements):

- **10.1.1 / 10.1.3 / 11.2 (fork risk).**
  - Keep the distinct name "Phramer" and a distinct icon. Never use the Flameshot logo or name in the title, icon, screenshots or keywords.
  - Do state the source honestly in the description, for example "Phramer is an independent fork of the open-source Flameshot project (GPL-3.0); it is not affiliated with or endorsed by the Flameshot developers." 10.1 requires an accurate "source" and forbids misleading "relationship to other products". A factual attribution in the description is not a search term, so 10.1.3 isn't breached.
  - Make sure no screenshot shows Flameshot branding, such as the About dialog or legacy strings.
- **10.1.1 first-run value / 10.4.2 for a tray-only app.**
  - A tray app that shows nothing at launch risks "not functional" or "unclear value" findings.
  - Phramer already has a first-run welcome tour. Make sure it appears on first Store launch.
  - Make a second launch from the Start menu, while the daemon is already running, do something visible: open a capture or the settings window, plus a tray balloon. Testers and WACK launch the app through its Start tile.
- **10.1.5 / 10.2.2 (self-updating).**
  - Strictly, no clause forbids self-update for a non-game PC app, since 10.2.5 covers only games and Xbox.
  - But Phramer's updater downloads and runs an MSI from GitHub. That is acquiring and installing software outside the Store (10.1.5) and could install a second, non-Store copy beside the MSIX.
  - Compile out or disable the updater in the Store build (e.g. force `checkForUpdates` off and hide the UI). The Store delivers updates.
- **10.2.8 (Windows settings).**
  - The Print Screen toggle writes the undocumented-for-apps registry value `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`. Even with consent, that is arguably an "unsupported method". The policy line most aligned with the 2023 blog is to deep-link the user to the Settings page instead (Accessibility → Keyboard, `ms-settings:easeofaccess-keyboard`) and let them flip the switch.
  - The ms-screenclip handler should be declared through the manifest `windows.protocol` extension in the MSIX, never by writing `UserChoice`/HKLM Classes. Windows lets the user pick the default.
- **10.2.4 (Snipping Tool handoff).** Video handoff is optional and off by default, so it is not "primary functionality". Disclosure isn't strictly required, but one sentence in the description is cheap insurance: "Optional screen recording hands off to Windows Snipping Tool".
- **10.2.7 (clean uninstall).** MSIX removes package files and the virtualized app data. Anything Phramer writes outside the package survives uninstall and should be avoided or documented:
  - non-virtualized registry such as the Control Panel key,
  - the legacy ms-screenclip cleanup,
  - a `phramer.ini` beside the exe (impossible under WindowsApps anyway).

  The packaging researcher should confirm which writes MSIX virtualizes.
- **10.5 (privacy).**
  - A privacy policy URL is mandatory because Phramer is a Win32 product.
  - It should say:
    - captures, OCR and UI Automation text reading happen on-device;
    - nothing is sent anywhere unless the user chooses Upload, in which case the image goes to Imgur under Imgur's terms, with the delete link kept locally;
    - settings are stored locally;
    - there is no telemetry or account;
    - if any network update check remains, the request to api.github.com reveals the IP address;
    - how to delete data (uninstall, delete history) and a contact email.
  - Keep the upload confirmation dialog on, since `uploadWithoutConfirmation` defaults to false. A first-use explanation of where images go satisfies 10.5.2's "opt-in consent".
  - Host the policy on GitHub Pages or a repo file (e.g. `PRIVACY.md`).
- **10.8.3 / Imgur client ID.** The embedded Imgur client ID is an API credential inherited from upstream Flameshot. Get Phramer's own Imgur client ID, and never ask users for secrets. Alternatively, drop the uploader from the Store build to simplify privacy, IARC and UGC.
- **10.6.** Declare only `runFullTrust`. Declare `internetClient` only if the build still has network features (full-trust processes aren't actually gated by it, but declaring it is harmless and honest). Declare no other restricted capabilities (see section 3).
- **10.7.** Phramer ships 48 translations. If the manifest's `<Resources>` lists all those languages, Partner Center proposes 48 listing languages, and 10.7 then expects localized descriptions. Either declare only the languages you will write listings for, or remove the extra listing languages in Partner Center and keep the app genuinely localized.
- **11.12.** An Imgur upload makes an image reachable by URL, but other Phramer users don't view it "in the product". So Phramer probably isn't a UGC product under 11.12's definition. This is an interpretation, not a Microsoft ruling.

### Gaps
- No official statement was found on whether Windows lets a packaged (MSIX) app declare `ms-screenclip` as a protocol. Windows owns that scheme and some `ms-` schemes are reserved. This needs a test on a real machine.
- No Microsoft document addresses the PrintScreenKeyForSnippingEnabled value specifically. The "unsupported method" reading is an inference.
- No public certification report was found for a tray-only app or a fork rejected under 10.1.1. Community evidence on these failure modes is thin.

---

## 3. Restricted capabilities: runFullTrust justification, unvirtualizedResources, allowElevation

### Takeaway
`runFullTrust` is required for a full-trust Win32 package and is routinely approved. You justify it in a free-text box on the **Submission options** page, and it generally isn't asked again on updates. `allowElevation` and `unvirtualizedResources` are effectively unavailable to a non-game individual app and Phramer needs neither.

### Cited Findings
- `runFullTrust`: "a package needs this capability if the package uses features for which full trust is needed. A common example is a package that contains one or more full-trust apps". A full-trust app runs at medium integrity. — [App capability declarations: Restricted capabilities](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- On upload, Partner Center detects restricted capabilities, and "you will be required to provide details about how your product uses each capability on the Submission options page ... this may add some additional time". "If we approve ... You generally will not have to repeat the capability approval process when you submit updates (unless you declare additional capabilities)." If it is rejected, certification fails and you resubmit without the capability or with a better justification. — [App capability declarations](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities); [Manage submission options for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/manage-submission-options)
- Optional pre-approval: you may request approval in advance (required only for development-sandbox, i.e. Xbox, titles). "this process typically takes 5 business days or longer". — [App capability declarations](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- `allowElevation`: "enables apps developed by Microsoft partners or enterprise organizations to maintain existing desktop functionality that depends on auto-elevation ... For Microsoft Store submissions, this capability is subject to approval under **strict criteria**. If you intend to use this capability, contact reportapp@microsoft.com in advance with a detailed justification." — [App capability declarations](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- `unvirtualizedResources`: "designed for certain types of desktop PC games that are published by Microsoft and our partners. It's also needed for apps packaged with external location ... It is **not intended to be used for other scenarios**, because it could compromise the system's ability to uninstall cleanly." `modifiableApp` "will not be granted for other scenarios". — [App capability declarations](https://learn.microsoft.com/windows/apps/package-and-deploy/app-capability-declarations#restricted-capabilities)
- "Note that there are some restricted capabilities which will very rarely be approved." — [Manage submission options for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/manage-submission-options)

### Inferences
- Phramer should declare `rescap:runFullTrust` only.
- It needs no elevation: the MSI was per-machine only for install location. Elevation-dependent features, such as writing HKLM `RegisteredApplications` for ms-screenclip, must be removed or replaced by manifest declarations.
- `unvirtualizedResources` would very likely be refused. The registry-sharing reason it exists for (other processes reading HKCU written by the app) is exactly what the Print Screen toggle relies on, which is another reason to switch that feature to a Settings deep link.
- Draft runFullTrust justification (≈80 words): "Phramer is an existing Win32 (Qt 6, C++) desktop screenshot and annotation tool packaged as MSIX. It must run as a full-trust desktop process to: register system-wide hotkeys (RegisterHotKey) for Print Screen / Win+Shift+X capture; capture all monitors via desktop duplication / screen grab APIs at native DPI; show a frameless always-on-top overlay across monitors; run a system-tray icon; and read on-screen text through UI Automation for OCR verification. None of this is available to an AppContainer app."

### Gaps
- Microsoft doesn't publish approval rates or timings for `runFullTrust`. "Routinely approved" rests on the many full-trust Win32 apps in the Store (e.g. ShareX, GIMP), not on an official statement.

---

## 4. Open source / GPL-3.0, other GPL apps in the Store, forks, and Flameshot trademark/naming

### Takeaway
GPL-3.0 apps are accepted in the Store. ShareX (GPLv3, a screenshot tool), GIMP, Inkscape and Krita are all published there by their own projects. The App Developer Agreement makes the developer responsible for meeting FOSS obligations such as source availability, and Partner Center has an "Additional license terms" field to point customers to the GPL.

A **"Flameshot" listing already exists in the Store** (product 9NDMNZGSL4X0), and **winget carries `Flameshot.Flameshot`**. Who published that Store listing could not be verified, and security reporting on trojanized Store utilities names Flameshot. Flameshot's README has no trademark policy. So the safest posture is a fully distinct name and icon, with factual attribution in the description only.

### Cited Findings
- Current App Developer Agreement: **version 8.11, published 17 Mar 2026, effective 17 Apr 2026**; v8.10 was published 12 Sep 2025. (Known from the search-result titles of the official PDFs; the PDFs could not be opened here.) — [MS Store ADA v8.11 PDF](https://cdn-dynmedia-1.microsoft.com/is/content/microsoftcorp/microsoft/store/documents/legal/ada/fy26/MS.Store.ADAv8.11.US.EN.pdf); [ADA v8.10 PDF](https://cdn-dynmedia-1.microsoft.com/is/content/microsoftcorp/microsoft/store/documents/legal/ada/fy26/MS.Store.ADAv8.10.EN.US.pdf)
- ADA FOSS terms, from a **search-engine summary only** and not verified against the PDF: if any code is "licensed under any FOSS License, developers are solely responsible for compliance with those license terms and conditions including any source code availability requirements", and the license grant to Microsoft excepts "any material subject to any FOSS licenses". — [ADA v8.11 PDF (search snippet)](https://cdn-dynmedia-1.microsoft.com/is/content/microsoftcorp/microsoft/store/documents/legal/ada/fy26/MS.Store.ADAv8.11.US.EN.pdf)
- Historical context: since the Windows 8 Store (2012), the ADA has allowed apps under OSI-approved licenses, including GPLv2/GPLv3/AGPLv3/LGPL. — [Jean-Baptiste Kempf, "Windows Store and the GPL"](https://jbkempf.com/blog/Windows-Store-and-the-GPL/); [The Register forum: Microsoft welcomes OSI open source to Win8 store](https://forums.theregister.com/forum/all/2011/12/08/open_source_windows_8_windows_store/)
- The **Microsoft Publisher Agreement** (commercial Microsoft Marketplace, not the consumer Store) treats any GPL, LGPL or AGPL version as an "Excluded License" (search snippet). This is a different agreement from the Store ADA and shouldn't be confused with it. — [Microsoft Publisher Agreement](https://learn.microsoft.com/en-us/legal/marketplace/msft-publisher-agreement)
- Partner Center "Additional license terms": leave it blank to use the **Standard Application License Terms**, or enter a single URL (shown as a link) or up to 10,000 characters of your own terms. There is also a "Copyright and trademark info" field (200 characters). — [Add additional information for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/add-additional-information)
- ShareX, a GPLv3 screenshot, annotation and upload tool, is in the Microsoft Store (product 9NBLGGH4Z1SP), now including a native ARM64 build. — [ShareX on Microsoft Store](https://www.microsoft.com/en-us/p/sharex/9nblggh4z1sp); [heise: ShareX ARM64 via Microsoft Store](https://www.heise.de/en/news/ShareX-Beautify-screenshots-without-an-extra-app-11274145.html); [ShareX GitHub](https://github.com/ShareX/ShareX)
- GIMP (free; MSIX build), Inkscape (free) and Krita (paid, US$14.99) are posted by their original developers. GIMP warns "You might see other 'GIMP' on the Microsoft Store ... use the link provided by the developers" (search summaries of these pages). — [GIMP 2.10.32 is on the Microsoft Store](https://www.gimp.org/news/2022/06/18/gimp-2-10-32-on-microsoft-store/); [Krita and Inkscape hit the Microsoft Store](https://en.linuxadictos.com/krita-and-inkscape-finally-hit-the-microsoft-store.html); [OMG! Ubuntu: Inkscape & Krita in Windows Store](https://www.omgubuntu.co.uk/2017/06/inkscape-krita-windows-store)
- In June-July 2022, Microsoft proposed (7.16) and then withdrew (7.16.1) policy text against charging for open-source software. The trigger was third parties re-listing free OSS for money. — [Change history](https://learn.microsoft.com/windows/apps/publish/store-policies-change-history); [TechCrunch](https://techcrunch.com/2022/07/15/dissecting-microsofts-delayed-policy-to-ban-commercial-open-source-apps/); [Windows Central](https://www.windowscentral.com/software-apps/windows-11/microsoft-store-killing-open-source-app-sales-angering-developers)
- **Flameshot in the Store**: a listing titled "Flameshot - Free download and install on Windows" exists at apps.microsoft.com/detail/9ndmnzgsl4x0. Its description calls itself "the fastest and most powerful snipping tool alternative on the Microsoft Store" (search snippet). The publisher could not be checked because the page was blocked. — [Flameshot listing (apps.microsoft.com)](https://apps.microsoft.com/detail/9ndmnzgsl4x0?hl=en-US&gl=US)
- Official Flameshot README Windows install options: Scoop, GitHub release packages, Docker. It does **not** list the Microsoft Store, winget or Chocolatey as official channels, and has **no trademark, name/logo or fork policy**. License: GPLv3+ code; **logo under the Free Art License 1.3**; button icons Apache-2.0. — [flameshot-org/flameshot README](https://github.com/flameshot-org/flameshot/blob/master/README.md)
- winget package id `Flameshot.Flameshot` exists (`winget install Flameshot.Flameshot`), with a reported upgrade-duplication issue. — [winstall: Flameshot.Flameshot](https://winstall.app/apps/Flameshot.Flameshot); [flameshot-org/flameshot #4375](https://github.com/flameshot-org/flameshot/issues/4375); [winget-pkgs #311045](https://github.com/microsoft/winget-pkgs/issues/311045)
- Security context: **STORESOCKS**. Since at least October 2025, an unknown actor has published trojanized utility apps to the Microsoft Store as MSIX packages "validly signed through Microsoft's Store signing pipeline". They impersonate WinDirStat, Lightshot, screen recorders and others. At least two seller accounts ("PersonalUtilities", "SOFTWARE MARKETPLACE") had live armed apps on 27 June 2026. Search summaries (unverified, page blocked) say an unofficial Flameshot listing under "PersonalUtilities" was part of this. — [Hexastrike: STORESOCKS](https://hexastrike.com/resources/blog/threat-intelligence/storesocks-microsoft-store-apps-deliver-a-go-backconnect-proxy/); [lukeacha blog part 2 (Jul 2026)](https://blog.lukeacha.com/2026/07/microsoft-store-apps-go-backconnect.html); [MalwareTips thread](https://malwaretips.com/threads/storesocks-%E2%80%93-microsoft-store-apps-continue-to-deliver-a-go-backconnect-proxy.142114/)
- Naming guidance again: "Do not use trademarked names". If someone else uses a name you hold rights to, use the Microsoft infringement contact. — [Reserve your app name FAQ](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/reserve-your-apps-name#frequently-asked-questions); [Store Policies 11.2](https://learn.microsoft.com/windows/apps/publish/store-policies)
- After certification, Microsoft re-signs MSIX packages with its own certificate. — [App certification process for MSIX](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-certification-process#release)

### Inferences
- **GPL compliance in practice.** Set "Additional license terms" to a URL for the GPL-3.0 text, e.g. the repo's LICENSE. Put "© Phramer contributors; based on Flameshot © Flameshot contributors. GPL-3.0-or-later" in Copyright. Link the source repo from the description or the Website field. Make the About box show the license and the source URL. A public repo with matching tags for each Store version meets the "corresponding source" obligation.
- **Microsoft's re-signing** shouldn't create a GPLv3 "Installation Information" problem. That clause targets locked-down "User Products", and Windows users can still build and sideload their own builds. This is a legal interpretation, not legal advice.
- **Qt (LGPLv3) dynamic linking** inside an MSIX is the common pattern for Qt apps in the Store. Ship the Qt DLLs unmodified, include the LGPL notices, and offer the Qt source pointers.
- **Forks.** No policy bars a fork. The rules that bite are 10.1.1 ("same name, images" as another product; "relationship to other products") and 10.1.3 (no other product's title in keywords). Phramer's rename already does most of the work. Re-check that every listing asset is Phramer-branded.
  - Don't reuse the Flameshot logo. The Free Art License would legally allow it, but it would make the icon "the same as that of other products" under 10.1.1.
- **Trademark.** Flameshot publishes no trademark policy, and the only Store "Flameshot" listing may not be the upstream project. So the practical exposure is a 10.1.1/11.2 report by whoever owns that listing, or by the Flameshot maintainers, if Phramer's metadata leans on the Flameshot name.
  - Attribution only in the description, phrased as "fork of", with an explicit non-affiliation sentence, is the defensive choice.
  - Optionally tell the Flameshot maintainers before publishing.
- **The malware campaign raises the bar for trust.** In the description and on the GitHub README, give users the official Store link (Store ID) so they can tell Phramer from impostors, the same way GIMP does.

### Gaps
- The exact current ADA FOSS clause text and section number could not be read because the PDF was blocked. Verify it from the v8.11 PDF before relying on it.
- The publisher of Store product 9NDMNZGSL4X0 ("Flameshot") is unknown, as is whether it is still live or is the STORESOCKS impersonator. Check it in a browser.
- No documented case was found of a fork of an open-source project being rejected or removed for name or icon similarity.
- Whether "Flameshot" is a registered trademark anywhere is unknown. No registry search was done.

---

## 5. Submission: Pricing & availability, Properties, Age ratings, Store listing, Packages, Submission options

### Takeaway
A submission has six pages: Pricing and availability, Properties, Age ratings, Packages, Store listings and Submission options. For Phramer:

- **Pricing and availability**: Free, all markets, public, discoverable.
- **Properties**: category **Productivity** or **Photo + video** (Utilities + tools as secondary), and a privacy URL, which is mandatory for Win32.
- **Age ratings**: the IARC questionnaire. Expect the lowest rating band, possibly with interactive elements if upload remains.
- **Store listings**: description up to 10,000 characters, up to 20 features of 200 characters, at least one 1366×768+ PNG screenshot (4+ recommended, 10 maximum), up to 7 keywords.
- **Packages**: one x64 `.msix` or `.msixupload` targeting `Windows.Desktop`.
- **Submission options**: runFullTrust justification plus clear tester notes.

### Cited Findings
**Checklist of required fields**
- Pricing and availability: Markets (default: all), Audience (default: public), Discoverability (default: available and discoverable), Schedule, and **Base price** are required. Free trial, sale pricing and organizational licensing are optional. Properties: **Category** required; **Privacy policy URL required if the app collects or transmits PI**; Support contact info required only for Xbox; Contact details required for company accounts. Age ratings: all questions required. Packages: at least one package. Store listings: **Description** and **at least one screenshot** required; Store logos "Required for some OS versions"; everything else optional. Submission options: **Restricted capabilities required if declared**. — [Create app submission for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/create-app-submission)
- The submission can be automated with the Microsoft Store submission API. — [Create app submission for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/create-app-submission)

**Properties / category**
- MSIX categories include **Productivity** ("Apps which help user to complete a particular task more efficiently"), **Photo + video** ("capturing, editing, and sharing photos or videos"), **Utilities + tools** ("assist user in solving problems or completing specific tasks"), Multimedia design → Illustration + graphic design / Photo + video production, and Developer tools → Utilities. You choose one category and an optional secondary category. — [Categories and subcategories for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/categories-and-subcategories); [Set category and subcategory](https://learn.microsoft.com/windows/apps/publish/publish-your-app/pwa/categories-and-subcategories)
- 10.1.1: "should be listed in the most appropriate category and genre". — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies)

**Age ratings (IARC)**
- The Age ratings page runs the IARC questionnaire. "The first question asks you to choose the category that best describes your app", and follow-up questions depend on it. "we share your publisher display name and email address with IARC". After "Save and generate" you see the ratings. The rating is reused for later updates, but you must "retake the questionnaire by clicking the Edit button" if content changes. You can instead enter an existing IARC rating ID. Ratings can be appealed through the link in the IARC certificate email. Some markets can be blocked by a rating. — [Generate age ratings for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/age-ratings#answering-the-age-ratings-questionnaire)
- IARC uses ESRB-style descriptors plus four "Interactive Elements": **Digital Purchases, Shares Info, Shares Location, Users Interact**. Its questions cover whether users can communicate, share information (photos, videos, text), share location, or make purchases (summary of IARC and third-party explainers). — [How IARC works](https://globalratings.com/how-iarc-works/); [International Age Rating Coalition (Wikipedia)](https://en.wikipedia.org/wiki/International_Age_Rating_Coalition)
- Policy 11.11.2 (7.20): keep age-rating info current and update it promptly when it changes. — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies)

**Store listing fields and limits**
- Description: required, up to **10,000** characters of plain text. What's new: **1,500** characters, and "If this is the first time you're submitting your app, leave this field blank". Product features: up to **20**, each ≤ **200** characters, no bullets. Short description: up to 1,000 characters, but only ~270 are shown in some views; if omitted, the first paragraph (≤500 characters) is used. Don't put HTML, code or URLs in the description; links go in their own fields. — [Add and edit Store listing info for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info)
- Screenshots: PNG, ≤ 50 MB each. **Desktop: 1366 × 768 or larger, 4K (3840 × 2160) supported**. Only **one** is required; up to **10 desktop** screenshots; at least four recommended (the FAQ says 5–8). Captions ≤ 200 characters. Keep key content in the top two-thirds, and don't add extra logos or marketing text. — [Add app screenshots, images, and trailers for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/screenshots-and-images)
- Store logos: the listing page calls them **optional** and says the Store can otherwise use the package's logos. The same page's FAQ says "A Store logo is mandatory for submission", and the submission checklist says "Required for some OS versions". This is a conflict. For apps the relevant image is the **1:1 App tile icon (300 × 300)**. 2:3 poster and 1:1 box art are mainly for games and Xbox. Trailers are optional. — [Screenshots and images for MSIX](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/screenshots-and-images); [Add and edit Store listing info](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info); [Create app submission](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/create-app-submission)
- Keywords (formerly "Search terms"): up to **7**, max **40 characters each** per the MSIX page, and **no more than 21 words** across all keywords. Partner Center offers AI-suggested keywords. — [Add additional information for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/add-additional-information). This **conflicts** with the submission API contract, which says "30 characters per search term; Up to 7 search terms; 21 unique words TOTAL". — [Microsoft Store submission API for MSI or EXE app](https://learn.microsoft.com/windows/apps/publish/store-submission-api#api-contracts)
- Copyright and trademark: 200 characters. Additional license terms: blank (SALT), a URL, or ≤10,000 characters. Developed by: 255 characters ("Published by" always shows the publisher display name). — [Add additional information for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/add-additional-information)

**Packages**
- Accepted formats: .msix, .msixbundle, **.msixupload**, .appx, .appxbundle, .appxupload. MSIX gets "Free Microsoft code signing and CDN hosting" and supports flighting. — [App package requirements for MSIX app FAQ](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-package-requirements#frequently-asked-questions)
- Version: "the last (fourth) section of the version number is reserved for Store use and **must be left as 0**"; the other sections are 0–65535, and the first can't be 0. — [App package requirements: Package version numbering](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-package-requirements#package-version-numbering)
- To keep a package off other device families, set `TargetDeviceFamily` to `Windows.Desktop` rather than `Windows.Universal`. Device family availability checkboxes apply only to new acquisitions. — [Upload MSIX app packages: Device family availability](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/upload-app-packages#device-family-availability)
- Installing MSIX from the Microsoft Store requires Windows 10 version 1809 or later. — [MSIX features and supported platforms](https://learn.microsoft.com/windows/msix/supported-platforms#microsoft-store)
- "The Packages section will show as 'Incomplete' until all required fields are completed, even if individual packages show 'Validated' status." — [Create app submission for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/create-app-submission)
- No own code-signing certificate is needed. The Store re-signs MSIX after certification. — [Get started FAQ #11](https://learn.microsoft.com/windows/apps/publish/get-started#frequently-asked-questions)

**Submission options / Notes for certification**
- Publishing hold: as soon as certified (default), manual "Publish now", or a date at least 24 h ahead. — [Manage submission options for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/manage-submission-options)
- Notes for certification should include test credentials if needed, "Steps to access hidden or locked features ... Apps that appear to be incomplete may fail certification", region differences, update changes, and the date. "A real person will read these notes ... Be succinct". Any required services must be online. — [Manage submission options: Notes for certification](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/manage-submission-options#notes-for-certification)
- You must verify your email address in Action Center to receive certification notifications. — [Manage submission options: Notifications](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/manage-submission-options)

### Inferences
- **Category.** **Productivity** fits Phramer best, with **Photo + video** as secondary. Comparable tools appear under either. Utilities + tools is reasonable too, but its examples (file managers, calculators) are less descriptive.
- **IARC answers.**
  - Choose the non-game "app" category (productivity/utility). Answer "no" to violence, sex, gambling and so on.
  - For interactive elements:
    - "Users Interact": **no**, because there is no chat or social feature.
    - "Shares Location": **no**.
    - "Digital Purchases": **no**.
    - "Shares Info": **yes**, if the Imgur uploader stays, because the user can send images to a third party. Otherwise **no**.
  - Expected result: the lowest age band in each system (e.g. ESRB Everyone / PEGI 3 / IARC 3+), possibly with a "Shares Info" notice. This is an inference; IARC computes the rating.
- **Draft listing.**
  - First line: "Capture, annotate and share screenshots from the system tray."
  - Then the features (≤20):
    - region/window/full-screen capture across monitors (mixed DPI),
    - annotation tools (arrows, shapes, highlighter, pixelate, text, numbered markers),
    - on-device OCR (Windows.Media.Ocr),
    - copy as image or as file,
    - pin to screen,
    - optional Imgur upload,
    - global hotkeys / Print Screen,
    - floating editor canvas,
    - optional Snipping Tool video handoff.
  - Then the attribution and non-affiliation line, and the GPL / source link sentence. Put the source URL in the Website field rather than the description, per the "no URLs in description" guidance.
- **Keywords** (≤7, keep each ≤30 characters to satisfy both stated limits): `screenshot`, `screen capture`, `snipping tool`, `annotate`, `OCR text`, `screen snip`, `markup`.
  - "Snipping tool" is Microsoft's product title, and 10.1.3 bans other product titles in search terms. Use "snip" or "snipping" generically instead, e.g. `screen snip`.
  - Never use `flameshot` or `sharex`.
- **Screenshots.** Use 4–8 PNGs at 1920×1080 or 2560×1440: capture overlay with selection, annotation toolbar, OCR result window, editor canvas, settings/shortcuts, tray menu. Take them on a 100%/125% display, crop to the exact size, and add no marketing overlays.
- **Store logo.** Provide the 300×300 1:1 tile icon to cover the "mandatory logo" ambiguity. Include full MSIX visual assets (Square44x44, Square150x150, Wide310x150, StoreLogo) in the package, because WACK checks for missing images.
- **Version mapping.** CMake `FLAMESHOT_VERSION` x.y.z becomes MSIX `x.y.z.0`. 14.x easily fits the 0–65535 limit.
- **Draft Notes for certification:**
  > "Phramer is a free, open-source screenshot tool. No account, login or server is required. On first launch a welcome window appears; afterwards Phramer runs in the system tray (notification area; it may be in the overflow ^ menu). To test: press Win+Shift+X (or click the tray icon → Take Screenshot), drag to select an area, then use the toolbar (O = OCR text extraction, Ctrl+C copy, Ctrl+S save, Esc cancel). Print Screen can be assigned in Settings → Shortcuts; Windows' own Print Screen → Snipping Tool setting must be turned off first (the app explains how). Right-click the tray icon for Settings and Quit. Optional features: image upload (Imgur, asks before uploading) and screen recording handoff to Windows Snipping Tool (off by default; Settings → General). runFullTrust is required for global hotkeys, multi-monitor capture overlay and the tray icon."

### Gaps
- The exact current IARC question wording for non-game apps isn't publicly documented by Microsoft, and the questionnaire itself is visible only inside Partner Center.
- Whether a Store logo upload is truly required for a desktop-only MSIX app: the docs conflict, as noted above.
- The keyword character limit (40 vs 30) conflicts between Microsoft pages.

---

## 6. Certification: timeline, common rejection reasons for full-trust desktop apps, responding to a rejection

### Takeaway
Certification takes "up to three business days" and is often faster. Once published, the listing is visible in about 15 minutes. The checks are a malware scan, WACK-based technical compliance (launch, crash and hang), and human content review of the metadata and behaviour.

For a tray-only full-trust app, the realistic failure modes are:

- crashes or "cannot launch" on the test machines,
- an unclear first-run value (10.1.1) or "not fully functional" (10.1.2),
- a missing or insufficient privacy policy (10.5.1),
- metadata problems (10.1.1 / 10.1.3),
- an unapproved restricted capability.

Fix the problem and create a new submission. Questions and appeals go to reportapp@microsoft.com.

### Cited Findings
- "This process can take **up to three business days**. After your submission passes certification, on average, customers will be able to see the app's listing **within 15 minutes**". The tests are: security tests (malware), technical compliance with the **Windows App Certification Kit**, and content compliance, whose duration varies with complexity and queue. A certification report lists the failed test or policy; "After you fix the problem, you can create a new submission". Microsoft also "conduct[s] spot checks of apps after they've been published". — [The app certification process for MSIX app](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-certification-process)
- The technical compliance step "Ensures your app doesn't crash or use prohibited APIs. The Store installs and runs your app". The content check covers the listing, the age rating and descriptions. — [App certification process FAQ](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-certification-process#frequently-asked-questions)
- If the app fails or is removed: review the policies, fix, and resubmit. "For clarification or help, contact Microsoft at reportapp@microsoft.com with your app ID or respond to the certification report email directly." Best practice: submit only when complete, with no placeholders, and run WACK locally. — [App certification process (MSI/EXE) FAQ](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msi/app-certification-process#frequently-asked-questions)
- Appeal statistics (1 Jul 2025 – 30 Jun 2026):

  | Statistic | Count |
  |---|---|
  | Enforcement-action appeals | 1,118 |
  | Questions about certification, policy, submission and technical help | 4,347 |
  | Overturned decisions | 623 |
  | Average processing time | 2.37 days |

  — [Store Policies 7.20: Certification Appeal Process](https://learn.microsoft.com/windows/apps/publish/store-policies#certification-appeal-process)
- WACK launch test: WACK uses `IApplicationActivationManager::ActivateApplication`. UAC must be on and the screen at least 1024×768. Activation failures are logged under Event Viewer → Microsoft\Windows\Immersive-Shell, event IDs 5900–6000. — [Windows App Certification Kit tests](https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-app-certification-kit-tests)
- Community (Microsoft Q&A): a report "The product crashes while trying to load" on Dell Latitude/Inspiron test devices (OS build 22621). The advice was to get crash logs from Developer Support (aka.ms/storesupport) and reproduce on a clean VM at the same build. — [Microsoft Q&A: app store certification failed](https://learn.microsoft.com/answers/a/12453423)
- Community (Microsoft Q&A): a Win32 app packaged as MSIX failed with "Windows cannot find 'a path'" and the WACK test "Detects if there are references to process launch APIs and blocked executables". The advice was to test on a clean machine; absolute paths and launching cmd or PowerShell trip these checks. — [Microsoft Q&A: reproduce certification errors](https://learn.microsoft.com/answers/a/12314384)
- Community (GitHub release notes): a Store submission was rejected under **10.1.1.11** over tile art reused from electron-builder samples. — [ponyabc-desktop v0.3.17-store-candidate](https://github.com/macrokinetic-ai/ponyabc-desktop/releases/tag/v0.3.17-store-candidate)
- Community example of a 10.2.4 failure, for an app embedding WireGuard, a non-Microsoft driver/service. — [Microsoft Q&A: WireGuard 10.2.4](https://learn.microsoft.com/en-us/answers/questions/1058146/publishing-an-app-with-embedded-wireguard-on-the-m)

### Inferences
Phramer-specific pre-submission checks:

1. Install the MSIX on a clean Windows 11 VM with no Qt installed and no `phramer.ini`, and check that it launches from Start with no console window.
2. Check that the first launch shows the welcome window.
3. Run WACK against the `.msix`.
4. Check that nothing assumes a writable install folder. The portable-config path and `applicationFilePath()` relaunch both point into read-only WindowsApps.
5. Check that the debug Qt DLL pitfall noted in CLAUDE.md (`windeployqt --debug`) isn't repeated, because certification would see "no Qt platform plugin" as a crash on launch.
6. Check that "Open with" (Ctrl+O) and the Snipping Tool handoff use ShellExecute of documented URIs, not cmd or PowerShell.
7. Check that the privacy URL resolves.

If rejected: read the report for the policy number, fix, and resubmit (a new submission). If you disagree, reply to the report email or write to reportapp@microsoft.com with the Store ID. For crashes, ask Developer Support for the dumps.

### Gaps
- Microsoft publishes no current statistics on rejection reasons for desktop or full-trust apps. The list above combines policy text with scattered community reports.
- No first-hand report was found of a tray-only app failing for "no window". The risk is inferred from 10.1.1 (first-run value) and 10.3 (testability).

---

## 7. Updates after publication, staged rollout, pointing existing MSI users to the Store

### Takeaway
Each update is a new submission. Customers get the highest-versioned applicable package, so updates must carry a higher version, with the fourth field still 0. Gradual rollout (a percentage of users, then finalize or halt) is available for MSIX updates, as is a "mandatory update" flag if the app uses the Store update APIs.

Existing MSI users can be pointed at the listing with `ms-windows-store://pdp/?ProductId=<StoreID>` or `https://apps.microsoft.com/detail/<StoreID>`. A final MSI release could open that link from its in-app updater.

### Cited Findings
- The Store "will always use the highest-versioned package that is applicable to the customer's ... device". You can submit packages in any order, and you can roll back new acquisitions by re-uploading an older package. Customers who already got the newer one keep it until you ship a higher version. — [App package requirements: Package version numbering](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/app-package-requirements#package-version-numbering)
- Gradual package rollout: tick "Roll out update gradually after this submission is published" on the Packages page of an **update**, set a percentage, then adjust it from the app Overview. You must **Finalize** or **Halt** before creating the next submission. Halting doesn't roll back users who already updated. The Store listing changes apply to everyone immediately. This is only for MSIX app updates. — [Gradual package rollout](https://learn.microsoft.com/windows/apps/publish/gradual-package-rollout)
- "Make this update mandatory" sets a date and time and requires the app to use the Windows.Services.Store APIs to check, download and install updates (Windows 10 1607+). — [Upload MSIX app packages](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/upload-app-packages#device-family-availability)
- Package flights can target specific groups of users with different packages. — [Package flights](https://learn.microsoft.com/windows/apps/publish/package-flights#gradual-package-rollout)
- Beta via a hidden listing: "Make this product available but not discoverable in the Store → Direct link only". It must be **Free** for testers. Switch to discoverable in a later submission. — [Beta testing and targeted distribution](https://learn.microsoft.com/windows/apps/publish/beta-testing-and-targeted-distribution#targeted-distribution-with-a-link-to-your-app's-listing)
- Store links: the web link is `https://apps.microsoft.com/detail/<Store ID>` (from Product identity). The protocol link is `ms-windows-store://pdp/?ProductId=<Store ID>` ("recommended"; PFN and AppId forms are deprecated). "Starting February 2026, the Microsoft Store will no longer support activations made through the mode=mini parameter". A rating deep link also exists: `ms-windows-store://review/?ProductId=`. — [Using ms-windows-store URIs](https://learn.microsoft.com/windows/apps/develop/launch/launch-store-app#opening-to-a-specific-product); [View product identity details](https://learn.microsoft.com/windows/apps/publish/view-app-identity-details#link-to-your-app's-listing)
- Campaign tracking: append `?cid=<id>` (web) or `&cid=<id>` (protocol) to attribute acquisitions. — [Create a custom app promotion campaign](https://learn.microsoft.com/windows/apps/publish/create-a-custom-app-promotion-campaign#embed-a-custom-campaign-id-to-your-app's-microsoft-store-page-url)
- Store badge and Web Installer: the badge generator at apps.microsoft.com/badge has a "Direct" mode that downloads a small installer which installs the app from the Store backend without opening the Store app (free apps). — [How to use the Microsoft Store Web Installer](https://learn.microsoft.com/windows/apps/distribute-through-store/how-to-use-store-web-installer-for-distribution#frequently-asked-questions)

### Inferences
- **"Strictly increasing" is not enforced by Partner Center**, since you may upload a lower version to roll back. But existing customers only update to a higher version, so keep the CLAUDE.md rule: the git tag becomes x.y.z.0.
- **Use the same version numbers for MSI and MSIX** (e.g. 14.2.0 → 14.2.0.0) so support questions map 1:1.
- **Path for existing MSI users.**
  - Ship one last MSI release whose updater, instead of downloading an MSI, offers "Phramer is now on the Microsoft Store" and opens `ms-windows-store://pdp/?ProductId=<id>&cid=msi-migration`.
  - Have it tell users to uninstall the MSI version, or detect the Store copy and offer to remove the MSI. The MSI (per-machine, WiX upgrade GUID) and the MSIX (package family) are separate identities and will coexist; the packaging researcher should confirm.
  - Keep the GitHub `releases/latest` checksum flow working for users who stay on the MSI.
- **Use gradual rollout** (e.g. 10% → 50% → finalize) for risky updates such as Qt upgrades. MSIX gives no per-user rollback.

### Gaps
- No Microsoft documentation was found on migrating users from a per-machine MSI to a Store MSIX of the same app, or on whether the Store treats them as related. This belongs in the packaging notes.

---

## 8. Fees, tax/payout forms, and whether a Company account is needed

### Takeaway
There is no fee: individual registration is free and so is publishing a free app. A free app needs **no payout account and no tax forms**. A Company account isn't needed unless the publisher name looks like a business or the app needs financial information. Phramer needs neither.

### Cited Findings
- The registration fee is waived in the new individual flow; company onboarding is also free ("Select Company account (free)"). — [Free developer registration](https://learn.microsoft.com/windows/apps/publish/whats-new-individual-developer); [Steps to open a developer account (company)](https://learn.microsoft.com/windows/apps/publish/partner-center/open-a-developer-account#step-by-step-flow)
- "If you only plan to list free offers, you don't need to set up a payout account or fill out any tax forms." You can add them later if you start selling. — [Manage your account in Partner Center: Financial details](https://learn.microsoft.com/partner-center/marketplace-offers/manage-account-settings-and-profile); [Set up payout and tax profiles](https://learn.microsoft.com/partner-center/account-settings/set-up-your-payout-account)
- A company account is required "if your product requires financial information for primary functionality ... or if a reasonable consumer would interpret your application or publisher name to be that of a business entity". Company accounts must provide business verification and customer-support contact details that are shown on the product page in some regions (EU DSA). — [Store Policies 7.20, 10.14](https://learn.microsoft.com/windows/apps/publish/store-policies); [Change history 7.18](https://learn.microsoft.com/windows/apps/publish/store-policies-change-history)
- Individual accounts can't distribute crypto wallet or trading apps (10.2.6) or apps whose primary function requires financial info (10.8.3). Neither applies. — [Store Policies 7.20](https://learn.microsoft.com/windows/apps/publish/store-policies)
- MSIX code signing by the Store is free, so no certificate needs to be bought. — [Get started FAQ](https://learn.microsoft.com/windows/apps/publish/get-started#frequently-asked-questions)

### Inferences
- Total cash cost is $0. The only costs are time, a hosted privacy-policy page (GitHub Pages works) and, optionally, a domain.
- Accepting donations later (GitHub Sponsors links) doesn't change this. 10.8.2 governs donations only if they are taken *in-app* or unlock features.

### Gaps
- None material.

---

## 9. Consolidated step-by-step checklist (account → "In the Store")

### Takeaway
Fifteen steps, in the order they block each other. Steps 1–2 take about a day. Steps 3–9 are engineering and content work. Steps 10–15 are Partner Center work. Certification then takes up to 3 business days.

### Cited Findings
(Each step's sources are cited in sections 1–8 above. The overall flow comes from [Get started with Microsoft Store: App submission](https://learn.microsoft.com/en-us/windows/apps/publish/get-started#app-submission-[msix-pwa]) and [Create app submission for MSIX apps](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/create-app-submission).)

### Inferences
1. **Account.**
   - Sign up at storedeveloper.microsoft.com → Individual (free) → personal MSA → ID + selfie.
   - Pick a non-business-sounding **Publisher display name**.
   - Verify your email in Action Center (My Preferences) so certification mail arrives.
   - No tax or payout setup is needed.
2. **Reserve "Phramer".** Partner Center → Apps & games → New product → MSIX or PWA app. The reservation is valid for 3 months. Open **Product identity** and copy:
   - Package/Identity/Name,
   - Package/Identity/Publisher (`CN=…`),
   - Package/Properties/PublisherDisplayName,
   - the Store ID.
3. **Make a Store build flavor of Phramer.**
   - Updater off.
   - Print Screen toggle replaced by a Settings deep link.
   - ms-screenclip registered only through the manifest, or dropped if Windows refuses the scheme.
   - No HKLM writes, no writes beside the exe.
   - Config in the per-user app data folder.
   - Imgur uploader either removed or given its own client ID and an explicit first-use consent.
   - About box shows the GPL text and the source URL.
4. **Manifest.**
   - Identity values exactly as copied, with Version `x.y.z.0`.
   - `TargetDeviceFamily Name="Windows.Desktop"` (MinVersion ≥ 10.0.17763 for Store MSIX).
   - `rescap:runFullTrust` only.
   - `Resources` limited to the languages you'll write listings for.
   - Full visual assets.
5. **Package and test.**
   - Build `.msix` (x64) or `.msixupload`.
   - Sign it locally with a self-signed certificate whose subject equals the Publisher CN.
   - Install on a clean Windows 10/11 VM.
   - Run **WACK**.
   - Check first-run window, tray, hotkeys, OCR, uninstall cleanliness.
6. **Privacy policy page.** Cover on-device processing, Imgur upload (if kept), no telemetry, user controls, and contact. Publish it at a stable URL.
7. **License and attribution.**
   - GPL-3.0 link ready for "Additional license terms".
   - Copyright line.
   - Source repo with a tag matching each Store version.
8. **Listing assets.**
   - Description, with the attribution and non-affiliation sentence and the optional Snipping Tool dependency sentence.
   - Up to 20 features.
   - Short description.
   - 4–8 PNG screenshots ≥1366×768.
   - 300×300 tile icon.
   - ≤7 keywords (no "Flameshot" or other product titles).
9. **Decide markets.** All markets, Free, public, discoverable. Or start "available but not discoverable – direct link only" for a beta.
10. **Partner Center → Start submission → Pricing and availability.** Free, markets, schedule.
11. **Properties.** Category Productivity, secondary Photo + video. Privacy URL. Website (the GitHub repo). Support contact (an email or GitHub Issues URL). No product declarations needed.
12. **Age ratings.** Complete the IARC questionnaire as described in section 5, then Save and generate.
13. **Packages.** Upload. Uncheck every device family except Desktop. Wait for "Validated".
14. **Store listings.** Enter the text and images per language, plus copyright and Additional license terms (the GPL URL).
15. **Submission options.**
    - runFullTrust justification.
    - Notes for certification (tray app, hotkeys, Print Screen caveat, no login, optional features).
    - Publishing hold if you want to time the announcement.

    Then **Submit for certification**. Expect a result within ≤3 business days and visibility about 15 minutes after publishing. Status becomes **In the Store**. If it fails, fix and resubmit, or write to reportapp@microsoft.com.

After publishing:
- Add the Store badge or link to the README and the GitHub releases page.
- Ship the final MSI release that points to `ms-windows-store://pdp/?ProductId=<id>`.
- Use gradual rollout for later updates.
- Re-run IARC if features change (11.11.2).
- Keep the privacy policy current (10.5.1).

### Gaps
- The exact behaviour of the WACK checks and certification testers on a tray-first Qt app can only be confirmed by a test submission. A private or hidden first submission (direct-link-only discoverability) is a low-risk way to find out before a public launch.
