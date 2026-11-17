Cryo Engine is a native 3D game engine with a visual editor, scripted in the Cryo language.

## Download

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `CryoEngine-v0.19.0-beta-windows-x86_64.zip` |
| macOS 11+ (Apple Silicon and Intel) | `CryoEngine-v0.19.0-beta-macos-universal.zip` |
| Linux (x86_64) | `CryoEngine-v0.19.0-beta-linux-x86_64.zip` |

Unzip and run. On first launch the editor creates the example game in `~/CryoProjects/FrostpeakIsles` and starts a welcome tour.

**macOS:** the app is not signed. Right-click **Cryo Engine.app** and choose **Open** the first time. If macOS still refuses, run `xattr -dr com.apple.quarantine "Cryo Engine.app"` in a terminal.

## New in 0.19.0-beta

- **RPG systems** (File > RPG: items, skills, talents…, saved in `rpg.json`): an inventory with stacks and equipment (weapon, armour, trinket), levels and experience with talent points, a skill bar with cooldowns (keys 1-4), and a talent tree with columns, tiers, ranks, requirements and skills it teaches. In-game inventory and talent screens (I and K by default, gamepad too) in the colours and fonts of the game's menus. Scripts: `inventory.*`, `level.*`, `skills.*`, `talents.*`, and events `on level_up`, `on skill`, `on item_used`, `on talent_learned`... The player's progress goes with them to the next level and can be saved.
- **Game HUD editor** (File > Game HUD…, saved in `hud.json`): place texts, bars, the scripts' bars, boss bars, pictures, the skill bar, level and XP, crosshair, minimap and a message feed on nine anchors, drag them in the preview, choose colours, sizes, fonts, shadows and corners. New script calls: `hud.set(key, value, max)`, `hud.message(text)`, `hud.show(id, on)`; texts show values with `{key}`.
- **Free fonts and sounds** in the examples: Cinzel Decorative and Alegreya Sans (Ashen Keep), Orbitron and Rajdhani (Outpost Kestrel), all under the SIL Open Font License; menu clicks, inventory and level-up sounds by Kenney (CC0). Each project gets `CREDITS.txt` and the licences.
- **Ashen Keep becomes an action RPG**: experience and loot from every enemy, the Bone Brute drops the Knight's Blade, the armoury an Iron Hauberk, the first shrine an Ember Ring, the Warden his Cleaver; ten levels; 11 talents in three columns teach Whirlwind, Shield Bash, War Cry, Ember Burst and Second Wind; armour, lifesteal, guard and stamina regeneration change the fight. Its HUD has a minimap, the loot feed, the skill bar and experience.
- Outpost Kestrel has a tactical HUD (health and magazine at the bottom corners, minimap, messages).

## From 0.18.0-beta

Screens of your own, game options, loading screens, confirmations, hints and more menu looks.

## Details of 0.18.0-beta

- **Screens of your own** (Game menus > Screens): level select, difficulty, game over, a codex... each with a title, text, buttons and whether it pauses. Open one with a button ("Open a screen") or from a script: `menu.open("gameover")`, `menu.close()`, `menu.is_open`.
- **Options of your game** (Game menus > Options): on/off, sliders and choices that appear on the in-game Options screen, kept per player. Scripts read them with `option("difficulty", "Normal")` (already set when `on start()` runs) and hear changes with `on option_changed(id, value)`. You can also choose which built-in parts (graphics, frame rate, volume, full screen, controls) the Options screen shows.
- **Loading screen** between levels (Game menus > Loading): text, picture, a random tip, a minimum time and a progress bar.
- **"Are you sure?"** on any button (Quit, Restart and Main menu ask by default); No is highlighted first, and Esc, B or No return to the button that asked.
- **Hints**: a line of text shown while a button is highlighted.
- **More look options**: the project's own .ttf/.otf fonts for the title and texts, capitals, text shadow, fade-in, a slide on the highlighted button, panel colour, sounds for moving and choosing, and a slow turn of the camera behind the main menu.
- **Test from the main menu** button in the Game menus window, and a preview of every tab (main, pause, the selected screen, loading).
- The examples use all of it: Ashen Keep has a Difficulty option (enemies hit harder and have more health on Hard) and loading tips; Outpost Kestrel has mouse sensitivity and invert-look options.
- Usability fixes found by playing the exported games: going back from Options, Credits or a confirmation now lands on the button you came from (it used to jump to the first or last); the Controls window stays above the menus, which wait while it is open; the camera behind the main menu also stays behind Options and Credits opened from it; check boxes are outlined so they show on dark panels.

