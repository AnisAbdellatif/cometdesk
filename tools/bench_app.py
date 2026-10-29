#!/usr/bin/env python3
"""Measure CPU time and macOS energy impact of an app's processes over a fixed window.

Works for any app without sudo, so the web app in a browser and the cometdesk viewer can be
compared the same way. Each --group is a regex matched against full process command lines;
all matching processes (e.g. a browser's GPU, renderer and main processes) are summed.

    python3 tools/bench_app.py --group viewer='cometdesk-cli view' \\
        --group decoder=VTDecoderXPCService --group windowserver=WindowServer --duration 20

CPU% is total CPU time / wall time (100% = one core). Energy impact is the `top` POWER column
averaged per sample (macOS's unitless "Energy Impact", the same number Activity Monitor
shows). Processes that start or exit during the window are counted for the part they ran.
"""

import argparse
import os
import json
import re
import subprocess
import sys
import time


def cpu_times():
    """pid -> (cpu seconds, command line) for every process."""
    out = subprocess.run(["/bin/ps", "-A", "-o", "pid=,time=,command="],
                         capture_output=True, text=True, check=True).stdout
    procs = {}
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) < 3:
            continue
        pid, t, cmd = parts
        procs[int(pid)] = (parse_cputime(t), cmd)
    return procs


def parse_cputime(t):
    """`[[dd-]hh:]mm:ss.ss` -> seconds."""
    days = 0
    if "-" in t:
        d, t = t.split("-", 1)
        days = int(d)
    secs = 0.0
    for part in t.split(":"):
        secs = secs * 60 + float(part)
    return days * 86400 + secs


def top_power(duration):
    """pid -> mean energy impact over `duration` one-second samples."""
    out = subprocess.run(
        ["/usr/bin/top", "-l", str(duration + 1), "-s", "1", "-stats", "pid,power"],
        capture_output=True, text=True, check=True).stdout
    samples = out.split("PID")[2:]  # the first sample has no meaningful deltas
    totals = {}
    for sample in samples:
        for line in sample.splitlines()[1:]:
            parts = line.split()
            if len(parts) == 2 and parts[0].isdigit():
                try:
                    totals[int(parts[0])] = totals.get(int(parts[0]), 0.0) + float(parts[1])
                except ValueError:
                    pass
    n = max(len(samples), 1)
    return {pid: v / n for pid, v in totals.items()}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--group", action="append", required=True, metavar="NAME=REGEX")
    ap.add_argument("--duration", type=int, default=20)
    ap.add_argument("--label", default="")
    args = ap.parse_args()

    groups = []
    for g in args.group:
        name, _, rx = g.partition("=")
        groups.append((name, re.compile(rx)))
    me = {os.getpid()}

    before = cpu_times()
    t0 = time.monotonic()
    power = top_power(args.duration)  # blocks for the duration
    wall = time.monotonic() - t0
    after = cpu_times()

    result = {"label": args.label, "seconds": round(wall, 1), "groups": {}}
    for name, rx in groups:
        # Skip ourselves and the shell that launched us: its command line contains the regex.
        def wanted(pid, cmd):
            return rx.search(cmd) and pid not in me and "bench_app.py" not in cmd

        pids = {pid for pid, (_, cmd) in after.items() if wanted(pid, cmd)}
        pids |= {pid for pid, (_, cmd) in before.items() if wanted(pid, cmd)}
        cpu = sum(after.get(p, before.get(p))[0] - before.get(p, (0.0, ""))[0] for p in pids)
        result["groups"][name] = {
            "processes": len(pids),
            "cpu_percent": round(cpu / wall * 100, 1),
            "energy_impact": round(sum(power.get(p, 0.0) for p in pids), 1),
        }

    for name, g in result["groups"].items():
        print(f"{args.label:<14} {name:<14} cpu {g['cpu_percent']:6.1f}%  "
              f"energy {g['energy_impact']:6.1f}  ({g['processes']} processes)")
    print(json.dumps(result), file=sys.stderr)


if __name__ == "__main__":
    main()
