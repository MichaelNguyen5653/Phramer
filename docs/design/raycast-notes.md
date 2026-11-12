# Raycast study (design reference)

Source: Raycast 2.11.0 for Windows (Store package), installed files read in
place on 2026-11-11. Static inspection only; nothing was executed or copied
into this repo. We take the method, not the assets, class names or code.

## Stack (observed from shipped files)

| Layer | What ships | Evidence |
|---|---|---|
| Shell | .NET + WPF host window, tray via H.NotifyIcon | `Raycast.dll`, `PresentationFramework.dll`, `Wpf.Ui.dll`, `H.NotifyIcon.Wpf.dll` |
| UI | React app in WebView2, one HTML entry per window | `frontend/*-window.html`, `Microsoft.Web.WebView2.Wpf.dll`, `WebView2Loader.dll` |
| Bundler | Rolldown/Vite, CSS modules, per-view code split | `rolldown-runtime-*.js`, hashed `*-settings-view-*.css` |
| Backend | Bundled Node process with Rust native addons | `backend/node.exe`, `backend/*.win32-x64-msvc.node` |

Inference: the smooth feel comes from the web layer and its motion rules,
not from a native UI toolkit. Tauri (Rust + WebView2) can match it.

## Method worth copying

- **One zoom unit.** Every size is `N * --spx`, rounded to whole pixels.
  Sharp at any DPI, and a single variable scales the whole UI.
- **Fixed scales, no free values.** Spacing 2/4/6/8/12/16/24/32/40/48/56,
  radius 2/4/6/8/10/12/18/full. 8 and 4 dominate spacing; 4 and 6 dominate radius.
- **Foreground-alpha surfaces.** Hover, borders and fills are the text colour
  at 5/10/20/40/60 % alpha, so one `--fg`/`--bg` pair themes everything.
- **Hairlines.** Borders use a half-pixel variable (0.5 px on HiDPI) and inset
  shadows rather than 1 px borders.
- **Type.** Inter Variable for UI, JetBrains Mono for code. 13 px body,
  11 px secondary, weights 400/500/600.
- **Motion budget.** 100–200 ms for hover and state (150 ms most common),
  250–300 ms for panels, longer only for ambient loops.
- **Animate cheap properties.** Opacity, colour, transform and scale carry
  almost all transitions; layout properties are rare. `will-change` is set
  on moving elements.
- **Two curve families.** Expo-style ease-out for entrances; overshoot
  (back/spring) curves for pops and toggles.
- **Blur as depth.** Backdrop blur in tiers, with saturation boost on overlays.
- **Reduced motion is honoured** throughout.
- **Per-window entry points.** Each window loads only its own bundle.

## Applied to Phramer

Tokens live in `app/src/tokens.css`. Values are our own choices on the same
kind of scale. Rules for every UI change:

1. Sizes come from tokens. No raw pixel values in components.
2. Transition only opacity, colour, transform. Never width/height/top/left.
3. Interaction feedback within 150 ms; nothing blocks input while animating.
4. Changes apply live (colour, size, font). No OK/Apply step.
5. `prefers-reduced-motion` turns transitions into instant state changes.