## From 0.17.0-beta

Main menu and pause menu for games, customisable in File > Game menus…, with an Options screen.

## Details of 0.17.0-beta

- **Main menu for games**: exported games open on a title screen (title, subtitle, logo, background picture or a camera of the level behind, music, corner text) with buttons to start, continue a saved checkpoint, open options, controls and credits, and quit.
- **Customisable menus**: **File > Game menus…** edits the main and pause menus (buttons, their order and actions: start, continue, options, controls, credits, resume, restart, main menu, load a scene, send an event to scripts, quit) and their look (position, colours, sizes, spacing, rounded corners), with a live preview. Saved in `menus.json`.
- **Options screen** inside games: graphics quality, frame-rate limit, volumes, full screen, controls.
- Menus work with the mouse, the keyboard (arrows/WS, Enter, Esc) and the gamepad (d-pad, A, B, Start).
- Both examples have their own menus (Ashen Keep: gold on black over the castle; Outpost Kestrel: cyan, tactical).
- Usability fixes found by playing the exported games: a click or key shorter than one frame was lost (the rifle did not fire on quick clicks when the game ran slowly); several presses in one frame now all count in menus; the HUD, banner and crosshair no longer show through menus.

## From 0.16.0-beta

Gamepads, remappable controls, volume sliders, prefabs and `scene.load`.

## Details of 0.16.0-beta

- **Gamepads**: Xbox, PlayStation, Switch Pro and most other controllers work in every game with no changes. The left stick moves (`input.axis`), the right stick looks like the mouse, buttons are `pad_a`, `pad_b`... and Start opens the pause menu.
- **Remappable controls**: bindings make one name stand for several inputs (by default `space` is also the A button, `mouse_left` the right trigger...). Players change them in **Controls** (pause menu, or Window > Controls and gamepad), kept per game; a project can list its own in `input.json`.
- **Volume**: sliders for everything, music and effects in the pause menu and the Window menu.
- **Prefabs**: right-click > Save as prefab writes an entity and its children to `prefabs/`; double-click places a copy; `scene.spawn_prefab(file, position)` places one while playing, with its scripts running.
- **Several levels**: `scene.load("scenes/Level2.cryoscene")` switches scene while playing.
- Fix: the pause menu of exported games (since 0.12) was never drawn; Esc only paused.
- Linux builds need `libudev-dev` (gamepads).

## From 0.15.0-beta

Video cutscenes, Create > Cutscene with subtitles and fades, faster rendering (lamps per object, shadow caching) and much faster rebuilds.

## Details of 0.15.0-beta

- **Video cutscenes**: **File > Import video…** turns a film into a `.cryovid` in `videos/` that every exported game plays the same way, with its sound. Animated GIFs convert directly; MP4, MOV, WebM, MKV and AVI need ffmpeg on the computer where the game is made; without ffmpeg, a folder of numbered pictures (plus `sound.wav`) works too. Play it with a **Play video** event on the Timeline or `video.play(...)`; the game waits, Enter/Space/Esc skips, `on video_end(file)` follows.
- **Making cutscenes in the engine**: **Create > Cutscene** makes a clip with its own camera at the editor's view, a fade in, a first subtitle and the cut back to the game. New Timeline events **Subtitle** and **Fade**, and `subtitle(text, seconds)` and `fade(amount, seconds)` for scripts. Cutscenes show cinema bars and a "skip" hint.
- **Faster games**: each object is shaded only by the lamps that reach it; shadows of lamps that stay still are redrawn every other frame; shader settings are looked up once instead of hundreds of times a frame.
- **Faster builds**: `cargo build --release` is now incremental (a change to the engine rebuilt in about 29 s here; now about 5 s). Development builds optimise dependencies and keep only line numbers (the debug editor was 400 MB). The shipped build is the new `dist` profile (thin LTO, stripped), used by `scripts/package.sh` and the release workflow.

## From 0.14.0-beta

Frame-rate limit (30, 60, 120, none), render scale on dense screens, smooth terrain texture blending, first check on macOS.

## Details of 0.14.0-beta

