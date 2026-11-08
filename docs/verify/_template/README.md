# Differential test template

Copy this folder to `docs/verify/<program>-harness/`, add the program's pinned source (do not commit it; record the commit or version), and follow [docs/ADD_A_PROGRAM.md](../../ADD_A_PROGRAM.md).

Files: `cases.c` (the test, shared by host and guest), `host/host_main.c` (host reference runner), `main_nx.c` (Switch entry point).
