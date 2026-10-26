# M3A design: GPU swapchain presentation layer

Status: **backend choice approved by the user 2026-10-26 (wgpu first). Remaining open points in section 9. No code written yet.**
Date: 2026-10-26. Base: `main` @ 629cd87 (69 unit tests, 24 stages `0xEEE`, zero `unsafe` in `src/`).

## 1. Goal and scope

Replace the CPU-only `minifb` blit with a GPU-backed swapchain on Linux, Windows and macOS, behind a Cargo feature. Keep a headless path as the default for CI and for hosts without a GPU driver.

In scope:
- Host window + swapchain, GPU texture upload of the guest framebuffer, scaled present, vsync pacing.
- Backend selection with fallback (GPU -> minifb -> headless).
- Host keyboard input decoupled from `minifb::Key`.

Out of scope (honest framing):
- Guest rasterization stays on the CPU. M3A makes *presentation* GPU-backed (upload, scale, vsync). It does not translate NVN draws into host GPU draws. That is a later milestone and should get its own design.
- The guest-facing model in `src/gpu/vulkan.rs` (`VulkanGpuBackend`, SPIR-V module checks, counters used by the stage harness) is **not touched**. It models the guest API; the new code is host presentation. Different layer, no rewrite.

Expected win: removes the per-pixel RGBA -> 0RGB conversion loop in `display.rs`, gives proper aspect/integer scaling, real vsync and resize. It is not a big MIPS win; the CPU core remains the bottleneck until M4.

## 2. Current state (read from code)

- `src/gpu/display.rs`: `DisplayServer` owns a `minifb::Window`, reads 1280x720 RGBA8 from guest VA `0x9000_0000`, converts to `u32` 0RGB, draws a 20 px HUD bar, flips a 2-buffer `Swapchain`, `update_with_buffer`.
- `src/main.rs` `run_vm_internal`: on `SvcResult::PresentFrame` calls `disp.get_active_keys()` -> `hid.poll_keyboard(&[minifb::Key])`, then `disp.present_frame(&mmu)`. The `test` harness passes `display = None` (already headless).
- `src/hid/controller.rs` imports `minifb::Key` directly. This couples input to the window crate.
- `src/gpu/vulkan.rs`: guest-side model only, no host GPU calls.

## 3. The `unsafe` problem

Project rule: `grep -rn unsafe src` must be 0. Raw Vulkan (`ash`) is `unsafe` on nearly every call. Options:

| Option | `unsafe` in `src/` | macOS | Effort | Notes |
|---|---|---|---|---|
| A. `ash` directly in `src/gpu/` | Many | Needs MoltenVK installed | High | Breaks the rule. Rejected. |
| B. `vulkano` | Some (draw commands and several builders are `unsafe` in recent releases) | Needs MoltenVK | Medium-high | Safe-ish, but not zero. Vulkan only. |
| C. `wgpu` + `winit` | **0** | **Native Metal**, no MoltenVK | Medium | Fully safe public API. Vulkan on Linux, Vulkan/DX12 on Windows, Metal on macOS, GL fallback. |
| D. New workspace crate `crates/ounce-vk` with `ash`, exposing a safe API | 0 in `src/`, lots in `crates/` | Needs MoltenVK | Highest | Meets the letter of the rule, not its spirit. We would own soundness of every Vulkan call. |
| E. `pixels` (thin layer on wgpu) | 0 | Metal | Low | Quickest, but lags wgpu releases and hides the device/queue we will want for M4 compute. |

Honest note: every option pulls `unsafe` in via dependencies (so do `minifb`, `cpal`, `gilrs` today). The rule is about code we own and must review. C keeps that at zero with no workaround.

### Decision: C, `wgpu` + `winit`, behind feature `gpu-present` (user-approved 2026-10-26)

`ounce-core` stays `#![forbid(unsafe_code)]`. Raw `ash` is only acceptable later inside a separate workspace crate (`crates/ounce-vulkan`), and only if wgpu proves insufficient.

Why:
- Zero `unsafe` in our tree, no wrapper crate to maintain.
- macOS gets Metal natively; no MoltenVK install step for users or CI.
- Gives us a real `Device`/`Queue` and compute shaders (WGSL, or SPIR-V passthrough on Vulkan) we can reuse for M4 / Stage 25 experiments.

Trade-offs accepted:
- No explicit command pools / Vulkan extensions; wgpu manages them. The roadmap item "device selection, command pool allocation" maps to wgpu adapter/device/queue selection. If a later milestone truly needs raw Vulkan (e.g. vendor extensions), revisit option D for that piece only.
- Bigger build (wgpu + naga + winit): expect a noticeably longer clean build and a few MB of binary. Feature-gated, so default builds and the CI harness job are unaffected.
- The roadmap text said "add `ash` in `gpu/vulkan.rs`". This proposal changes that: new code goes in a new module, `gpu/vulkan.rs` stays as the guest model. Needs your OK.

## 4. Architecture

```
guest RGBA8 @ 0x9000_0000
        |
  FrameCompositor (CPU, safe): read_bytes -> Frame { rgba: Vec<u8>, w, h } + HUD overlay
        |
  trait PresentBackend
   |-- HeadlessBackend   (always built; default in CI/tests)
   |-- MinifbBackend     (current behavior, moved out of display.rs)
   '-- WgpuBackend       (#[cfg(feature = "gpu-present")])
```

New files:
- `src/gpu/present/mod.rs`: `PresentBackend` trait, `Frame`, `HostKey`, `BackendKind`, selection logic.
- `src/gpu/present/headless.rs`: no window. Counts frames, keeps last frame hash (FNV-1a) for golden tests, optional PPM dump via `OUNCE_DUMP_FRAMES=<dir>`.
- `src/gpu/present/minifb.rs`: the existing window code from `display.rs`, unchanged behavior.
- `src/gpu/present/wgpu.rs`: feature-gated GPU backend.

