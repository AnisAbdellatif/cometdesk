# Phase 0 baseline (PLAN.md §5.1)

Measured 2026-10-28 on an Apple M5 MacBook, GL-RM1 V1.10.1, target at 2560×1440@60 over LAN,
mostly static screen. Each row is one 20 s window, taken with:

```bash
python3 tools/bench_app.py --duration 20 --group app='<process regex>' --group decoder=VTDecoderXPCService
```

CPU% = CPU time of all matching processes / wall time (100 % = one core). Energy = macOS
"Energy Impact" (`top` POWER column, the Activity Monitor number; unitless, comparable only on
the same machine). `VTDecoderXPCService` is Apple's decoder service, used by both clients, so
it is counted for whichever client was running alone.

| Client | Run | App CPU | Decoder CPU | **Total CPU** | **Total energy** |
|---|---|---|---|---|---|
| glkvm viewer (zero-copy) | 1 | 9.3 % | 4.6 % | **13.9 %** | **14.1** |
| | 2 | 8.5 % | 4.4 % | **12.9 %** | **13.1** |
| | 3 | 8.6 % | 4.5 % | **13.1 %** | **13.3** |
| Web app, Brave, Direct mode (WebCodecs) | 1 | 61.8 % | 5.4 % | **67.2 %** | **71.4** |
| | 2 | 63.8 % | 5.5 % | **69.3 %** | **73.5** |
| Web app, Brave, WebRTC mode (device setting) | 1 | 47.8 % | 5.8 % | **53.6 %** | **58.0** |
| | 2 | 48.4 % | 6.2 % | **54.6 %** | **58.9** |

Ratios (means):

- glkvm vs web Direct: **19 %** of the CPU, **19 %** of the energy impact (~5× less).
- glkvm vs web WebRTC: **24 %** of the CPU, **23 %** of the energy impact (~4× less).

## Against the targets

| ID | Target | Result |
|---|---|---|
| M2 power while viewing | ≤ 60 % of baseline | **Pass (proxy)**: 19 % of web Direct by CPU and Energy Impact. Package watts (`sudo powermetrics`) not measured yet. |
| M3 power while hidden | ≤ 50 mW over idle | Not measured in watts; the viewer stops decoding when hidden (main thread idle, decode thread parked). |
| M4 frame delivery | ≥ 59 fps, < 0.5 % dropped | **Pass**: 60.0 fps presented, 0 drops in steady state (`--bench`). |
| M5 time to first frame | ≤ 1.5 s warm, ≤ 5 s cold | **Pass**: 0.8 s warm, 4.8 s cold (device tests). |
| M6 memory | ≤ 200 MB | **Pass**: 120–145 MB RSS. |
| M1 glass-to-glass latency | ≤ baseline p50 + 5 ms | **Not measured**: needs a 240 fps camera filming a ms clock on the target next to both clients. Client-side, a frame is on screen 1.4–2.0 ms after it arrives. |
| M7 remote | — | Phase 3. |

**Go/no-go: provisional go.** The power gate passes by a wide margin on CPU and Energy
Impact; M1 still needs the camera measurement before Phase 0 is formally closed.

## Caveats

- Baseline browser is **Brave** (Chromium, same WebCodecs hardware decode path as Chrome);
  Chrome isn't installed on this machine.
- The device remembers the web app's video mode. It was switched to `direct_h264` for the
  Direct runs and back to `webrtc` (the original setting) afterwards; verified via
  `system/get_config`.
- `WindowServer` was excluded: it sat at ~44 % CPU even with no client running (other screen
  activity), which drowns the difference between clients.
- Window sizes were similar (~1280×720 logical), both visible and unobstructed; Brave had a
  single tab.
- To get watts:
  `sudo powermetrics --samplers cpu_power,gpu_power -i 1000 -n 20` while each client runs.
