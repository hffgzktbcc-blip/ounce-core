# Developer issue evidence checked on 2026-11-07

Read-only review of public issues and their comments; no maintainers contacted.
These are candidate problems, not resolved issues, feedback on Ounce-Core, or
evidence of demand for the proposed toolkit.

| Report / affected developer | Existing workaround or smallest input | Fit and evidence gap |
| --- | --- | --- |
| [switch-examples #83](https://github.com/switchbrew/switch-examples/issues/83), TheLat: unspecified SDL2 build dependencies | Reporter suggests `pacman -S switch-sdl2 switch-sdl2_mixer`; comments add further packages. Input is the affected example build, but no exact revision or original build log is supplied. | Dependency troubleshooting could help onboarding. An old open ticket does not prove a failure in today's pinned image. Need a failing build log and exact package/revision context before changing preparation tooling. |
| [switch-examples #69](https://github.com/switchbrew/switch-examples/issues/69), T4cC0re: intermittent thread creation freeze | [Reporter-provided source and instructions](https://github.com/T4cC0re/libnx-threads-freeze), including a Linux comparison target. Maintainer suggested making the busy-loop variable volatile and increasing its iteration count; reporter subsequently updated the example and still reported a target-specific freeze. | Strongest concrete reproduction candidate. It is intermittent and no matched Ounce-Core canonical evidence/NRO is retained. GitHub metadata reports no repository license; inspect actual notices before any source/binary redistribution. Public accessibility alone does not clear redistribution. Local investigation could remain separate from a shipped artifact. |
| [nx-hbmenu #186](https://github.com/switchbrew/nx-hbmenu/issues/186), diekleinekuh: old-ABI warning overlooked, leading to crashes | Reporter describes rebuilding for the new ABI as the remedy and proposes a clearer warning. No specific libretro-core input or matched old/new binaries are attached. | Toolchain identity matters to diagnosis, but the requested hbmenu warning behavior is not demonstrated by the current retained first-frame adapter. Need a minimal licensed input and observable fixed expectation; do not generalize an ABI checker speculatively. |
| [ounce-core #202](https://github.com/hffgzktbcc-blip/ounce-core/issues/202), project owner: duplicate step IDs corrupt checkpoint placement | Report identifies the old scripts and duplicate step 15 blocks. W1 now rejects duplicate IDs and stores staged-only checkpoints in Git commits without editing the plan. | Direct project workflow issue covered by PR #205 and its disposable-Git regression checks. This is internal contributor evidence, not an external runtime user result, and does not separately close R2. |

The hypothesis is narrower after this check: the current report-pair diagnosis
works for retained canonical corpus evidence, while these external reports mostly
need input capture/build context first. A small supported failure with exact
source/binary identity and an unchanged observable expectation should select the
next runtime component change. Do not infer a generalized debugger, new platform
adapter, or community adoption from this shortlist. R2 remains open.

Follow-up on 2026-11-07: inspected the threading reproducer at exact commit
`67e1684ecce7f46082c3b91bbef02647a43a59e5`. Its five tracked files contain no
license file; the README attributes the pool implementation to a video. The
updated case uses billion-iteration worker loops and repeats pool creation up to
3,602 times; its recorded environment was devkitA64 r13-4 and firmware 8.1.0.
No source or executable was copied into this repository. Reading this case does
not reproduce the freeze on the current runtime or justify a speculative fix.
R2 remains open; R3 proceeded from R1's demonstrated workflow in the meantime.

Selected case (2026-11-08): [libnx #685](https://github.com/switchbrew/libnx/issues/685),
ITotalJustice's default-channel-padding report, supplies a small source reproducer
that builds unchanged with the pinned toolchain. [The registered case and executed
dossier](AUDIO_CHANNEL_CASE.md) now provide the matched local runtime failure and
repair for R2. No upstream source or binary is redistributed and no maintainer
was contacted. The earlier threading case failed to build unchanged against the
current API; the current pinned image already has the SDL2 dependencies named in
#83. Neither of those observations was counted as a resolved runtime issue.
