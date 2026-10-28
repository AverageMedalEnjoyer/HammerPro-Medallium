[Documentation](../README.md) > Troubleshooting > Discord presence not showing | [Français](../../fr/troubleshooting/discord-presence.md)

# Discord presence not showing

Hammer Pro can show what you are mapping in your Discord profile (the map, the active tool, the game and the time spent on the map), like Hammer++. When nothing appears, go through this checklist. The feature itself is described in [Discord Rich Presence](../features/discord.md).

## Checklist

1. **An application ID is set.** Hammer Pro has no built in Discord application: until you paste the ID of one, nothing is shown, and the **Discord** settings page says "No application ID: nothing is shown on Discord yet." See below to create one.
2. **The ID is valid.** It is a number of 17 to 20 digits, from the **General Information** page of the application. Anything else is refused ("This is not a Discord application ID").
3. **Show activity on Discord is on** (**Settings > Discord**, on by default). Turning it off clears the activity at once.
4. **The Discord desktop app runs on this computer.** Discord in a web browser cannot receive the presence: Hammer Pro talks to the local Discord client through its IPC pipes (`\\.\pipe\discord-ipc-0` to `discord-ipc-9` on Windows).
5. **Activity sharing is allowed in Discord.** In Discord's settings, **Activity Privacy**, turn on **Share your detected activities**.
6. **Wait a little.** Updates are sent at most every 15 seconds. When Discord is not running, Hammer Pro retries quietly, first after 5 seconds, then less often (up to every 60 seconds): starting Discord after Hammer Pro works, it just takes up to a minute.

Automated runs (`--script`, tests) never talk to Discord.

## Creating the Discord application

1. Open the Discord Developer Portal and create an application named "Hammer Pro" (the name is what your profile shows).
2. Under **Rich Presence > Art Assets**, upload the Hammer Pro logo with the key `hammerpro` (without it, the activity shows no picture).
3. On the **General Information** page, copy the **Application ID**.
4. In Hammer Pro, **Tools > Options...** (`Ctrl+,`), page **Discord**, paste it in **Discord application ID**.

## The Discord settings page

| Option | What it does | Default |
|---|---|---|
| **Show activity on Discord** | Turns the presence on or off. | On |
| **Discord application ID** | The ID of your Discord application. | Empty |
| **Show the map name** | Off: "Editing a map" instead of the map name. | On |
| **Show the active tool** | Off: the target game is shown instead (and "Compiling..." during a compile). | On |
| **Show the elapsed time** | The time spent on the map. | On |
| **"Get Hammer Pro" button link** | Optional web page (`https://...`) shown as a button under the activity; empty: no button. | Empty |

These are stored in the `[discord]` table of `settings.toml` (`enabled`, `application_id`, `show_map`, `show_tool`, `show_elapsed`, `project_url`).

## FAQ

**The activity shows but without the logo.** The art asset key must be exactly `hammerpro` in the Rich Presence art assets of your application. Discord can take a few minutes to make new assets available.

**Other people do not see the map name.** Check **Show the map name**. With it off, Discord shows "Editing a map".

**It worked, then stopped after restarting Discord.** Hammer Pro reconnects on its own within a minute; nothing to do.

## Related pages

- [Discord Rich Presence](../features/discord.md)
- [Settings window](../features/settings-dialog.md)
- [Settings reference](../reference/settings.md)
- [FAQ](faq.md)

---
[Back to the documentation index](../README.md)
