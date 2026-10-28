[Documentation](../README.md) > Features > Particles | [Français](../../fr/features/particles.md)

# Particles

Hammer Pro has a particle browser that lists every particle system of the mounted particle files (`.pcf`), and a particle preview that plays the effects of `info_particle_system` entities in the 3D views, like Hammer++.

## Particle browser

### Opening it

- **View > Panels > Particle browser** adds the **Particles** tab to the right dock (or removes it). Saved as `show_particle_browser` in `[browser]` (default off).
- The browse button of a particle key (`effect_name` of `info_particle_system`) in the entity panel opens it in pick mode.
- No default shortcut. With the Hammer++ keymap preset (**Settings > Keyboard**), the shortcut is `Shift+F11`.
- The `⊞` button (**Open in a window**) moves the browser into a floating window; `⊟` (**Back to the dock**) puts it back.

### Options

| Control | What it does | Default |
|---|---|---|
| **Grid** / **List** | Thumbnail grid, or one row per system with a small thumbnail, its particle file and its source badge. Kept for the session. | Grid |
| Tile size slider (grid only) | **Tile size** of the particle and model grids, 48 to 256 points by steps of 8. `tile_size` in `[browser]`. | 96 |
| **Search particle systems (!word excludes)** | Keeps the systems whose name or file contains every word; `!word` excludes. | |
| Particle file dropdown | **All particle files**, or the systems of one file. | All |
| Source chips **All**, **Game**, **Addons**, **Workshop** | Keep the systems of one kind of source (a chip appears only when the content has it). | All |
| **Garry's Mod content only** | Only what every Garry's Mod player has. Shared with the model and sound browsers (`gmod_only` in `[browser]`). | Off |
| **Systems: N** | Number of systems listed ("Reading the particle files..." while they load). | |

Thumbnails show the material of the system (a `?` when it has none or it is missing). Hover a system for its name, file, material and source.

### Using a system

- Click selects a system.
- Double click (or `Enter` while the pointer is over the list): in pick mode, writes the system into the key; otherwise sets it on the selected `info_particle_system` entities.
- Footer: the selected name, its file and source, **Apply to selected (N)** (sets the system as `effect_name` of the selected `info_particle_system` entities, one undo step) and **Copy name**.

### Picking for a key

A particle key of the entity panel (see [Entity properties](entity-properties.md)) has a `▾` button that lists the particle systems of the game content with their file (searchable, and you can type a value that is not listed), and a browse button ("Choose in the particle browser (double click a system)").

In pick mode the browser shows "Pick for *key*: double click an item, or select it and press Use." with the current value selected. Double click, `Enter` or **Use** writes the system (one undo step) and returns to the **Entity** tab; **Cancel** leaves pick mode.

## Particle preview in the views

### Turning it on

| Option | Where | What it does | Default |
|---|---|---|---|
| **Show particles** | **View > Display options**, and **Settings > Nav mesh and particles** | Plays the effects of `info_particle_system` entities near the camera in the 3D views. `show_particles` in `[lab]`. | On |
| **Particles of the selection only** | Same places | Plays only the effects of the selected particle systems. `particles_selected_only` in `[lab]`. | Off |

### What plays

- The definitions come from the particle files of the mounted content: the files of the map's particle manifest (`maps/<map>_particles.txt`) first, then the particle catalog of the content. They are read in the background, with the look of their materials (`$additive`, `$overbrightfactor`, `$addself`, fade sizes, the sprite sheet of the base texture). Child systems are loaded too.
- Every effect plays whatever its start state (`start_active`), at its control points: the entity itself with its angles, and the entities named by its `cpoint1` to `cpoint63` keys. Continuous effects start already running (1.5 seconds ahead).
- To keep the cost low whatever the map size, only effects within 4096 units of the camera play, at most 24 at once (nearest first), with at most 3000 particles in all and 400 per effect.
- Particles are drawn by the renderer with their sprite sheet frame, screen aligned, upright, flat or stretched (trails, ropes), blended or additive. The preview is an approximation: some operators of the particle system are not simulated.
- In the [player view](player-view.md) with the compiled world, the particle systems of the compiled map play instead.

Hover or select an `info_particle_system` in a 3D view to see its effect name above it, with its state: nothing more when it plays, "(not playing: too far or too many)", "(loading...)" or "(not found in the particle files)".

## Tips

- On a map with many effects, turn on **Particles of the selection only** to watch one effect at a time.
- Through MCP, `search_particles` searches the systems of the content and `list_particle_systems` reports the state of each `info_particle_system` of the map (see the [MCP tool reference](../../MCP.md)).

## FAQ

**An effect says "not found in the particle files".** No mounted particle file defines that name. Check the spelling, mount the game or addon that has it, or add its file to the map's particle manifest.

**An effect does not play although it is in view.** It is more than 4096 units away, or 24 nearer effects already play. Move the camera closer.

**Why does the browser list nothing?** The game content is not loaded, or the search, file filter, source chips or **Garry's Mod content only** hide everything.

## Related pages

- [Entity properties](entity-properties.md)
- [Render modes and display](render-modes-display.md)
- [Player view](player-view.md)
- [Model browser](model-browser.md)
- [Sound browser](sound-browser.md)
- [Editor performance](../troubleshooting/performance.md)

---
[Back to the documentation index](../README.md)