Trait sketch:
```rust
pub trait PresentBackend {
    fn name(&self) -> &'static str;
    fn present(&mut self, frame: &Frame) -> PresentStatus; // Continue | Closed
    fn pressed_keys(&mut self) -> Vec<HostKey>;           // polls events too
    fn is_open(&self) -> bool;
    fn set_title(&mut self, title: &str);
}
```

`DisplayServer` keeps its public API (`new`, `present_frame(&mmu) -> bool`, `is_window_active`, `get_active_keys`) and holds a `Box<dyn PresentBackend>`. Callers in `main.rs` stay the same apart from the key type. HUD and FPS/title logic move into the compositor so all backends show the same thing.

Input: add `HostKey` (the ~20 keys `hid/controller.rs` actually maps). `get_active_keys` returns `Vec<HostKey>`; each backend maps its own key type. `hid` stops depending on `minifb`.

### WgpuBackend details
- Window/event loop: `winit` 0.30 with `EventLoopExtPumpEvents::pump_app_events`. This keeps today's pull-style loop (VM calls `present` per frame) instead of handing the main thread to winit. Supported on Linux (X11/Wayland), Windows, macOS. Fallback if pump proves flaky on macOS: VM on a worker thread, latest-frame-wins channel to a main-thread `run_app`.
- Device selection: `Instance::default()` (all backends), `request_adapter` with `PowerPreference::HighPerformance` and the surface; `OUNCE_ADAPTER=<substring>` override; `OUNCE_WGPU_BACKEND=vulkan|metal|dx12|gl`. Log adapter name/backend once.
- Swapchain: surface configured `PresentMode::Fifo` (vsync; `AutoNoVsync` via `--no-vsync`), format chosen from caps preferring a non-sRGB 8-bit format (guest data is already display-encoded). Reconfigure on resize; handle `Lost`/`Outdated` by reconfigure, `Timeout` by skipping a frame.
- Upload: one `Rgba8Unorm` 1280x720 texture, `queue.write_texture` per frame (no CPU swizzle). Resolution changes (docked 1080p) recreate the texture.
- Draw: fullscreen triangle WGSL shader, nearest/linear sampler toggle, letterboxed aspect-correct viewport.
- Init failure (no adapter, no display) returns `Err` so selection falls back.

### Backend selection
`--present=auto|gpu|minifb|headless` (env `OUNCE_PRESENT` same values). `auto`: gpu if compiled in and init succeeds, else minifb, else headless. `cargo run --release -- test` stays windowless as today.

## 5. Cargo

```toml
[features]
default = []
gpu-present = ["dep:wgpu", "dep:winit", "dep:pollster"]

[dependencies]
wgpu = { version = "<pin at impl time>", optional = true }
winit = { version = "0.30", optional = true }
pollster = { version = "0.4", optional = true }
```
Versions pinned exactly when implementing; check MSRV against edition 2024. `minifb` stays a default dependency (no behavior change for existing users).

## 6. Testing and CI

Unit tests (default features, run everywhere):
- Compositor: RGBA layout, HUD region, resolution change.
- Headless backend: frame count, golden FNV hash of a synthetic frame pattern.
- `HostKey` mapping table for minifb.

Feature tests (`--features gpu-present`):
- Offscreen render test: render the fullscreen-triangle pass to a texture (no surface), read back, compare to input. Skips with a printed reason if no adapter.
- Linux CI installs `mesa-vulkan-drivers` (lavapipe, software Vulkan), so this test really runs on GitHub runners without a GPU. macOS runners have Metal and should run it too. Windows: runs if WARP/DX12 adapter is found, else skips.

CI changes (`ci.yml`): keep the existing job as is; add a `gpu-present` job: `cargo build --release --features gpu-present` + `cargo test --release --features gpu-present` on the 3 OS matrix. Harness stays on default features. No stage 25 added here (reserved for HMMA).

Gates unchanged: `cargo test --release`, `cargo run --release -- test` = `0xEEE`, `grep -rn unsafe src` = 0, plus `cargo build --features gpu-present` without warnings.

Manual check on a real window: run a synthetic/homebrew framebuffer demo (`run_gfx_demo`) with `--present=gpu` on Linux and macOS.

## 7. Implementation plan (separate small PRs)

1. **Refactor, no behavior change**: add `#![forbid(unsafe_code)]` to `src/main.rs` so the compiler enforces the rule (the grep stays as a second gate); `present/` module, `PresentBackend`, `HeadlessBackend`, `MinifbBackend`, `HostKey`; `DisplayServer` delegates; `hid` drops `minifb`. Tests above.
2. **wgpu backend** behind `gpu-present`: window, adapter, surface, upload, scaled present, resize, fallback selection, `--present` flag.
3. **CI + docs**: feature job with lavapipe, README section (Linux/macOS/Windows notes), AGENTS.md log.

Each step keeps all gates green and is reviewable alone.

## 8. Risks

- winit `pump_app_events` on macOS: live-resize can stall the pump. Mitigation is the worker-thread fallback above.
- Wayland: no window decorations on some compositors without `winit` client-side decorations feature; acceptable for a research tool.
- Build time and size increase, limited to the feature build.
- lavapipe availability on `ubuntu-latest` image changes; test skips cleanly rather than failing if no adapter.

## 9. Decisions needed

1. ~~Approve option C (wgpu + winit) instead of raw `ash`.~~ Approved.
2. Approve leaving `gpu/vulkan.rs` as the guest model and putting host presentation in `gpu/present/`.
3. OK to keep `minifb` as the default window backend for now (GPU opt-in), or make `gpu-present` default later once CI is stable.
