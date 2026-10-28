[Documentation](../README.md) > Features > Render modes and display options | [Français](../../fr/features/render-modes-display.md)

# Render modes and display options

How the views draw the map: the render mode of the 3D view, the many switches of **View > Display options**, background reference images in the 2D views, the draw distance and the frame rate limits.

## 3D render modes

Choose the mode in **View > 3D render mode**, with the render mode list of the toolbar, or with the keys:

| Mode | Shortcut | What you see |
| --- | --- | --- |
| **Textured** (default) | `Alt+1` | Materials with the editor's shading. |
| **Flat shaded** | `Alt+2` | Plain colors per object, shaded. |
| **Wireframe** | `Alt+3` | Edges only. |
| **Textured + mesh edges** | `Alt+4` | Materials with the triangle edges drawn over them. |
| **Lighting preview** | `Alt+5` | The Lit mode: the light entities light the map in real time, with shadows (see [Lighting preview](lighting-preview.md)). |
| **Textured, unshaded** | `Alt+6` | Materials without the editor shading. |
| **Smoothing groups** | `Alt+7` | Faces colored by smoothing group. |
| **Lightmap grid** | `Alt+8` | The lightmap luxel grid, colored by lightmap scale; faces with a tiny scale are red. |
| **Baked lighting preview** | `Ctrl+F5` | The lighting the game will have, computed by the built-in compiler (see [Lighting preview](lighting-preview.md)). |

**View > View type** also has **3D textured view** (`F5`) and **3D flat view** (`Shift+F5`). The mode is remembered between sessions (`view.render_mode`).

## Show menu entries

Directly in the **View** menu:

| Entry | What it does | Default |
| --- | --- | --- |
| **Show models** | Draws props as models (off: boxes). | On |
| **Show helpers and sprites** | Entity helpers (radii, cones, lines to targets) and sprites. | On |
| **Show tool textures** | Faces with tool textures in the 3D view (see [Hide and isolate](hide-isolate.md)). | On |
| **Background images...** | Reference images behind the 2D views (below). | |

## Display options

**View > Display options**. Each entry is a switch; most are also on the **Rendering** page of the settings (`[render]` table).

| Entry | What it does | Default |
| --- | --- | --- |
| **3D view information** (`O`) | Frame rate, camera position and angles in the 3D view. | Off |
| **Bounding boxes** (`I`) | Bounding boxes of the visible objects. | Off |
| **Show overlays** | Overlays drawn on their faces (see [Decals and overlays](decals-overlays.md)). | On |
| **Show connections** | Input/output lines: off, selected, all (each use goes to the next). See [I/O and logical view](io-logical-view.md). | Selected |
| **Show editor objects** | Entity icons and helper geometry that only exist in the editor. | On |
| **Show point entities** | Every point entity. | On |
| **Show entity names** | Names next to point entities in the 2D views. | On |
| **Helpers of every entity** | Helpers of every entity instead of the selected ones only. | Off |
| **Show ropes** / **Simulate ropes** | Ropes as they hang in game; simulated ropes swing while they settle. | On / Off |
| **Render effects** | Render amount, mode and color, body groups, default animation and scale of entities. | On |
| **Sky preview** | The 2D sky (worldspawn `skyname`) behind the 3D view. | On |
| **3D skybox** (`Alt+V`) | The room around the `sky_camera`, at its scale, around the map (see [Sky cameras](sky-cameras.md)). | On |
| **Collision models** | Collision hulls drawn over the props (also in the model browser preview). | Off |
| **Show model collisions** | The 3D view shows the collision models instead of the models (a model without one shows its box in orange). | Off |
| **Fog preview** | Submenu: **Off**, or the fog of one `env_fog_controller` of the map. | Off |
| **Animated materials** | Animated materials and sprites play. | On |
| **Model fade** | Props fade between their fade distances, like in game. | Off |
| **Opaque materials** | Translucent materials drawn opaque. | Off |
| **Walkable slopes** | Tints the surfaces too steep for players to stand on. | Off |
| **Show displacements** | Displacements are drawn. | On |
| **Displacement mesh in 2D views** | Switches the 2D views between the triangle grid of displacements and their outline. | Mesh |
| **Displacement walkability** | Colors displacement triangles by walkability (see [Displacement tool](displacement-tool.md)). | Off |
| **Radius culling** | Nothing farther than the culling radius from the camera is drawn (a circle shows it in the 2D views). | Off |
| **Highlight invalid brushes** | Brushes that are not closed and convex are drawn in red in every view. | On |
| **Show nodraw faces** | Shows or hides the nodraw faces in the 3D view. | Shown |
| **Water preview** | Water drawn like in game: refraction, water fog, reflection of the scene and the sky, moving waves. Off: its tool texture. | On |
| **Cubemap reflections** | Materials with `$envmap` reflect their cubemap (see [Lighting preview](lighting-preview.md)). | On |
| **Use the previewed sky camera in game** | See [Sky cameras](sky-cameras.md). | |
| **Show particles** / **Particles of the selection only** | See [Particles](particles.md). | On / Off |
| **Show visible clusters** | See [Leaks and pointfiles](leaks-pointfiles.md). | Off |
| **Show nav mesh** | See [Navigation mesh](nav-mesh.md). | Off |
| **Player view** (`F6`) | See [Player view](player-view.md). | |

