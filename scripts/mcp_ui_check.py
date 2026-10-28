"""Drive the generic MCP tools of a running Hammer Pro through the stdio bridge.

Usage:
    python scripts/mcp_ui_check.py <hammerpro.exe> <out folder> [instances folder]

Like scripts/mcp_check.py (same client), for the generic tools: help
(get_capabilities), every action (list_actions, run_action), settings,
views and layout, describe_ui, UI automation (ui_click, ui_key, ui_type,
ui_scroll, ui_drag, wait_frames) and window screenshots, saved as
<out>/mcp_window_*.png. A Hammer Pro window must have MCP on (the pill of the
top bar, or a script running `action toggle_mcp_server`) with a map open.

Prints a JSON report; exit code 0 when every step worked. Only the standard
library is used.
"""

import base64
import json
import os
import sys
import time

from mcp_check import Client, text_of

REPORT = {}


def ok(client, report, name, arguments=None, key=None):
    """Call a tool that must succeed; store its JSON answer in the report."""
    result = client.call(name, arguments)
    if result["isError"]:
        raise RuntimeError(f"{name} {arguments}: {text_of(result)}")
    value = result.get("structuredContent")
    report[key or name] = value
    return value


def fails(client, report, name, arguments, needle):
    """Call a tool that must answer a tool error containing `needle`."""
    result = client.call(name, arguments)
    text = text_of(result)
    if not result["isError"] or needle not in text:
        raise RuntimeError(f"{name} {arguments} should fail with '{needle}': {text}")
    report.setdefault("refused", []).append(f"{name}: {text}")


def save_png(result, path):
    image = next(c for c in result["content"] if c["type"] == "image")
    png = base64.b64decode(image["data"])
    if not png.startswith(b"\x89PNG"):
        raise RuntimeError("not a PNG")
    with open(path, "wb") as f:
        f.write(png)
    return len(png)


