"""Drive a running Hammer Pro window through the MCP stdio bridge.

Usage:
    python scripts/mcp_check.py <hammerpro-medallium.exe> <out folder> [instances folder]

Starts `<hammerpro-medallium.exe> --mcp` (the bridge an AI app starts), speaks MCP on
its stdin and stdout like a client, and calls get_map_info, create_brush,
screenshot_view (saved as <out>/mcp_view.png), undo and get_map_info again.
A Hammer Pro window must have MCP on (the pill of the top bar, or a script
running `action toggle_mcp_server`). With an instances folder, the bridge
looks there (HAMMERPRO_MCP_DIR) instead of the user's local app data.

Exit code 0 when every step worked. Only the standard library is used.
"""

import base64
import json
import os
import subprocess
import sys
import time


class Client:
    def __init__(self, exe, env):
        self.proc = subprocess.Popen(
            [exe, "--mcp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        self.next_id = 1

    def send(self, msg):
        self.proc.stdin.write((json.dumps(msg) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method, params=None):
        msg_id = self.next_id
        self.next_id += 1
        msg = {"jsonrpc": "2.0", "id": msg_id, "method": method}
        if params is not None:
            msg["params"] = params
        self.send(msg)
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError("the bridge closed stdout: " + self.proc.stderr.read().decode("utf-8", "replace"))
            answer = json.loads(line)  # stdout must only carry JSON messages
            if answer.get("id") == msg_id:
                return answer

    def call(self, name, arguments=None):
        answer = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        if "error" in answer:
            raise RuntimeError(f"{name}: protocol error {answer['error']}")
        return answer["result"]

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        return self.proc.returncode


def text_of(result):
    return "".join(c.get("text", "") for c in result["content"] if c["type"] == "text")


def main():
    exe, out = sys.argv[1], sys.argv[2]
    env = dict(os.environ)
    if len(sys.argv) > 3:
        env["HAMMERPRO_MCP_DIR"] = sys.argv[3]
    os.makedirs(out, exist_ok=True)
    client = Client(exe, env)
    report = {}

    init = client.request(
        "initialize",
        {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "mcp_check", "version": "1"}},
    )
    report["initialize"] = init["result"]["protocolVersion"], init["result"]["serverInfo"]["name"]
    client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    tools = client.request("tools/list")["result"]["tools"]
    report["tools"] = len(tools)

    # Wait for a window with MCP on.
    deadline = time.time() + 90
    while True:
        info = client.call("get_map_info")
        if not info["isError"]:
            break
        if time.time() > deadline:
            raise RuntimeError("no window: " + text_of(info))
        time.sleep(1)
    before = info["structuredContent"]
    report["map"] = before["map"]
    report["solids_before"] = before["stats"]["solids"]

    instances = client.call("list_instances")["structuredContent"]
    report["instances"] = [(i["pid"], i["map"]) for i in instances["instances"]]

    brush = client.call(
        "create_brush",
        {"min": [-128, -128, 0], "max": [128, 128, 192], "shape": "cylinder", "sides": 16, "material": "dev/dev_measuregeneric01b"},
    )
    if brush["isError"]:
        raise RuntimeError("create_brush: " + text_of(brush))
    report["create_brush"] = brush["structuredContent"]

    floor = client.call("create_brush", {"min": [-512, -512, -16], "max": [512, 512, 0], "material": "dev/dev_measuregeneric01b"})
    report["floor"] = floor["structuredContent"]["solid_ids"] if not floor["isError"] else text_of(floor)

    light = client.call(
        "create_entity",
        {"classname": "light", "origin": [0, 0, 256], "keyvalues": {"_light": "255 240 220 400"}},
    )
    report["light"] = light["structuredContent"] if not light["isError"] else text_of(light)

    camera = client.call("set_camera", {"position": [-420, -380, 300], "look_at": [0, 0, 64]})
    report["camera"] = camera["structuredContent"]["camera_3d"]

    for view in ["3d", "top"]:
        shot = client.call("screenshot_view", {"view": view, "width": 960, "height": 540})
        if shot["isError"]:
            raise RuntimeError("screenshot_view: " + text_of(shot))
        image = next(c for c in shot["content"] if c["type"] == "image")
        png = base64.b64decode(image["data"])
        if not png.startswith(b"\x89PNG"):
            raise RuntimeError("not a PNG")
        path = os.path.join(out, f"mcp_view_{view}.png")
        with open(path, "wb") as f:
            f.write(png)
        report[f"screenshot_{view}"] = (path, len(png), text_of(shot))

    problems = client.call("check_problems", {"limit": 5})["structuredContent"]
    report["problems"] = problems["total"]

    undo = client.call("undo")
    report["undo"] = undo["structuredContent"] if not undo["isError"] else text_of(undo)
    after = client.call("get_map_info")["structuredContent"]
    report["solids_after_create_and_undo"] = after["stats"]["solids"]
    report["undo_label_now"] = after["undo"]

    # Tool options and the displacement brush (view axis, intensity, live sewing).
    floor_ids = floor["structuredContent"]["solid_ids"] if not floor["isError"] else []
    if floor_ids:
        faces = client.call("get_faces", {"solid_ids": floor_ids[:1]})["structuredContent"]
        top = next(f for f in faces["solids"][0]["faces"] if f["normal"][2] > 0.9)
        face = {"solid": floor_ids[0], "side": top["side"]}
        client.call("create_displacements", {"faces": [face], "power": 3})
        opts = client.call(
            "set_tool_options",
            {"tool": "displacement", "options": {"axis": "view", "strength": 400, "radius": 96, "autosew": True}},
        )
        report["set_tool_options"] = text_of(opts) if opts["isError"] else "ok"
        stroke = client.call("stroke_displacement", {"points": [[0, 0, 0], [32, 0, 0]], "seconds_per_sample": 0.1})
        report["stroke_displacement"] = text_of(stroke) if stroke["isError"] else stroke["structuredContent"]
    block = client.call("set_tool_options", {"tool": "block", "options": {"kind": "arch", "preview": True}})
    report["block_options"] = text_of(block) if block["isError"] else "ok"
    # Compile settings: step options, the toolbar switches, the log.
    comp = client.call(
        "set_compile_options",
        {"bsp_options": {"nodetail": True}, "rad_options": {"softsun": 2}, "copy_to_maps": False},
    )
    report["set_compile_options"] = (
        text_of(comp) if comp["isError"] else (comp["structuredContent"]["extra_bsp_args"], comp["structuredContent"]["extra_rad_args"])
    )
    log = client.call("get_compile_log", {"from_file": True, "problems_only": True})
    report["compile_log_file"] = text_of(log) if log["isError"] else log["structuredContent"].get("found")
    layout = client.call("set_view_layout", {"displacements_2d": ["global", "outline", "mesh", "selected_mesh"]})
    report["displacements_2d"] = text_of(layout) if layout["isError"] else "ok"
    bake = client.call("bake_lighting", {"quality": "draft"})
    report["bake_lighting"] = text_of(bake) if bake["isError"] else bake["structuredContent"]["quality"]

    bad = client.call("create_brush", {"min": [0, 0, 0], "max": [0, 0, 0]})
    report["invalid_call_is_tool_error"] = bad["isError"]
    report["bridge_exit"] = client.close()
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
