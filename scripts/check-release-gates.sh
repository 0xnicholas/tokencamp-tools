#!/bin/sh
# Enforce the release size and startup latency gates for one packaged target.
set -eu

usage() {
    cat >&2 <<'EOF'
usage: check-release-gates.sh <tcamp-binary> <tarball>

environment:
  TCAMP_MAX_BINARY_BYTES       uncompressed binary limit (default: 15 MiB)
  TCAMP_MAX_ARCHIVE_BYTES      tarball limit (default: 6 MiB)
  TCAMP_LATENCY_SAMPLES        measured runs per command (default: 25)
  TCAMP_STARTUP_P95_LIMIT_MS   --version/--help p95 limit (default: 100)
  TCAMP_OFFLINE_P95_LIMIT_MS   status/doctor p95 limit (default: 500)
EOF
}

if [ "$#" -ne 2 ]; then
    usage
    exit 2
fi

binary=$1
archive=$2
[ -f "$binary" ] || {
    printf 'release gates: binary not found: %s\n' "$binary" >&2
    exit 1
}
[ -f "$archive" ] || {
    printf 'release gates: archive not found: %s\n' "$archive" >&2
    exit 1
}

max_binary=${TCAMP_MAX_BINARY_BYTES:-15728640}
max_archive=${TCAMP_MAX_ARCHIVE_BYTES:-6291456}
samples=${TCAMP_LATENCY_SAMPLES:-25}
startup_limit=${TCAMP_STARTUP_P95_LIMIT_MS:-100}
offline_limit=${TCAMP_OFFLINE_P95_LIMIT_MS:-500}

binary_size=$(wc -c <"$binary" | tr -d ' ')
archive_size=$(wc -c <"$archive" | tr -d ' ')

if [ "$binary_size" -gt "$max_binary" ]; then
    printf 'release gates: binary size %s bytes exceeds %s bytes\n' \
        "$binary_size" "$max_binary" >&2
    exit 1
fi
if [ "$archive_size" -gt "$max_archive" ]; then
    printf 'release gates: archive size %s bytes exceeds %s bytes\n' \
        "$archive_size" "$max_archive" >&2
    exit 1
fi

command -v python3 >/dev/null 2>&1 || {
    printf 'release gates: python3 is required for latency checks\n' >&2
    exit 1
}

python3 - "$binary" "$samples" "$startup_limit" "$offline_limit" <<'PY'
import math
import os
import shutil
import subprocess
import sys
import tempfile
import time

binary, samples, startup_limit, offline_limit = (
    sys.argv[1],
    int(sys.argv[2]),
    float(sys.argv[3]),
    float(sys.argv[4]),
)
if samples < 1:
    sys.exit("release gates: TCAMP_LATENCY_SAMPLES must be at least 1")

root = tempfile.mkdtemp(prefix="tcamp-release-gates-")
bin_dir = os.path.join(root, "bin")
os.makedirs(bin_dir, exist_ok=True)
env = os.environ.copy()
env.update(
    {
        "HOME": os.path.join(root, "home"),
        "XDG_CONFIG_HOME": os.path.join(root, "config"),
        "XDG_STATE_HOME": os.path.join(root, "state"),
        "CLAUDE_CONFIG_DIR": os.path.join(root, "claude"),
        "PI_CODING_AGENT_DIR": os.path.join(root, "pi"),
        "KIMI_CODE_HOME": os.path.join(root, "kimi"),
        "PATH": bin_dir,
    }
)
for name in ("home", "config", "state", "claude", "pi", "kimi"):
    os.makedirs(os.path.join(root, name), exist_ok=True)
for name in ("TOKENCAMP_API_KEY", "TOKENCAMP_BASE_URL", "TOKENCAMP_MODEL"):
    env.pop(name, None)


def measure(label, args, limit, expected_codes):
    subprocess.run(
        [binary, *args],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=10,
        check=False,
    )
    times = []
    for _ in range(samples):
        started = time.perf_counter()
        completed = subprocess.run(
            [binary, *args],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=10,
            check=False,
        )
        times.append((time.perf_counter() - started) * 1000.0)
        if completed.returncode not in expected_codes:
            sys.exit(
                f"release gates: {label} exited {completed.returncode}, "
                f"expected one of {sorted(expected_codes)}"
            )
    times.sort()
    p95 = times[max(0, math.ceil(0.95 * len(times)) - 1)]
    print(
        f"release gates: {label} p95={p95:.1f}ms "
        f"(limit {limit:.0f}ms, samples {samples})"
    )
    if p95 > limit:
        sys.exit(f"release gates: {label} p95 exceeds {limit:.0f}ms")


try:
    measure("--version", ["--version"], startup_limit, {0})
    measure("--help", ["--help"], startup_limit, {0})
    measure("status", ["status"], offline_limit, {0, 1})
    measure("doctor", ["doctor"], offline_limit, {0, 1})
finally:
    shutil.rmtree(root, ignore_errors=True)
PY

printf 'release gates: ok\n'
