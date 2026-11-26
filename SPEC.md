# Yoin: first-session spec

Yoin is a personal DAW for getting an idea out of my head and into music with as little friction as possible. This page describes what the first usable version must do. Anything not listed here waits.

## Who it is for

Me. I have a mic and a mouse, no MIDI keyboard, and no music theory background. Ideas arrive as a hummed melody, a beat in my head, or a sound I hear (a water drop, a sample) that suggests a melody. My first real project is a soundtrack for an indie game, so the goal is finished, arranged tracks exported as audio files.

## The six things I do when an idea hits

1. **Capture it by voice, instantly.** One click or key starts recording from the mic. No project setup, no track arming, no dialogs. I can hum a melody or beatbox a rhythm and it is saved as a clip. The mic and the speakers or headphones I want are easy to pick: a visible device selector shows the current input and output, a level meter confirms the mic is heard, and Yoin remembers my choice.
2. **Turn any sound into an instrument.** I drop in a sample or record a sound, and Yoin lets me play it at different pitches, so a water drop can carry a melody.
3. **Splice it precisely.** I can cut any recording or sample into pieces exactly where I want: zoom into the waveform, place cuts by hand or snapped to the beat, trim the edges, and move, copy, reorder, or delete the pieces. Cuts never click or pop, and the original audio is never destroyed, so any cut can be adjusted or undone later. For drum loops like a breakbeat, Yoin can slice the loop into its individual hits automatically (by detecting each hit, or by even beat divisions), let me nudge the slice points, and then let me re-sequence the hits on the grid to build a new pattern, with each slice playable on its own.
4. **Loop it.** I select a clip or a region and it repeats seamlessly at the project tempo while I listen and add to it.
5. **Layer parts on top.** While the loop plays I add more tracks: another sample, built-in drums, a built-in piano, or another mic take. Notes can be clicked into a simple grid that keeps me in key, so I don't need theory to make something that sounds right.
6. **Arrange and export a song.** I lay loops out along a timeline into sections (intro, main, ending), then export one audio file (WAV, and a compressed format for sharing) that I can drop into a game or send to someone.

## Principles

- **Idea first.** From launch to recording is one action.
- **Samples are the focus.** Built-in piano and drums exist, but sample handling is the best part of the app.
- **No theory required.** Scale-locked grid, sensible defaults, nothing that assumes I can read music.
- **Fully understood.** Every control on screen is something I know how to use. A feature is added only when I need it.
- **Samples are editable material.** Splicing is a core tool with fine control, not an afterthought.
- **Small and simple UI.** One window, few panels, no hidden modes.

## Not in the first version

VST/plugin hosting, MIDI keyboard input, mixing and mastering effects beyond volume and pan, hum-to-notes conversion, live collaboration, the web client, and the Mac build.

## Technical frame

Rust audio engine, TypeScript web UI, Tauri desktop shell. Windows first, Mac later. Keep the engine and UI separated so a web client with live collaboration stays possible.

## Done means

I can open Yoin, hum an idea, build it into a looping multi-track piece using my own samples and the built-in drums and piano, arrange it into a full track, and export an audio file, all without leaving the app or reading a manual.
