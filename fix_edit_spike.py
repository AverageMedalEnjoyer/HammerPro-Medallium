#!/usr/bin/env python3
"""
Permanently reduce post-edit CPU spikes in HammerPro-Medallium.

Patches:
  1. health.rs          – live checks / leak checks off by default
  2. render.rs          – cubemaps_auto off by default
  3. scene.rs           – no parallel on incremental rebuilds + higher threshold
  4. compile.rs         – background_precompile always off by default

Run from the repository root, then rebuild:
  cargo build --release -p hammerpro-medallium
"""

from pathlib import Path
import sys
import re

ROOT = Path(__file__).resolve().parent

def must_exist(rel: str) -> Path:
    p = ROOT / rel
    if not p.is_file():
        print(f"ERROR: expected file not found: {rel}")
        sys.exit(1)
    return p

def patch_file(path: Path, replacements: list[tuple[str, str]], label: str) -> None:
    text = path.read_text(encoding="utf-8")
    original = text
    for old, new in replacements:
        if old not in text:
            # allow already-patched
            if new in text:
                print(f"  [{label}] already applied: {old[:60]!r}...")
                continue
            print(f"  [{label}] WARNING: pattern not found, skipping:\n    {old[:80]!r}")
            continue
        text = text.replace(old, new, 1)
        print(f"  [{label}] patched")
    if text != original:
        path.write_text(text, encoding="utf-8")
        print(f"  [{label}] wrote {path.relative_to(ROOT)}")
    else:
        print(f"  [{label}] no change needed")

def main() -> None:
    print("HammerPro-Medallium edit-spike fix")
    print(f"Repo root: {ROOT}\n")

    # 1) Health defaults
    health = must_exist("crates/hp-app/src/settings/health.rs")
    patch_file(
        health,
        [
            (
                """        Self {
            live_checks: true,
            live_leak_check: true,
            show_leak_path: true,""",
                """        Self {
            live_checks: false,
            live_leak_check: false,
            show_leak_path: false,""",
            ),
        ],
        "health.rs",
    )

    # 2) Cubemap auto off
    render = must_exist("crates/hp-app/src/settings/render.rs")
    patch_file(
        render,
        [
            (
                """            cubemaps: true,
            cubemaps_auto: true,""",
                """            cubemaps: true,
            cubemaps_auto: false,""",
            ),
        ],
        "render.rs",
    )

    # 3) Scene sync – stop parallelising every incremental edit
    scene = must_exist("crates/hp-assets/src/scene.rs")
    scene_text = scene.read_text(encoding="utf-8")

    # Raise the parallel threshold so only truly huge batches use rayon
    scene_text2 = scene_text.replace(
        "const PARALLEL_SOLIDS: usize = 32;",
        "const PARALLEL_SOLIDS: usize = 4096;",
        1,
    )
    if scene_text2 == scene_text and "const PARALLEL_SOLIDS: usize = 4096;" not in scene_text:
        print("  [scene.rs] WARNING: PARALLEL_SOLIDS constant not found")
    else:
        print("  [scene.rs] PARALLEL_SOLIDS -> 4096")

    # Force incremental rebuild_solids to stay sequential
    # Original: let built = self.build_solids(doc, ids, display, true);
    old_rebuild = "let built = self.build_solids(doc, ids, display, true);"
    new_rebuild = "let built = self.build_solids(doc, ids, display, false); // sequential: avoids core-saturation spikes on every edit/undo"
    if old_rebuild in scene_text2:
        scene_text2 = scene_text2.replace(old_rebuild, new_rebuild, 1)
        print("  [scene.rs] rebuild_solids now sequential")
    elif new_rebuild in scene_text2:
        print("  [scene.rs] rebuild_solids already sequential")
    else:
        print("  [scene.rs] WARNING: rebuild_solids call site not found")

    if scene_text2 != scene_text:
        scene.write_text(scene_text2, encoding="utf-8")
        print(f"  [scene.rs] wrote {scene.relative_to(ROOT)}")
    else:
        print("  [scene.rs] no change needed")

    # 4) Background precompile off
    compile_rs = must_exist("crates/hp-app/src/settings/compile.rs")
    compile_text = compile_rs.read_text(encoding="utf-8")
    old_fn = """pub fn background_precompile_default() -> bool {
    std::thread::available_parallelism().is_ok_and(|n| n.get() >= 8)
}"""
    new_fn = """pub fn background_precompile_default() -> bool {
    // Disabled by default: on high-core CPUs the background work contributed to
    // edit-time fan spikes and brief audio lag. Users can still enable it in Settings.
    false
}"""
    if old_fn in compile_text:
        compile_text = compile_text.replace(old_fn, new_fn, 1)
        compile_rs.write_text(compile_text, encoding="utf-8")
        print("  [compile.rs] background_precompile_default -> false")
        print(f"  [compile.rs] wrote {compile_rs.relative_to(ROOT)}")
    elif "Disabled by default: on high-core CPUs" in compile_text:
        print("  [compile.rs] already patched")
    else:
        print("  [compile.rs] WARNING: background_precompile_default not found exactly")

    print("\nDone.")
    print("Rebuild with:")
    print("  cargo build --release -p hammerpro-medallium")
    print("Binary will be at: target/release/hammerpro-medallium.exe")
    print("\nOptional (existing install): also turn off in the running editor")
    print("  Settings > Map health > uncheck the three live options")
    print("  Settings > Rendering > uncheck \"Redraw the cubemaps after edits\"")
    print("  Settings > Compile > uncheck background precompile")
    print("  Settings > Rendering > Frame rate limit = 60 (helps idle heat)")

if __name__ == "__main__":
    main()