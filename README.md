# Soundboard

A lightweight Windows soundboard built with Rust and Tauri. Play clips to your
speakers and to a virtual microphone at the same time, so Discord and games hear
them, trigger sounds with global hotkeys or from your phone, and mix your own
voice in.

- Installer: about 3 MB. Installed program: about 14 MB.
- Memory: the app process itself uses about 30 MB of private memory while idle.
  The window is drawn by Windows' built-in WebView2 (the Edge engine that is
  already part of Windows 10 and 11), which runs as separate shared processes.
- No Electron, no bundled browser, no background services. One process, one
  tray icon.

Download the latest installer from the
[Releases](https://github.com/ahad-hossin/Soundboard/releases) page.

## Features

**Playing sounds**
- Grid of square tiles. Click to play, click again to restart. Different sounds
  overlap.
- Drag and drop audio files onto the window, or use Add / the plus tile.
  Supports mp3, wav, ogg and flac.
- Dual output: play to your speakers and to a virtual microphone (for example
  VB-Cable) at once, so you hear what the call hears.
- Per-sound volume and master volume, both shown as percentages.
- Stop All button and hotkey.

**Microphone passthrough**
- Pick your real microphone in Settings and the app mixes your voice into the
  virtual mic together with the sounds. Discord only needs one input device.
- Mic gain control. Passthrough restarts itself if a device drops out.

**Hotkeys**
- A global hotkey per sound and one for Stop All. They work while a game or any
  other window is focused.
- Conflicts are detected before saving.

**Tiles that are easy to tell apart**
- Every sound gets its own vibrant color from a ten-color retro palette,
  assigned least-used-first, with a subtle motif so no two tiles look alike.
- Optional image per sound (png, jpg, webp, gif), shown edge to edge.
- While a sound plays the tile shows a ring and a progress sweep that matches
  the clip length.
- Drag tiles to reorder. Rename, change color or image, set hotkey, delete
  from the right-click menu.

**Trim audio**
- Right-click a tile and choose Trim audio: a waveform editor with drag
  handles, exact start and end times, and Play selection.
- Trims are non-destructive. The original file is untouched and Reset restores
  it. Importing a single file opens the editor right away.

**Phone remote**
- The app serves a web page on your Wi-Fi. Scan the QR code in the Remote
  panel and your phone shows the same tiles with the same colors and images.
- Tap to play, Stop All, master volume, long-press a tile for its own volume.
- Rotate button for landscape, Full button for full screen, installable as a
  home-screen app.
- Optional PIN lock with lockout after repeated wrong attempts. Changing the PIN
  signs every phone out.

**Phone cannot connect?**
1. Open the Remote panel and press Allow through Windows Firewall, then accept
   the Windows administrator prompt. The panel shows "Windows Firewall:
   allowed" afterwards.
2. Make sure the PC and phone are on the same Wi-Fi, not a guest or isolated
   network.
3. Type the URL exactly as the panel shows it, including the port.

**Startup and tray**
- Start with Windows, start minimized to the tray, close to tray.
- Tray menu: Show, Stop All, Quit. Launching the app again just brings the
  window back.

**Design**
- Monochrome interface: color only means something (amber for live, red for
  stop or delete) and the sound tiles themselves.
- Keyboard first: Tab between tiles, Enter or Space plays, Esc stops all.

## Requirements

For users:
- Windows 10 or 11, 64-bit.
- WebView2 runtime. Already present on current Windows; the installer tells
  you if it is missing.
- For the virtual microphone: a virtual audio cable such as
  [VB-Cable](https://vb-audio.com/Cable/) (free). Without it the app still
  plays to your speakers.

For building from source:
- Rust (stable, MSVC toolchain) and Visual Studio Build Tools 2022 with the
  "Desktop development with C++" workload:
  ```bash
  winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
  ```
- Node.js 20 or newer.

## Setting up the virtual microphone

1. Install VB-Cable and reboot.
2. Soundboard, Settings: Speakers = your real output, Virtual mic =
   `CABLE Input (VB-Audio Virtual Cable)`, Microphone = your real microphone.
3. Discord or your game: microphone = `CABLE Output (VB-Audio Virtual Cable)`.

Your voice and the sounds now reach the call together. Your voice never plays
on your own speakers.

## Develop

```bash
npm install
npm run tauri dev
```

The window hot-reloads on UI changes and recompiles on Rust changes.

Tests:

```bash
cargo test -p soundboard-core
npm test
```

Phone page without the desktop app (fake sounds, optional PIN):

```bash
cargo run -p soundboard-core --example remote_dev 1234
```

Build the installer:

```bash
npm run tauri build
```

Output: `target/release/bundle/nsis/Soundboard_<version>_x64-setup.exe`.

## How it is built

- `core/`: Rust library with no UI dependency. Audio engine (rodio and cpal,
  WASAPI), decode cache, mic passthrough, hotkey rules, config, and the axum
  server for the phone remote. Fully unit tested.
- `src-tauri/`: the Tauri 2 shell. Commands, events, global shortcuts, tray,
  autostart, single instance.
- `src/`: the desktop UI in React and Tailwind.
- `core/src/remote/page.html`: the phone page, a single file with no framework.

Settings and copied files live in `%APPDATA%\com.yokitori.soundboard\`.

## License

GNU General Public License v3.0 or later. See [LICENSE](LICENSE).

Copyright (c) 2026 Ahad Hossin (Yokitori).

If this saves you some clicks, you can [buy me a coffee](https://buymeacoffee.com/yokitori).