The View menu also has **3D skybox camera** (when the map has several sky cameras) and **Hidden tool textures** (nodraw, clips, triggers, hints and skips hidden in the 3D view).

The toolbar repeats the most used switches: radius culling, displacement mesh, displacement walkability, nodraw, model collisions, helpers, models, tool textures, editor objects, 3D skybox, particles, simulate ropes, the render mode list and the player view.

## Background images

**View > Background images...** opens the **Background images** window: reference images (a floor plan, a photo) drawn behind the grid of the 2D views.

| Field | What it does |
| --- | --- |
| **Add an image...** | PNG, JPEG, BMP or TGA. |
| **View** | **Top**, **Front** or **Side**: the 2D views of that axis show it. |
| **Center** | Position of its center, in view coordinates. |
| **Units per pixel** | Scale (0.001 to 1000). |
| **Rotation** | Degrees. |
| **Opacity** | 0 to 1. |
| **Remove this image** | Removes it. |

The images are stored in the map (`bgimages_plus` block of the VMF), so they travel with it. Files are only read, never written.

## Draw distance and culling

| Option | Where | What it does | Default |
| --- | --- | --- | --- |
| **View distance** (`view.far_plane`) | Settings > Viewports | Farthest distance the 3D view draws (2048 to 262144). | 32768 |
| **Shorter draw distance** / **Longer draw distance** | **View** menu | Divide or multiply the draw distance by 1.5. | |
| **Near plane** (`render.near_plane`) | Settings > Rendering | Closest distance drawn, 0.5 to 16. Raise it if far surfaces flicker. | 1 |
| **Model distance** (`render.model_distance`) | Settings > Rendering | Models farther than this are drawn as boxes (0: no limit). | 0 |
| `render.lod_distances` | `settings.toml` | Heavy models use their LOD 1 and LOD 2 beyond these distances (0: always the full model). | 0, 0 |
| **Culling radius** (`render.cull_radius`) | Settings > Rendering | Radius of the radius culling (256 to 65536). | 4096 |
| **Skip hidden geometry** (`render.occlusion_culling`) | Settings > Rendering | Parts of the map hidden behind closer geometry are not drawn. | On |
| **Antialiasing (MSAA)** (`view.msaa`) | Settings > Viewports | Smoother edges, more GPU memory. | On |

## Frame rate

| Option | What it does | Default |
| --- | --- | --- |
| **Frame rate limit** (`render.max_fps`) | Most frames per second (0: no limit; with vertical sync the display limits it). | 0 |
| **In the background** (`render.unfocused_fps`) | Limit while the window is not active (0: no limit). | 30 |
| **Vertical sync** (`render.vsync`) | Waits for the screen refresh (applies after a restart). | On |

## Other Rendering settings

| Option | What it does | Default |
| --- | --- | --- |
| **See the sky through toolsskybox** (`sky_see_through`) | Hides the `toolsskybox` faces so the sky previews show through them. | Off |
| **Relief (normal maps)** (`normal_maps`) | Normal maps give the surfaces their relief in the 3D view. Off: flat surfaces, less video memory. | On |
| **Selection outline** (`selection_outline`) | Selected objects get an outline visible through walls. | Off |
| **Models** (2D views, `models_in_2d`) | **Boxes**, **Wireframe**, **Wireframe when selected**. | Wireframe when selected |
| **Fill selected brushes** / **Fill opacity** | Selected brushes filled in the 2D views (opacity 0.02 to 0.6). | On, 0.2 |
| **Vertex dots** | Brush vertices drawn as dots when zoomed in. | On |
| **Entity icons** / **Icon size** | Entity icons in the 2D views (8 to 48 px). | On, 16 px |
| **3D camera** | Shows the 3D camera in the 2D views. | On |
| **Visgroup colors** | Brushes drawn in the color of their visgroup. | Off |

## Tips

- On a big map, **Model distance**, **Radius culling** and a lower **View distance** keep the frame rate up (see [Editor performance](../troubleshooting/performance.md)).
- The background frame rate limit (30) saves power while you are in another program.
- MCP tools: `set_render_mode`, `screenshot_view`, `get_views`, `set_setting` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**My sky faces show the tool texture.** That is the default. Turn on **See the sky through toolsskybox** to see the sky through them.

**Where did my props go?** Check **Show models**, **Model distance** and **Show model collisions**.

**The fog preview shows nothing.** Choose a controller in **View > Display options > Fog preview**: the map needs an `env_fog_controller`.

## Related pages

- [Lighting preview](lighting-preview.md)
- [Sky cameras](sky-cameras.md)
- [Camera and navigation](camera-navigation.md)
- [Editor performance](../troubleshooting/performance.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
