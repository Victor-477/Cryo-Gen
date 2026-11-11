First public beta of Cryo Engine: a native 3D game engine with a visual editor, scripted in the Cryo language.

## Download

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `CryoEngine-v0.2.1-beta-windows-x86_64.zip` |
| macOS 11+ (Apple Silicon and Intel) | `CryoEngine-v0.2.1-beta-macos-universal.zip` |
| Linux (x86_64) | `CryoEngine-v0.2.1-beta-linux-x86_64.zip` |

Unzip and run. On first launch the editor creates the example game in `~/CryoProjects/FrostpeakIsles` and starts a welcome tour.

**macOS:** the app is not signed. Right-click **Cryo Engine.app** and choose **Open** the first time. If macOS still refuses, run `xattr -dr com.apple.quarantine "Cryo Engine.app"` in a terminal.

## New in 0.2.1-beta

- **English interface**, with Portuguese as a second language (🌐 menu, top right).
- **Built-in tutorials**: eight guided tours that highlight the part of the editor they explain (Help > Tutorials, or F1). The welcome tour starts on first launch.
- **Frostpeak Isles**, a 3D platformer example: triple jump, backflip, long jump, ground pound, wall jump, cap throw, cap bounce and creature capture, all written in Cryo.
- **Documentation site**: https://victor-477.github.io/Cryo-Gen/
- Scripts: `on collide(other, normal)` now receives the contact normal; new `entity.trigger`.
- File > Open or create project; the editor remembers the last project.
- Fixed an OpenGL warning on macOS for meshes without a texture.

## Known limits

No glTF animations yet, no rotational physics, shadows from one directional light, and export only for the system you are running on. Script error messages are in English only.
