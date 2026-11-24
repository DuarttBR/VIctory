# rgarage: VIctory on an NVIDIA RTX 3080

This branch (`rgarage` on [DuarttBR/VIctory](https://github.com/DuarttBR/VIctory)) is our working
copy of [VIctory](https://github.com/Marvisak/VIctory). We study it alongside
[XCL](https://github.com/ivall/xcl) to get Grand Theft Auto VI's world drawing on a 32-lane NVIDIA
card. What works goes back upstream as pull requests and issue comments; what the two projects
find is brought here and tested.

Nothing from the game is in this repository (see `docs/asset-policy.md`). The game copy is our
own, kept outside the repository; caches and data live outside it too.

## The machine

| | |
|---|---|
| GPU | NVIDIA GeForce RTX 3080, 10 GB, driver 617.42 (32-lane waves only, no wave64) |
| CPU | AMD Ryzen 9 5900X, 12 cores / 24 threads |
| RAM | 32 GB (about 17 GB free with the usual desktop programs open) |
| OS | Windows 11 Pro 26H2, build 26300.9974 |
| Toolchain | Rust 1.98.1 (the repository's `rust-toolchain.toml`), DXC / `dxil.dll` 1.9.2609 for validation |
| Title | Xbox Series package 1.0.2.1, run in **Series S** mode (`--console series-s`) to fit 32 GB of RAM and 10 GB of VRAM |

## How we run it

Diagnostics build: `cargo build --profile diag -p victory --features diagnostics`.

```
VICTORY_D3D12_DRED=1 VICTORY_SHADER_LOOP_LIMIT=32 \
VICTORY_KEYS=6:Enter,10:Enter,...,110:Enter \
victory run <game exe> --execute --console series-s
```

`VICTORY_KEYS` presses Enter (A) on a schedule, so the title reaches the world without anyone at
the controller. Runs go through a small supervisor that stops the game if Windows' free RAM or
commit gets too low.

## What is merged here

`main` plus these open pull requests: #89, #160, #189, #197, #206, #211, #265, #279, #302, #333,
#346, #454, #469, #472, #483, #489, #493, #500, #530, #539, #591. Also included: the data reads of
#136, with scalar buffer loads kept raw (the full #136 blacks out the menus here), and the
memory-release part of #458.

Our own commits:

- **Root UAVs**: a compute program writing through a root view (upstream as #1077).
- **Spilled root arguments past #333's user-data check**: the check is deferred until the stage
  data shows the spill rule applies (posted on #333).
- **Textures sampled by heap index are uploaded** (`texture::upload_heap`), within a per-frame
  budget.
- **Merge fixes**: the heap guard slot of #483 on top of #336, and the test harnesses that need the
  guard entry (posted on #483).
- **Diagnostics**:
  - `RG_WORLD_VIEW=1` shows the world's depth, shaded, under the title's interface.
  - `RG_FORCE_COLOR=<n>` writes a constant to render target n of every pixel program.
  - The view heap is swept for descriptors copied without the API.
- **The window is named "rgarage".**

## Where it stands (24 Nov 2026)

- **Stability:** Series S, 5 to 7 minutes in the prologue's world with no device removal, at
  0.5 to 1.6 fps. Memory holds at about 18.5 GB.
- **Geometry is drawn:** `RG_WORLD_VIEW=1` gives the first whole frames of the world here (the
  prison yard: fences, poles, tables, people) with the interface on top.
- **Colour comes out zero:** with `RG_FORCE_COLOR=0`, the G-buffer's colour target covers 28% of
  the frame, so the material passes run and write; the colour they compute is zero. They sample
  about seven textures each through descriptors loaded from heap memory. Why those reads give
  zero is what we are working on.
- **Ground and buildings are missing:** most of them come from indirect mesh dispatches that are
  still dropped (`prepare failed`, about 730 per frame).
- **Shaders:** about 430 programs still do not translate. Most refusals are lane masks used as
  data and wave64 programs on a 32-lane card.

Reports, measurements and questions are welcome as issues on this fork or on the upstream
threads (#71 for NVIDIA).
