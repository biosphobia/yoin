# Yoin

A small DAW for getting the idea in your head into music. See [SPEC.md](SPEC.md) for what the first version is aiming at.

## What works today

- One track. Press **R** (or the red button) to record from your mic, **R** again to stop.
- **Space** plays and stops. Stop while stopped jumps back to the start. Click the waveform to move the playhead.
- Pick your microphone and speakers/headphones from the device box in the top right. Yoin remembers the choice.
- Input level meter on the track, so you can see the mic is heard.
- **Export WAV** saves the take.

## Download a Windows build

Every push builds Windows installers on GitHub Actions. Open the latest run under the repo's **Actions** tab → **Build**, and download the `yoin-windows` artifact. Inside are:

- `Yoin_x.y.z_x64-setup.exe`: the installer (recommended)
- `Yoin_x.y.z_x64_en-US.msi`: the same, as an MSI
- `yoin.exe`: the bare app; needs the WebView2 runtime, which Windows 10/11 already ship with

Windows SmartScreen may warn because the build isn't signed yet. Choose **More info → Run anyway**.

If recording is silent, check *Settings → Privacy & security → Microphone* and allow desktop apps to use the mic.

## Layout

| Path | What |
| --- | --- |
| `crates/yoin-engine` | Rust audio engine (cpal for device I/O). No UI code, so a web client can reuse the design later. |
| `src-tauri` | Tauri desktop shell. Exposes engine calls to the UI as commands. |
| `src` | TypeScript UI (Vite, no framework). `engine.ts` is the only file that talks to Rust. |

## Developing

Needs [Rust](https://rustup.rs), Node 22+, and on Windows the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (MSVC build tools and WebView2).

```sh
npm install
npm run tauri dev      # run the app with hot reload
npm run tauri build    # build installers into target/release/bundle
cargo test --workspace # engine tests
npm run dev            # UI only, in a browser, with a fake engine (no audio)
```

`cargo run -p yoin-engine --example smoke -- out.wav` records two seconds from the default mic, plays it back and saves it, without the UI.
