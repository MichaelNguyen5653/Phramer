<div align="center">
  <p>
    <h1>
      <img src="data/img/app/appicon-256.png" alt="Phramer" width="128" />
      <br />
      Phramer
    </h1>
    <h4>Powerful yet simple to use screenshot software.</h4>
  </p>
</div>

Phramer is a Windows screenshot and annotation tool, forked from
[Flameshot](https://github.com/flameshot-org/flameshot). It was previously
released as Flameshot v2; installs of that version update to Phramer
automatically.

## Features

- **Editor** — collect captures into one window, annotate each independently,
  then copy, save, or export the whole session to a folder.
- **OCR** — extract text from a selection, with automatic upscaling for small
  text so it reads accurately.
- **Snip across screens** — one activation, then drag a selection on any
  monitor without picking one first.
- **Shape tools** — hollow and filled squares and circles, plus highlight
  shapes with adjustable border thickness.

**Windows only.**

## Preview

![image](data/img/preview/animatedUsage.gif)

## Installation

1. Go to the **Releases** page of this repository.
2. Download the **`.msi`** file from the latest release.
3. Run it. Windows may warn that the publisher is unknown — choose
   **More info → Run anyway**.

The installer is unsigned and installs for all users, so Windows asks for
administrator approval. Later updates are offered in the app itself.

## Usage

Phramer runs in the system tray. Start a capture from the tray icon or press
<kbd>Win</kbd> + <kbd>Shift</kbd> + <kbd>X</kbd>, then drag to select an area.

## Keyboard shortcuts

These shortcuts are available while capturing:

|  Keys                                                                     |  Description                                                   |
|---                                                                        |---                                                             |
| <kbd>O</kbd>                                                              | Extract text from the selection (OCR)                          |
| <kbd>P</kbd>                                                              | Set the Pencil as paint tool                                   |
| <kbd>D</kbd>                                                              | Set the Line as paint tool                                     |
| <kbd>A</kbd>                                                              | Set the Arrow as paint tool                                    |
| <kbd>S</kbd>                                                              | Set Selection as paint tool                                    |
| <kbd>R</kbd>                                                              | Set the Rectangle as paint tool                                |
| <kbd>C</kbd>                                                              | Set the Circle as paint tool                                   |
| <kbd>M</kbd>                                                              | Set the Marker as paint tool                                   |
| <kbd>T</kbd>                                                              | Add text to your capture                                       |
| <kbd>B</kbd>                                                              | Set Pixelate as the paint tool                                 |
| <kbd>←</kbd>, <kbd>↓</kbd>, <kbd>↑</kbd>, <kbd>→</kbd>                    | Move selection 1px                                             |
| <kbd>Shift</kbd> + <kbd>←</kbd>, <kbd>↓</kbd>, <kbd>↑</kbd>, <kbd>→</kbd> | Resize selection 1px                                           |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>←</kbd>, <kbd>↓</kbd>, <kbd>↑</kbd>, <kbd>→</kbd> | Symmetrically resize selection 2px           |
| <kbd>Esc</kbd>                                                            | Quit capture                                                   |
| <kbd>Ctrl</kbd> + <kbd>M</kbd>                                            | Move the selection area                                        |
| <kbd>Ctrl</kbd> + <kbd>C</kbd>                                            | Copy to clipboard                                              |
| <kbd>Ctrl</kbd> + <kbd>S</kbd>                                            | Save selection as a file                                       |
| <kbd>Ctrl</kbd> + <kbd>Z</kbd>                                            | Undo the last modification                                     |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Z</kbd>                         | Redo the next modification                                     |
| <kbd>Ctrl</kbd> + <kbd>Q</kbd>                                            | Leave the capture screen                                       |
| <kbd>Ctrl</kbd> + <kbd>O</kbd>                                            | Choose an app to open the capture                              |
| <kbd>Ctrl</kbd> + <kbd>Return</kbd>                                       | Commit text in text area                                       |
| <kbd>Ctrl</kbd> + <kbd>Backspace</kbd>                                    | Cancel current selection                                       |
| <kbd>Spacebar</kbd>                                                       | Toggle the side panel with tool options, colour picker and history |
| <kbd>G</kbd>                                                              | Start the colour picker                                        |
| Right Click                                                               | Show the colour wheel                                          |
| Mouse Wheel                                                               | Change the tool's thickness                                    |
| <kbd>Ctrl</kbd> + drawing *line*, *arrow* or *marker*                     | Draw only horizontally, vertically or diagonally               |
| <kbd>Ctrl</kbd> + drawing *rectangle* or *circle*                         | Keep aspect ratio                                              |

<kbd>Shift</kbd> + drag a handler of the selection area: mirror redimension in the opposite handler.

## Coming soon

- More to come.
