[Documentation](../README.md) > Features > Discord Rich Presence | [Français](../../fr/features/discord.md)

# Discord Rich Presence

Like Hammer++, Hammer Pro can show what you are mapping in your Discord profile: the map, the active tool, the game and the time spent on the map.

## Opening it

**Settings > Discord** (**Tools > Options...**, `Ctrl+,`), section **Discord Rich Presence**. Settings are stored as `[discord]` in settings.toml.

## Options

| Option | What it does | Default |
| --- | --- | --- |
| **Show activity on Discord** | Turns the presence on. Turning it off clears the activity at once. Key `enabled`. | on |
| **Show the map name** | Shows *Editing gm_city.vmf*; off, *Editing a map*. Key `show_map`. | on |
| **Show the elapsed time** | Shows the time spent on the map. Key `show_elapsed`. | on |
| **Show the active tool** | Shows the tool you use. Off: the target game is shown instead (and *Compiling...* during a compile). Key `show_tool`. | on |
| **Discord application ID** | ID of the Discord application that shows the activity (section **Discord application**). Key `application_id`. | empty |
| **"Get Hammer Pro" button link** | Optional web page (`https://...`) shown as a **Get Hammer Pro** button under the activity; empty: no button. Key `project_url`. | empty |

## What Discord shows

| Line | Content |
| --- | --- |
| Details | *Editing map.vmf*, *Editing a map* (map name hidden) or *No map open*. |
| State | The active tool, or *Compiling...* during a compile, or the name of the game. |
| Time | Time since the map was opened (when **Show the elapsed time** is on). |
| Large image | The Hammer Pro logo (image key `hammerpro`), with "Hammer Pro" and the version. |
| Small image | The icon of the game of the map, with its name. |

## Setting up the application ID

Discord needs an application to show a presence. Hammer Pro 1.0.0 has no built-in application ID: until you set one, nothing is shown (*No application ID: nothing is shown on Discord yet.*).

1. On the Discord Developer Portal, create an application named "Hammer Pro".
2. Under **Rich Presence > Art Assets**, upload the Hammer Pro logo with the key `hammerpro`.
3. Optional: upload game icons with these keys: `gmod`, `hl2` (Half-Life 2, episodes, Lost Coast, SDK 2013 singleplayer), `hls` (Half-Life: Source), `hl2dm`, `tf2`, `css`, `dods`, `portal`, `portal2`, `l4d`, `l4d2`, `csgo`. Other games use their own game id as key.
4. Copy the **Application ID** (General Information page) into **Discord application ID**.

The ID is a number of 17 to 20 digits; anything else is refused with a message.

## Requirements

The Discord desktop app must run on the same computer, with **Share your detected activities** on in Discord's **Activity Privacy** settings. Hammer Pro talks to it directly through the local Discord connection, on a background thread: when Discord is not running, it retries quietly and the editor never waits for it. Automated runs (`--script`, tests) never reach your Discord.

## Tips

- Turn off **Show the map name** when you work on a secret project; the presence then just says *Editing a map*.
- Share a link to your map's Workshop page or your community with the button link.

## FAQ

**Nothing shows on Discord.** Check that an application ID is set, the Discord desktop app runs, and activity sharing is on. See [Discord presence not showing](../troubleshooting/discord-presence.md).

**Does the presence slow down the editor?** No, it runs on its own thread and updates are throttled.

**Does it work with the Discord web app?** No, the presence needs the desktop app on the same computer.

## Related pages

- [Discord presence not showing](../troubleshooting/discord-presence.md)
- [Settings window](settings-dialog.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
