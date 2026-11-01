# Soundboard

Windows desktop soundboard. Plays local clips to your speakers and a virtual
microphone at the same time, from tiles, global hotkeys, or a phone on your Wi-Fi.

## Develop

### Prerequisites

- Rust (stable, MSVC toolchain)
- Node 20 or newer
- Visual Studio Build Tools 2022 with the "Desktop development with C++" workload (needed to link the Tauri app on Windows)

```bash
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

```bash
npm install
npm run tauri dev
```

Tests:

```bash
cargo test -p soundboard-core
npm test
```

If only the GNU toolchain links on your machine, the core crate tests still run with `cargo +stable-x86_64-pc-windows-gnu test -p soundboard-core`.

Phone page without the desktop app (fake sounds, optional PIN):

```bash
cargo run -p soundboard-core --example remote_dev 1234
```

## Build an installer

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/nsis/Soundboard_0.1.0_x64-setup.exe`.

Settings > Startup can register the app to start with Windows (minimized to the tray).

## Virtual microphone

Install [VB-Cable](https://vb-audio.com/Cable/). In Soundboard Settings pick
"CABLE Input" as the Virtual mic and your real speakers as Speakers. In Discord
or your game, choose "CABLE Output" as the microphone. To keep your own voice,
pick your real microphone under Settings > Microphone (Mic gain sets its level).
The app mixes it into the cable together with the sounds, so Discord's
microphone should be "CABLE Output" only. Your voice never plays on Speakers.

## Data

Settings and copied files live in `%APPDATA%\com.yokitori.soundboard\`.

## Manual checklist

- [ ] Sound plays on speakers alone, virtual mic alone, both.
- [ ] Discord test call hears the sound through the virtual mic.
- [ ] With Settings > Microphone set, Discord hears your voice and the sounds together; your voice is not on Speakers.
- [ ] Hotkey fires while a game window is focused.
- [ ] Phone connects via QR, plays, sees playing highlight, Stop All works.
- [ ] PIN required after setting; old phone token rejected after PIN change.
- [ ] Drag reorder persists after restart.
- [ ] Unplugging the configured device: next play falls back to default with one toast.
- [ ] Settings > Startup > Start with Windows adds a Run entry; after a reboot the app starts hidden in the tray and hotkeys work.
- [ ] Closing the window hides it to the tray; tray left click shows it; tray menu Stop All and Quit work.
- [ ] Corrupt `config.json` is backed up to `config.json.bak` and the app starts fresh with a toast.
