[Documentation](../README.md) > Features > Clipboard | [Français](../../fr/features/clipboard.md)

# Clipboard

Cut, copy and paste map objects, paste many copies at once with Paste special, duplicate the selection, and copy keyvalues from one entity to others.

## The commands

All are in the **Edit** menu and in the right click menu of the views:

| Command | Shortcut | What it does |
|---|---|---|
| **Cut** | `Ctrl+X` | Copies the selected objects to the clipboard and deletes them |
| **Copy** | `Ctrl+C` | Copies the selected objects to the clipboard |
| **Paste** | `Ctrl+V` | Pastes the clipboard contents; the pasted objects become the selection |
| **Paste special...** | `Ctrl+Shift+V` | Opens the Paste special dialog (several copies, offset, rotation) |
| **Delete** | `Delete` | Deletes the selected objects |
| **Duplicate** | `Ctrl+D` | Copies the selection one grid step away on X and Y, without using the clipboard; the copies are selected |
| **Copy keyvalues** | none | Copies the keyvalues of the selected entity |
| **Paste keyvalues** | none | Pastes them onto the selected entities |

## Where Paste puts the objects

- `Ctrl+V` with the pointer over a 2D view: the contents are centered on the cursor and keep their depth.
- `Ctrl+V` with the pointer over the 3D view: the contents sit on the surface under the cursor.
- The offset follows the grid, so objects on the grid stay on it.
- Anywhere else (menu, pointer outside the views): the objects are moved by one grid step on X and Y when **Offset pasted objects** is on (default), else pasted at their original place.

## What the clipboard holds

The clipboard is the system clipboard, as VMF text: top level `solid`, `entity` and `hidden` blocks. So:

- You can copy between two Hammer Pro windows, and between Hammer Pro and Hammer or Hammer++, both ways.
- Any VMF fragment, even a whole VMF, can be pasted. The visgroups of a whole VMF are merged by name.
- A leading `hammerpro_clipboard` block (ignored by other editors) carries the groups, so pasting keeps the grouping, even in another Hammer Pro instance.
- Pasted objects get new ids.

## Paste special

**Edit > Paste special...** (`Ctrl+Shift+V`) pastes the clipboard several times, each copy moved and rotated from the previous one (like Hammer's Paste Special):

| Option | What it does | Default |
|---|---|---|
| **Copies** | Number of copies (1 to 10000; above 1000 the dialog warns that the map gets much heavier) | 1 |
| **Offset per copy** | X, Y, Z offset added for each copy. The `<` button next to an axis takes the size of the clipboard contents on that axis (copies side by side) | 64, 0, 0 |
| **Rotation per copy** | Rotation in degrees around X, Y and Z added for each copy, around the center of the copied objects | 0, 0, 0 |
| **Name prefix** | Optional prefix added to the name of every named pasted entity | empty |
| **Start at the original position** | The first copy is placed exactly over the copied objects | Off |
| **Group each copy** | Each copy becomes a group | Off |
| **Make entity names unique** | Pasted entities whose name already exists are renamed (`name_2`, `name_3`...) | Off |
| **Rotate around the pivot** | The rotation turns around the movable pivot instead of the center of the copied objects. Only available while the pivot is shown (**Movable pivot**) | On |

- The dialog shows **Offset of the last copy** so you can check the total.
- Renamed entities keep working together: the keys of the pasted entities that name another pasted entity (targets, filters) follow the new names.
- `Enter` pastes, `Escape` closes. The values stay for the session.
- The whole paste is one undo step.

## Duplicate and clone

**Duplicate** (`Ctrl+D`) copies the selection one grid step away on X and Y. With the [Selection tool](selection-tool.md) you can also clone by holding `Shift` while dragging (move, scale, rotate, skew, gizmo, stretch) or with `Shift+arrow keys`. All these copies stay in the visgroups of their originals when **Keep visgroups when cloning** is on (page **Selection**, default on). They do not touch the clipboard.

## Copy and paste keyvalues

Select one entity and use **Edit > Copy keyvalues**, then select other entities and use **Edit > Paste keyvalues**: every key is pasted except `id`, `classname`, `origin` and `targetname`, in one undo step. Copy keyvalues also puts the keys on the system clipboard as `"key" "value"` lines, readable in a text editor. The same two commands are in the menu of the entity panel. Turn on **Paste keyvalues also pastes the name (targetname)** (page **Entities and browsers**, off by default) to paste the name too. See [Entity properties](entity-properties.md).

## Settings

| Option | Page | What it does | Default |
|---|---|---|---|
| **Offset pasted objects** | **Editing** | Paste moves the objects by one grid step so they do not overlap (when the paste does not follow the cursor) | On |
| **Keep visgroups when cloning** | **Selection** | Clones stay in the visgroups of their originals | On |
| **Paste keyvalues also pastes the name (targetname)** | **Entities and browsers** | Paste keyvalues also copies `targetname` | Off |

## Tips

- To lay out a row of identical pieces, copy one, open Paste special, click `<` on the axis of the row and set the number of copies.
- A spiral staircase: one step, **Rotation per copy** of 15 on Z and an **Offset per copy** of 8 on Z, with the pivot on the axis of the stairs.
- **File > Export selection as VMF...** saves the selection to a file instead of the clipboard (see [VMF compatibility](../getting-started/vmf-compatibility.md)).

## FAQ

**Can I paste into Hammer?** Yes. The clipboard holds plain VMF blocks that Hammer and Hammer++ read. The groups block is simply ignored there.

**Paste says the clipboard does not contain map objects.** The clipboard holds something else (text, an image). Copy objects first.

**My pasted entities lost their links.** Use **Make entity names unique** or a **Name prefix**: the keys naming other pasted entities follow the new names.

## Related pages

- [Selection tool](selection-tool.md)
- [Transform and align](transform-align.md)
- [Entity properties](entity-properties.md)
- [Groups](groups.md)
- [Visgroups](visgroups.md)
- [Undo and history](undo-history.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
