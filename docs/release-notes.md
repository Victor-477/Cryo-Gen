Cryo Engine is a native 3D game engine with a visual editor, scripted in the Cryo language.

## Download

| System | File |
|---|---|
| Windows 10/11 (64-bit) | `CryoEngine-v0.11.0-beta-windows-x86_64.zip` |
| macOS 11+ (Apple Silicon and Intel) | `CryoEngine-v0.11.0-beta-macos-universal.zip` |
| Linux (x86_64) | `CryoEngine-v0.11.0-beta-linux-x86_64.zip` |

Unzip and run. On first launch the editor creates the example game in `~/CryoProjects/FrostpeakIsles` and starts a welcome tour.

**macOS:** the app is not signed. Right-click **Cryo Engine.app** and choose **Open** the first time. If macOS still refuses, run `xattr -dr com.apple.quarantine "Cryo Engine.app"` in a terminal.

## New in 0.11.0-beta

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
