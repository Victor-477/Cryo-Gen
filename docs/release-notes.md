Cryo Engine is a native 3D game engine with a visual editor, scripted in the Cryo language.

## Download

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `CryoEngine-v0.4.0-beta-windows-x86_64.zip` |
| macOS 11+ (Apple Silicon and Intel) | `CryoEngine-v0.4.0-beta-macos-universal.zip` |
| Linux (x86_64) | `CryoEngine-v0.4.0-beta-linux-x86_64.zip` |

Unzip and run. On first launch the editor creates the example game in `~/CryoProjects/FrostpeakIsles` and starts a welcome tour.

**macOS:** the app is not signed. Right-click **Cryo Engine.app** and choose **Open** the first time. If macOS still refuses, run `xattr -dr com.apple.quarantine "Cryo Engine.app"` in a terminal.

## New in 0.4.0-beta

- **Quick time events**: `qte.press`, `qte.mash`, `qte.hold` and `qte.sequence` show the prompt and answer with `on qte_success(name)` / `on qte_fail(name)`. On the Timeline, a QTE event makes a cutscene wait for the player.
- **Better events**: `emit("event")` reaches every script, `global.name` shares values, objects have a Tag (`scene.tagged("enemy")`), and scripts can declare `on touch_start`, `on touch_end`, `on key_down` and `on key_up`. New Timeline events: event for everyone, play animation, game speed.
- **Language**: maps (`{ coins: 3 }`), `match`, state machines (`state patrol { ... }` and `goto`), `time.scale` for slow motion, and many new list, text and math functions.
- **First-person camera**: Create > First-person player, with mouse capture (`input.lock_mouse`).
- **Skeletal animation**: rigged glTF models keep their skeleton and clips; `self.animate("Walk")` switches clips with a blend. Create > Animated model (example) adds one to try.
- **Timeline easing**: ease in, ease out, overshoot and bounce.
- **Physics**: triangle-mesh colliders for static models, and bodies that can rotate (roll, spin, tip over).
- **Lights and shadows**: scenes may have any number of point and spot lights (the 16 nearest the camera are used); the reach of the shadows is adjustable.
- **Export for another system**: export a Windows, macOS or Linux game from any of them.
- Two new tutorials: events and quick time events; first person and animated models.

An existing project keeps its scripts: new template scripts such as `FirstPerson.cryo` are added when you use the Create menu.

## From 0.3.0-beta

Timeline with keyframes, cutscenes, events in sync, timers, and Edit Mesh modelling.

## Known limits

Skeletal animation runs on the CPU, mesh colliders are for static bodies, shadows come from one directional light, and there are no loop cuts or bevels in the modeller yet. Script error messages are in English only.
