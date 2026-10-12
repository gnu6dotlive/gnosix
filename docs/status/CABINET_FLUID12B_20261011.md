# CABINET F12-B — GPU timestamps

11 October 2026. Scoped E3 GPU timeline qualification; full F12 and E4 OPEN.

RTX 3070 supports timestamp queries. Both TIMESTAMP_QUERY and TIMESTAMP_QUERY_INSIDE_ENCODERS are required by wgpu 29. The initial 113 zero timestamp pairs were invalid and excluded. The standalone test measured nonzero hardware GPU ticks at a timestamp period of 1 ns.
## Qualified observations

Native Hyprland, RTX 3070, 100001 regular files per trial, 450 finite source-visible pulses, 80 keyboard navigation events. Query interval spans the renderer's encoded GPU work only, not compositor presentation.

| Trial | GPU query pairs | p50 GPU us | p95 GPU us | p99 GPU us |
|---|---:|---:|---:|---:|
| Exploratory Grid 1 | 116 | 182.272 | 225.280 | 226.304 |
| Exploratory List 1 | 115 | 148.480 | 193.536 | 199.680 |
| Exploratory List 2 | 112 | 137.216 | 183.296 | 229.376 |
| Exploratory Grid 2 | 118 | 152.576 | 191.488 | 195.584 |
| Exact rebuilt Grid | 116 | 205.824 | 234.496 | 257.024 |
| Exact rebuilt List | 115 | 169.984 | 198.656 | 219.136 |

Exploratory binary used temporarily modified vendored Cargo manifest and was subsequently overwritten; exploratory data are NOT exact-binary release evidence. The final rebuilt diagnostic binary was SHA256 `8ab7543c697b4b593b2683d8062499b96c03acce1415fd7a0c9be4b5761f6fd8`, archived separately before product restoration.

The exact-rebuild F11 oracle confirmed 464 Grid and 460 List Wayland feedback events, matching GPU submissions and CPU present calls in each mode. One terminal work-done callback was not observed before shutdown in each run and is NOT asserted as a hardware fault.

Six timestamp-oracle controls (zero ticks, zero period, wrong serial, inverted ticks, missing nonterminal sampled query, disabled feature) are all rejected on the exact-binary run. An earlier negative control uncovered overgenerous tolerance for missing callbacks, now corrected: only the terminal expected sample may be absent, never a missing intermediate source identity.

## Qualification boundaries

Hardware timestamps measure the GPU interval between command-encoder markers. They do not measure CPU time, waiting before the first marker, compositor composition, scanout, photons or input-to-photon latency. No 75-FPS claim follows from these results.

The 500ms historic freeze, every-buffer pixel identity and premium UX E4 remain OPEN. Full F12-B closure requires exact-HEAD CI, fixture cleanup, binary restoration and a SHA256 receipt.
