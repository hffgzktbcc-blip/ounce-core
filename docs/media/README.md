# README terminal demo

`cli-demo.gif` records the actual CLI at commit `e1b9bd7`, using only the
committed `tests/fixtures/nro/hello/hello.nro` homebrew fixture. The first run
passes `fixture` as its argument and exits normally; the second deliberately
limits execution to 100 instructions to show the real diagnosis and next step.
No program output is substituted, filtered, or retouched. The script clears the
terminal between commands and adds reading pauses. Timings and memory figures
will vary by host. The raw `cli-demo.cast` preserves the recorded output.

## Reproduce

Run from the repository root on a machine with Rust, Python 3, and Menlo (the
committed recording uses macOS). Install asciinema **2.4.0** in a virtualenv and
use the pinned agg **1.6.0** release binary:

```sh
python3 -m venv /tmp/ounce-demo-venv
/tmp/ounce-demo-venv/bin/pip install asciinema==2.4.0
mkdir -p /tmp/ounce-demo-bin
curl -fL https://github.com/asciinema/agg/releases/download/v1.6.0/agg-aarch64-apple-darwin \
  -o /tmp/ounce-demo-bin/agg
printf '%s  %s\n' \
  3171a76d924a582457552cfccdfb4f35f58d8080c96092605f0db56d77bde830 \
  /tmp/ounce-demo-bin/agg | shasum -a 256 -c -
chmod +x /tmp/ounce-demo-bin/agg
PATH="/tmp/ounce-demo-venv/bin:/tmp/ounce-demo-bin:$PATH" bash docs/media/record-demo.sh
```

For other hosts, select the matching binary from the same
[agg release](https://github.com/asciinema/agg/releases/tag/v1.6.0) and install
Menlo or choose an available monospace font in the script. The recording is
112 columns by 28 rows, uses a neutral GitHub Dark theme, and lasts about 20
seconds. `asciinema play docs/media/cli-demo.cast` replays the source recording.
