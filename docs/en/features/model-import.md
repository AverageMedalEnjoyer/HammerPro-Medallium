[Documentation](../README.md) > Features > Model import | [Français](../../fr/features/model-import.md)

# Model import

Model import turns a 3D model file (FBX, OBJ, glTF or GLB) into a Garry's Mod prop without Crowbar or Valve's compilers: Hammer Pro converts its textures to VTF and VMT, compiles the model with its built-in compiler, then makes it the model of new props and can place one in front of the camera.

## Opening it

- **File > Import a model (FBX, OBJ, glTF)...** opens a file dialog (filter "3D models": `.fbx`, `.obj`, `.gltf`, `.glb`).
- The **+** button of the [model browser](model-browser.md) does the same.
- Drag and drop a `.fbx`, `.obj`, `.gltf` or `.glb` file onto the Hammer Pro window. One model at a time: a second drop while an import runs is refused with a message.
- `--script` files have `import_model PATH`, `import_model_now PATH` (imports with the default options) and `import_model_scaled SCALE PATH` (see [Script commands](../reference/script-commands.md)).

The file is read in the background ("Reading ..."), then the **Import a model** window shows its options.

## Supported files

| Format | What is read |
|---|---|
| Wavefront OBJ (`.obj` with its `.mtl`) | Meshes and materials. Faces without normals get smooth normals per smoothing group (`s off` is flat), or across edges that are not too sharp when the file has no smoothing group. |
| glTF 2.0 (`.gltf`, `.glb`) | The meshes of the default scene with their node transforms (skinned meshes in their bind pose); the base color of the materials (texture with its coordinate set and `KHR_texture_transform`, color factor, alpha mode, double sided). Draco or meshopt compressed meshes are refused with a message. |
| FBX 7.x, binary and ASCII | Meshes placed by their models (translation, rotation and its order, pivots, offsets, scale, geometric transform, parents), materials (color, diffuse texture as a file or embedded), the axis system and the unit of the file. |

Only static geometry is imported: animations are ignored. Textures can be PNG, JPG, TGA or BMP.

## Options

The window first shows the file path and a summary: triangles, materials and size in units. Warnings appear for more than 65000 triangles (a heavy prop), and for a model smaller than 2 units or larger than 4096 units (check the scale).

| Option | What it does | Default |
|---|---|---|
| **Name** | `models/hammerpro/<name>.mdl`. Cleaned to lowercase letters, digits and `_` (48 characters at most). A warning says when a model of that name exists: it is replaced. | The file name |
| **Kind** | **prop_static**, or **Physics prop** (`prop_physics`). | prop_static |
| **Scale** | Extra scale after the unit conversion (0.001 to 1000). | 1 |
| **Convert the units** | Meters of the file become Source units (inches): 1 m = 39.37 units. Off: one unit of the file is one Source unit. | On |
| **Up axis** | **From the file**, **Y** or **Z**. Y up models are turned so that Z is up and the front stays in front. | From the file |
| **Origin** - **At the bottom center** | Puts the model origin at the center of its bottom. | On |
| **Surface property** | Physics and sound material: `default`, `concrete`, `metal`, `solidmetal`, `metal_box`, `metalpanel`, `wood`, `wood_crate`, `wood_furniture`, `plastic`, `plastic_barrel`, `glass`, `rock`, `dirt`, `grass`, `gravel`, `brick`, `tile`, `carpet`, `cardboard`, `paper`, `rubber`, `flesh`, `ice`, `porcelain`, `computer`, `pottery`. | default |
| **Collision** | **Pieces**: each separate piece of the model gets a convex hull. **Box**: the box of the model. **None**: no collision model. | Pieces |
| **Mass** (physics prop only) | 0.1 to 50000 kg. | 20 kg |
| **Levels of detail (LOD)** | Simplified versions of the model that the game draws far away. Shared with [Propper](propper.md) and saved (`[lab.model_lod]`). | On |
| **Levels** | Levels below the full model, 3 to 6 (big models get one or two more). | 4 |
| **Each keeps** | Part of the triangles of the level above that each level keeps, 20 to 90 % (small models are reduced more gently). | 50 % |
| **Place it in front of the camera** | After the import, places a prop of the model where the 3D camera looks. | On |

A physics prop without collision gets a warning: it cannot move in game.

**Materials** lists the materials the model uses with their texture: the file name, "texture in the file" (embedded), or "no texture (its color)".

The options other than the name and the LOD settings stay as you left them until you close Hammer Pro. **Import** is greyed out while the name is empty.

## What is written

The model goes into the game folder of the mounted content, its textures next to it:

- `materials/models/hammerpro/<name>/`: one VTF (with mipmaps) and one `VertexLitGeneric` VMT per used material. A material without a texture (or whose texture cannot be read) gets a small texture of its color, with a warning in the console. Double sided materials get `$nocull`; alpha becomes `$alphatest` or `$translucent` (from the material's alpha mode, or from the texture's alpha when the file does not say); a tinted textured material gets `$color2`, a see-through color `$alpha`.
- `models/hammerpro/<name>.mdl` with its `.vvd`, `.dx90.vtx` and `.phy` (no `.phy` with collision **None**). Older `.dx80.vtx`, `.sw.vtx`, `.vtx` and `.ani` files of the same name are removed. The MDL is written last, so the game never sees half a model.

The built-in compiler makes what studiomdl makes of a static prop: one bone, one idle sequence, one mesh per material, levels of detail, and a shadow LOD for physics props. A model with more than 65536 vertices is split into several body parts (the console says how many). The collision hulls are shrunk by the collision margin like studiomdl's; flat pieces are thickened to one unit.

**Build and save as content** packs these files with the map (see [Build and save as content](build-content-release.md)).

## After the import

- The content is mounted again and a toast says "*model* is ready (N textures, N collision pieces): it is the model of new props (Entity tool)". The console lists the levels of detail with their triangle counts.
- The model becomes the model of new props, with `prop_static` or `prop_physics` as class, for the [entity tool](entity-tool.md).
- With **Place it in front of the camera**, a prop is created where the 3D camera looks: resting on the surface hit (its origin is at its bottom center), else in the air in front of the camera. It becomes the selection (one undo step).

## Tips

- If the size in the summary looks 39 times too large or too small, toggle **Convert the units** or change **Scale**.
- Keep a model under 32 materials: a game model cannot have more and the import stops with a message.
- To check the result, select it in the model browser: the **Inspector** tab lists its files and materials.

## FAQ

**"The game content is not loaded": what now?** Import writes into the game folder of the mounted content. Set the game folder in the settings and wait for the content to load (see [Game setup](../getting-started/game-setup.md)).

**"The model is too large for the game".** Lower its detail in your 3D software and import it again.

**My glTF is refused.** Draco and meshopt compressed meshes are not read: export it without mesh compression.

**Can I import animations or skeletons?** No: only static geometry is read.

## Related pages

- [Model browser](model-browser.md)
- [Propper](propper.md)
- [Entity tool](entity-tool.md)
- [Build and save as content](build-content-release.md)
- [Custom texture folders](custom-texture-folders.md)
- [Script commands](../reference/script-commands.md)

---
[Back to the documentation index](../README.md)