- **Frame-rate limit**: 30, 60, 120 or no limit, in **Window > Frame rate limit** and in the pause menu of games (60 by default). "No limit" also turns off the screen's own sync, after a restart.
- **Checked on macOS** (Apple M4, macOS 27): the engine builds, all tests pass and both example games run. The first measurement showed that Retina screens were the bottleneck (2880 x 1760 pixels), which led to the next two changes.
- **Render scale**: on very dense screens the 3D picture is drawn up to 1260 lines high (High), 1080 (Medium) or 900 (Low) and stretched to the window; text and HUD stay sharp. `CRYO_RENDER_SCALE=1` forces full size.
- Ambient occlusion and bounced light are computed at half size (they are soft anyway).
- **Terrain**: the ground and cliff textures now blend smoothly by slope, with the rock's own detail breaking up the edge.
- Outpost Kestrel: the rifle is held a little turned in and rolled, so the hands show more.
- `scripts/check-mac.command` keeps its logs in `target/check` and reports each game's exit code.

## From 0.13.0-beta

High-resolution hands, terrain, screen-space global illumination, levels of detail for animated models, the benchmark.

## Details of 0.13.0-beta

- **High-resolution first-person hands**: about 118,000 triangles, made from code (`tools/make_hands.py`): palm, knuckles, fingernails, fingerless tactical gloves with a knuckle guard, 2048 x 2048 skin and fabric textures. Every finger has three joints and is curled automatically around the rifle's grip and handguard; Idle, Fire and Reload bend each joint.
- **Terrain**: **Create > Terrain** makes ground with hills from a grayscale height map, with a texture for gentle ground and another for steep slopes. Its Mesh collider is a height field, so characters walk its slopes and rays hit it. Ashen Keep now has mounds in the bailey and wooded hills and crags around the castle; Outpost Kestrel sits in a basin of ridges.
- **Global illumination in screen space**: Bounced light now bounces the light of everything on screen (sun, lamps, glowing parts) onto the surfaces around it, instead of the old approximation.
- **Levels of detail for animated models**: rigged models get simpler versions too, which keep their skeleton and move the same way.
- **Measuring performance**: `cryo MyGame --bench` writes `bench-report.txt` (average and 1% low frame rate, median, 95% and 99% frame times, graphics card); F3 shows the frame rate in a game; `scripts/check-mac.command` builds, tests and benchmarks on a Mac.
- Fixes: black squares that could appear near many glowing sparks; a thin pistol grip and a trigger on the rifle.

## From 0.12.0-beta

Glowing and transparent materials, glass windows, levels of detail, pause menu with graphics quality, animated hands.

## Details of 0.12.0-beta

- **Glowing and transparent materials**: emissive colour (MTL `Ke`, glTF `emissiveFactor`) feeds the bloom; transparent materials (MTL `d`/`Tr`, glTF alpha Blend) are drawn after solid objects, far to near, and reflect more at grazing angles. Outpost Kestrel's buildings now have glass windows.
- **Levels of detail**: big models (over 2500 triangles) get two simplified versions automatically; distant objects use them, and tiny far objects are skipped.
- **Bounced light**: lamps and torches add a soft fill to nearby surfaces (Scene settings, Bounced light). An approximation, not full global illumination.
- **Pause menu in games**: Esc pauses and opens Resume, Graphics quality (High, Medium, Low), Full screen and Quit. The quality is saved and also chosen in the editor's Window menu.
- **Animated first-person hands**: the FPS hands are a rigged model with Idle, Fire and Reload clips, driven from the script with `animate`.

## From 0.11.0-beta

PBR materials and normal maps, point-light shadows, particles, path finding, positional sound, saving.

## Details of 0.11.0-beta

- **Physically based materials**: GGX lighting with roughness and metallic per material (MTL `Pr`/`Pm`, glTF metallic-roughness), and **normal maps** (MTL `norm`/`map_Bump`, glTF `normalTexture`), with no tangents needed in the file. Materials without these values keep using the entity's Shininess.
- **Shadows from point lights**: torches and lamps now cast shadows too (a cube of six views). Together with spotlights they share 16 shadow tiles, given to the lights nearest the camera.
- **Particles**: `fx.burst(position, "sparks")` with sparks, smoke, dust, fire, embers, magic, explosion and blood, optional count, colour and direction. Glowing particles feed the bloom.
- **Path finding**: `scene.path(from, to)` returns the corners of a walkable way around walls and crates.
- **Positional sound**: `audio.play_at(file, position)` gets quieter with distance from the camera.
- **Saving**: `save.set`, `save.get`, `save.has`, `save.remove`, `save.clear` keep values between sessions.
- **Examples**: every surface has a normal map and its own finish; bullet impacts, explosions, sword hits, torch embers and magic use particles; enemies in Ashen Keep walk around obstacles; the rifle sways with the mouse and bobs with each step; both games remember a record (best time, victories).

