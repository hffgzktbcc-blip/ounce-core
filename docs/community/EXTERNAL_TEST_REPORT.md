# External test report template

Copy everything inside the block below into a comment on
[issue #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) (or a new issue) and fill
it in. The steps to run are in [REPRODUCE.md](../REPRODUCE.md).

Report what actually happened. **A mismatch or a run you could not finish is a useful result.**
Please do not guess a pass. Filling in this template does not by itself make a result
"independently reproduced"; that label needs a real rerun with evidence (see
[LIMITS.md](../LIMITS.md)).

**Do not upload secrets, keys, firmware, console dumps or retail software.** Redact private
paths, hostnames and usernames from logs.

````markdown
### Outcome (tick exactly one)
- [ ] **Match**: every compared output was identical
- [ ] **Mismatch**: at least one output differed
- [ ] **Couldn't run**: I could not complete the run. Reason:

### Who and where
- Tester (GitHub handle):
- Relationship to the project: independent / contributor / maintainer
- CPU and architecture (for example Apple M2, aarch64 Linux):
- OS and version:

### What I ran
- Ounce-Core commit (`git rev-parse HEAD`):
- Rust version (`rustc --version`):
- devkitA64 version (only if you used `--nro`):
- Exact commands, in order:

```sh
# paste commands here
```

### Result
- Expected end line (from REPRODUCE.md):
- What I observed:
- First mismatch (harness, engine, first differing line), if any:
- Logs and `differential-result.json` (attach, or link; give the SHA-256 of each file):

### Honest notes
- Steps I skipped or changed:
- Anything the instructions got wrong or left unclear:
- Were the instructions clear enough to follow without help? yes / mostly / no
````
