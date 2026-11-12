#!/usr/bin/env bash
# Run from the repository root; see README.md beside this script.
set -euo pipefail
export PATH="$PWD/target/release:$PATH"
export TERM=xterm-256color

if [[ ${1:-} == --session ]]; then
    stty cols 112 rows 28
    show() { printf '\033[2J\033[H$ %s\r\n' "$1"; }
    show 'ounce-core'
    ounce-core
    sleep 5
    show 'ounce-core run-nro tests/fixtures/nro/hello/hello.nro fixture --present=headless'
    ounce-core run-nro tests/fixtures/nro/hello/hello.nro fixture --present=headless
    sleep 6
    show 'OUNCE_NRO_BUDGET=100 ounce-core run-nro tests/fixtures/nro/hello/hello.nro --present=headless'
    # A budget stop intentionally returns status 1; preserve every output byte.
    set +e
    OUNCE_NRO_BUDGET=100 ounce-core run-nro tests/fixtures/nro/hello/hello.nro --present=headless
    status=$?
    set -e
    [[ $status == 1 ]] || exit 1
    sleep 7
    exit 0
fi

[[ $(asciinema --version) == 'asciinema 2.4.0' ]]
[[ $(agg --version) == 'agg 1.6.0' ]]
cargo build --release
asciinema rec --overwrite --quiet --cols 112 --rows 28 \
    --command 'bash docs/media/record-demo.sh --session' docs/media/cli-demo.cast
agg --font-family Menlo --font-size 16 --line-height 1.25 \
    --theme github-dark --fps-cap 10 --idle-time-limit 10 \
    --last-frame-duration 8 docs/media/cli-demo.cast docs/media/cli-demo.gif
