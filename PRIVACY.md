# Phramer privacy policy

Last updated: 2026-10-01

Phramer is a screenshot capture and annotation tool for Windows. This policy
covers the Microsoft Store edition and the installer and portable builds
published on GitHub.

## What Phramer collects

Nothing. Phramer has no account, no telemetry, no analytics and no
advertising. It does not send your screenshots, text or settings to the
developer or to anyone else.

## What happens on your device

Everything Phramer does with your data happens on your own computer:

- **Screenshots** are taken when you ask for one, and stay in memory until
  you save, copy, pin or discard them. Saved files go to the folder you
  choose.
- **Text recognition (OCR)** uses the text recognition built into Windows
  (Windows.Media.Ocr). It runs on your device.
- **Window text and outlines.** To offer clean window selection and better
  OCR results, Phramer reads window positions and, for OCR, the text of the
  window under a capture through Windows UI Automation. This is read only
  while a capture is open and is never stored or transmitted.
- **Settings** are kept in a `phramer.ini` file in your user profile (or,
  for the portable build, next to the program).

Images and text leave Phramer only when you copy, save, drag or open them in
another application yourself.

## Network access

- **Microsoft Store edition:** makes no network requests. Updates are
  delivered by the Microsoft Store.
- **Installer build from GitHub:** checks
  `api.github.com/repos/MichaelNguyen5653/Phramer/releases/latest` once at
  start and once a day for a newer version, and downloads the new installer
  from GitHub only when you choose to update. GitHub receives your IP
  address and a standard HTTP request with these checks, as with any web
  request; see GitHub's privacy statement. You can turn the check off in
  Settings > General.

## Removing your data

Uninstalling the Microsoft Store edition removes its settings. For the other
builds, delete the `phramer` folder under `%APPDATA%`. Screenshots you saved
remain where you saved them.

## Children

Phramer does not collect personal information from anyone, including
children.

## Changes and contact

Changes to this policy are published in this file, with the date above.
Questions: open an issue at
https://github.com/MichaelNguyen5653/Phramer/issues