## From 0.10.0-beta

Post-processing (bloom, ambient occlusion, screen-space reflections), spotlight shadows, 1024 textures, hands in the first-person view.

## Details of 0.10.0-beta

- **Post-processing**, turned on per scene (Scene settings): **bloom** (glow around lights and glowing objects), **ambient occlusion** (shading in creases, corners and under objects), and **screen-space reflections** (glossy floors and metal reflect what is on screen). The scene is drawn into an off-screen HDR image with its own multisampling, then a filmic tone curve, a soft vignette and dithering finish the picture. `CRYO_POST=0` turns it off for very old graphics cards.
- **Spotlight shadows**: the four spotlights nearest the camera cast shadows, besides the sun or moon.
- **High-resolution textures**: the example games' surfaces are now 1024 x 1024 (JPEG), with anisotropic filtering so floors stay sharp at a distance.
- **Hands in the first-person view**: Outpost Kestrel shows gloved hands and sleeves holding the rifle; they follow recoil and reloading.
- OBJ materials with a tinted or dark diffuse colour now tint their texture (containers, banners and gloves show their real colours).
- Outpost Kestrel's lamps and ceiling lights are now spotlights that cast shadows; both example games use the new effects.

## From 0.9.0-beta

Sky reflections with Fresnel and the filmic tone curve, the rounded rifle, camera collision in Ashen Keep.

## Details of 0.9.0-beta

- **More lifelike lighting** (in scenes with Relief on): glossy surfaces mirror the sky, most strongly at grazing angles; the texture decides how polished each spot is; and a filmic tone curve gives deeper shadows and softer highlights.
- **Outpost Kestrel**: a rifle with rounded parts instead of blocks, a reload movement, floors that catch the lamps, and drones that keep clear of each other.
- **Ashen Keep**: the camera no longer passes through walls, and the loose rocks are shaded correctly.

## From 0.8.0-beta

The Outpost Kestrel example, raycasts, relief lighting and the crosshair.

## Details of 0.8.0-beta

- **Outpost Kestrel**, a third example game: a first-person shooter with one complete level (yard, warehouse, reactor room), an automatic rifle with recoil and reloading, two kinds of security drones that hunt by line of sight, pickups, doors that open as sectors are cleared and a reactor to shut down. File > New example: Outpost Kestrel.
- **Raycasts**: `scene.raycast(origin, direction, distance, ignore)` returns the first solid thing along a ray.
- **Relief lighting**: a scene setting that makes textures bump the light, without normal maps. Ashen Keep uses it too.
- `crosshair(true)` draws an aiming mark.

## From 0.7.0-beta

The dark fantasy art pass of Ashen Keep.

## Details of 0.7.0-beta

- **Ashen Keep, dark fantasy art pass** (original designs): a cold, overcast evening with thicker fog; a worn knight with uneven pauldrons, a cowl and a torn cloak; hunched skeletons of differing heights in rusted gear; rust on all metal; dry grass, slate roofs, ruined walls and rubble in the bailey.

## From 0.6.0-beta

Skinning on the graphics card, view culling, faster physics, and the more detailed Ashen Keep models.

## Details of 0.6.0-beta

- **Skinning on the graphics card**: rigged models are posed by the vertex shader instead of the processor, so animated characters cost almost nothing per frame and can be far more detailed. Skeletons with more than 64 joints still use the processor.
- **View culling**: objects outside the camera's view are not drawn, and distant ones do not cast shadows.
- **Faster physics**: a broad phase skips pairs of bodies that are nowhere near each other (a scene with 440 bodies went from 2.6 ms to 1.2 ms per frame in our test).
- **Ashen Keep, redone models**: the knight, the Warden and the skeletons now have about 6 to 10 thousand triangles each (they had under 2 thousand), smooth skin weights at the joints, real anatomy for the skeletons (ribcage, spine, pelvis, skull with jaw and teeth, finger bones) and layered plate armour over mail. Characters and scenery use generated textures for steel, mail, cloth, leather, bone, stone, cobbles and wood, with relief painted into the colour.

## From 0.5.0-beta

The Ashen Keep example game, HUD bars and banners, distance fog.

## From 0.4.0-beta

Quick time events, global events and tags, maps, `match` and state machines in the language, the first-person player, skeletal animation, mesh colliders, bodies that rotate, export for another system.

## Known limits

Skeletal animation runs on the CPU, mesh colliders are for static bodies, shadows come from one directional light, and there are no loop cuts or bevels in the modeller yet. Script error messages are in English only.
