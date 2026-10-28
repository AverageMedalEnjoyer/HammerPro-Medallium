#!/usr/bin/env bash
# Startup and map open timing benchmark (package 13).
#
# Writes a synthetic map (100,000 brushes and 10,000 entities by default) and
# opens it with `hammerpro --bench-open`, which prints one line of key=value
# timings to stdout: startup phases (startup_window_ms, startup_app_ms,
# startup_first_frame_ms...), open phases (open_read_ms, open_scan_ms,
# open_build_ms, open_scene_ms, open_install_ms, open_first_draw_ms), the scene
# upload of the first frame, the longest frame while opening (a frozen window
# shows there) and the time until every texture and model settled.
#
# Usage: scripts/bench_open.sh [map.vmf] [solids] [entities]
# Targets: window under 0.5 s, the 100k map drawn under 1 s, no frame over
# 100 ms while it opens.
set -euo pipefail

map=${1:-target/bench/big100k.vmf}
solids=${2:-100000}
entities=${3:-10000}

mkdir -p "$(dirname "$map")"
cargo run --release -p hp-vmf --example big_map -- "$map" "$solids" "$entities"
cargo build --release -p hammerpro-medallium
target/release/hammerpro-medallium --bench-open "$map"