def main():
    exe, out = sys.argv[1], sys.argv[2]
    env = dict(os.environ)
    if len(sys.argv) > 3:
        env["HAMMERPRO_MCP_DIR"] = sys.argv[3]
    os.makedirs(out, exist_ok=True)
    client = Client(exe, env)
    report = REPORT
    init = client.request(
        "initialize",
        {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "mcp_ui_check", "version": "1"}},
    )
    report["protocol"] = init["result"]["protocolVersion"]
    client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    tools = client.request("tools/list")["result"]["tools"]
    report["tools_listed"] = len(tools)

    deadline = time.time() + 90
    while True:
        info = client.call("get_map_info")
        if not info["isError"]:
            break
        if time.time() > deadline:
            raise RuntimeError("no window: " + text_of(info))
        time.sleep(1)
    report["map"] = info["structuredContent"]["map"]

    caps = ok(client, report, "get_capabilities", {}, "capabilities")
    report["capabilities"] = {c["category"]: len(c["tools"]) for c in caps["categories"]}
    one = ok(client, report, "get_capabilities", {"tool": "ui_drag"}, "help_ui_drag")
    report["help_ui_drag"] = one["example"]

    actions = ok(client, report, "list_actions", {"limit": 1000}, "actions")
    report["actions"] = {
        "total": actions["total"],
        "enabled": sum(1 for a in actions["actions"] if a["enabled"]),
        "need_confirm": [a["id"] for a in actions["actions"] if "needs_confirm" in a],
    }
    ok(client, report, "run_action", {"action": "render_wireframe"}, "run_render_wireframe")
    ok(client, report, "run_action", {"action": "select_all"}, "run_select_all")
    fails(client, report, "run_action", {"action": "exit"}, "confirm")
    fails(client, report, "run_action", {"action": "launch_game", "confirm": True}, "automated runs")

    views = ok(client, report, "get_views", {}, "views")
    report["views"] = [(v["index"], v["kind"], v["rect"]) for v in views["views"]]
    ok(client, report, "set_render_mode", {"mode": "textured"})
    ok(client, report, "zoom_2d", {"view": "top", "factor": 0.5})
    ok(client, report, "pan_2d", {"view": "all", "center": [0, 0, 0]})

    fov = ok(client, report, "get_settings", {"path": "view.fov_deg"}, "fov")["value"]
    ok(client, report, "set_setting", {"path": "view.fov_deg", "value": 100})
    fails(client, report, "set_setting", {"path": "compile.vbsp_path", "value": "C:/x.exe"}, "read only")
    fails(client, report, "set_setting", {"path": "view.fov_deg", "value": 500}, "from 40 to 120")
    ok(client, report, "set_setting", {"path": "view.fov_deg", "value": fov}, "fov_restored")
    ok(client, report, "list_keybindings", {"query": "save"}, "keys_save")

    # Look at the window, then drive it.
    shot = client.call("screenshot_window", {"max_size": 1600})
    if shot["isError"]:
        raise RuntimeError("screenshot_window: " + text_of(shot))
    report["screenshot_full"] = (save_png(shot, os.path.join(out, "mcp_window_full.png")), text_of(shot))
    ui = ok(client, report, "describe_ui", {"console_lines": 5}, "describe")
    report["describe"] = {
        "window": ui["window"],
        "views": [(v["index"], v["kind"], v["rect"]) for v in ui["views"]],
        "panes": [(p["title"], len(p["texts"]), len(p["controls"])) for p in ui["panes"]],
        "windows": [w["title"] for w in ui["windows"]],
        "status_bar": ui["status_bar"][:8],
        "focused": ui["focused"],
        "tree": ui["accessibility_tree"],
    }

    # Clear the selection by clicking empty space in the top view, then
    # select everything with the shortcut.
    ok(client, report, "ui_click", {"view": "top", "fx": 0.05, "fy": 0.05}, "click_top")
    ok(client, report, "ui_key", {"key": "Ctrl+A"}, "key_select_all")
    ok(client, report, "ui_scroll", {"view": "top", "dy": 1}, "scroll_top")
    ok(client, report, "ui_drag", {"view": "3d", "from": {"fx": 0.5, "fy": 0.5}, "to": {"fx": 0.55, "fy": 0.5}, "button": "right"}, "drag_3d")
    ok(client, report, "wait_frames", {"frames": 5}, "wait")

    # Open the settings with its shortcut-less action, read the dialog, close it with Escape.
    ok(client, report, "run_action", {"action": "settings"}, "open_settings")
    ok(client, report, "wait_frames", {"frames": 3})
    ui = ok(client, report, "describe_ui", {}, "describe_settings")
    report["settings_dialog"] = [(w["title"], w["texts"][:6], [c["text"] for c in w["controls"]][:6]) for w in ui["windows"]]
    shot = client.call("screenshot_window", {"max_size": 1600})
    report["screenshot_settings"] = save_png(shot, os.path.join(out, "mcp_window_settings.png"))
    # Close it with its Close button, at the point describe_ui gives.
    settings = next(w for w in ui["windows"] if "Settings" in w["title"])
    close = next(c for c in settings["controls"] if c["text"] == "Close" and c["at"])
    ok(client, report, "ui_click", {"x": close["at"][0], "y": close["at"][1]}, "click_close")
    ui = ok(client, report, "describe_ui", {}, "describe_after_close")
    report["dialogs_after_close"] = ui["dialogs_open"]
    if "settings" in ui["dialogs_open"]:
        raise RuntimeError("the Close button did not close the settings")

    # Open the File menu with a click, read it, close it with Escape.
    bar = next(p for p in ui["panes"] if p["title"] == "menu bar")
    file_menu = next(c for c in bar["controls"] if c["text"] == "File")
    ok(client, report, "ui_click", {"x": file_menu["at"][0], "y": file_menu["at"][1]}, "click_file_menu")
    ui = ok(client, report, "describe_ui", {}, "describe_menu")
    report["open_menus"] = [[c["text"] for c in m["controls"]][:12] for m in ui["menus"]]
    shot = client.call("screenshot_window", {"max_size": 1600})
    report["screenshot_menu"] = save_png(shot, os.path.join(out, "mcp_window_menu.png"))
    ok(client, report, "ui_key", {"key": "Escape"}, "escape_menu")
    ui = ok(client, report, "describe_ui", {}, "describe_after_escape")
    report["menus_after_escape"] = len(ui["menus"])
    report["status_bar"] = ui["status_bar"]
    report["panes"] = [(p["title"], p["rect"]) for p in ui["panes"]]

    # A region of the window, downscaled.
    region = views["views"][1]["rect"] or [0, 0, 400, 300]
    shot = client.call("screenshot_window", {"region": region, "max_size": 512})
    report["screenshot_region"] = (save_png(shot, os.path.join(out, "mcp_window_region.png")), text_of(shot))

    fails(client, report, "ui_click", {"x": 100000, "y": 5}, "outside the window")
    fails(client, report, "ui_type", {"text": "a\nb"}, "control characters")
    for name in [t["name"] for t in tools]:
        result = client.call(name, {"zzz_unknown": 1})
        if not result["isError"]:
            raise RuntimeError(f"{name} accepted an unknown argument")
    report["every_tool_refuses_unknown_arguments"] = True
    report["bridge_exit"] = client.close()
    print(json.dumps(report, indent=1))


def run():
    try:
        main()
    except Exception:
        # The report so far helps to see where it stopped.
        print(json.dumps(REPORT, indent=1, default=str))
        raise


if __name__ == "__main__":
    run()
