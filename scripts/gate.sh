#!/usr/bin/env bash
# One gate, three modes (see PLAN.md "How we work"):
#   scripts/gate.sh                     local tests, failures only, no commit
#   scripts/gate.sh --checkpoint "<what/why>" [--next "<action>"] [--tried "<approach — why not>"] [--decision "<d — evidence>"]
#                                       commit + push work in progress, red or green, and update the step's checkpoint
#   scripts/gate.sh --ship [--step N] [--proof "<text>"] [--no-oracle "<why>"]
#                                       green tests + oracle evidence, tick with proof, PR, GitHub auto-merge
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1
. scripts/next.sh

die() { echo "gate: $*" >&2; exit 1; }
mode=test what= nxt= tried= decision= proof= no_oracle=
while [ $# -gt 0 ]; do
  case $1 in
    --checkpoint) mode=checkpoint; what=${2:-}; shift ;;
    --ship) mode=ship ;;
    --next) nxt=$2; shift ;;
    --tried) tried=$2; shift ;;
    --decision) decision=$2; shift ;;
    --step) STEP=$2; shift ;;
    --proof) proof=$2; shift ;;
    --no-oracle) no_oracle=$2; shift ;;
    *) die "unknown argument $1 (see the header of scripts/gate.sh)" ;;
  esac
  shift
done
[ "$mode" != checkpoint ] || [ -n "$what" ] || die '--checkpoint needs "<what/why>"'

branch=$(git branch --show-current)
[ -n "$branch" ] && [ "$branch" != main ] || die "refusing to work on ${branch:-a detached HEAD}; make a step branch first"

# Never stage keys, retail containers or dumps (even ignored ones lying in the tree), or build output.
guard_files() {
  local bad
  bad=$( { git ls-files --others --ignored --exclude-standard --directory; git ls-files --others --exclude-standard
           git diff --name-only HEAD; } | sort -u |
         grep -iE '\.(keys|nsp|xci|nca|nsz|xcz|tik)$|(^|/)(dumps|ns2jb[^/]*)/|(^|/)ns2jb|(^|/)(target/.+|[^/]+\.(o|rlib|dylib|so|dll))$' )
  [ -z "$bad" ] || { echo "gate: refusing: key, retail container, dump or build-output files in the tree:" >&2; echo "$bad" >&2; exit 1; }
}
checks() { scripts/check_legal_guardrails.sh && scripts/check_unsafe_boundary.sh || { echo "gate: guardrail check failed" >&2; return 1; }; }

# Tests: logs in target/gate, failures only on screen. Sets $status (RED|GREEN) and $summary.
run_tests() {
  mkdir -p target/gate
  echo "gate: cargo test --release ..."
  cargo test --release > target/gate/test.log 2>&1; local t=$?
  echo "gate: cargo run --release -- test ..."
  cargo run --release -- test > target/gate/suite.log 2>&1; local s=$?
  local unit
  unit=$(awk '/^test result:/ { p += $4; f += $6 } END { printf "%d passed, %d failed", p, f }' target/gate/test.log)
  summary="cargo test: $unit$([ $t -eq 0 ] || echo " (exit $t)"); suite: $([ $s -eq 0 ] && echo pass || echo "FAIL (exit $s)")"
  if [ $t -eq 0 ] && [ $s -eq 0 ]; then status=GREEN; else
    status=RED
    grep -E '^test .* FAILED$|panicked at|^error(\[E[0-9]+\])?:' target/gate/test.log | head -30
    [ $s -eq 0 ] || grep -iE 'fail|error|panicked' target/gate/suite.log | head -20
    echo "gate: full logs in target/gate/test.log and target/gate/suite.log"
  fi
  echo "gate: $status — $summary"
}

# Tracked changes anywhere, new files only under src/ crates/ tests/ docs/ scripts/; then the guardrails.
stage() {
  git add -u && git add -A -- src crates tests docs scripts || die "staging failed"
  checks || { git reset -q; exit 1; }
}

case $mode in
  test) guard_files; checks || exit 1; run_tests; [ "$status" = GREEN ] ;;

  checkpoint)
    guard_files; checks || exit 1; run_tests
    n=$(current_step)
    write_checkpoint "$n" "$status — tests: $summary — $what" "$nxt" "$tried" "$decision"
    stage
    git commit -q -m "wip(step$n): $status — $what" -m "$summary" || die "commit failed"
    out=$(git push -q -u origin HEAD 2>&1) || die "push failed, the commit is local only ($(git log -1 --format=%h)): $out"
    echo "gate: checkpoint $(git log -1 --format=%h) pushed to $branch (step $n, $status)" ;;

  ship)
    guard_files; checks || exit 1; run_tests
    [ "$status" = GREEN ] || die "ship needs green local tests"
    n=${STEP:-$(branch_step)}
    fail() {  # $1: what failed. Puts the error into the checkpoint (restoring it if the tick was committed).
      echo "gate: $1" >&2
      if [ -n "$n" ]; then
        [ -n "${ticked:-}" ] && git revert --no-commit HEAD
        write_checkpoint "$n" "RED — ship failed: $1"
        git commit -q -m "wip(step$n): RED — ship failed" -- "$PLAN" && git push -q -u origin HEAD
      fi
      exit 1
    }
    count=
    if [ -n "$n" ] && ! is_ticked "$n"; then
      if [ -n "$no_oracle" ]; then count="no oracle change: $no_oracle"; else
        rep=$(latest_report "$branch" only) || true
        [ -n "$rep" ] || die "no canonical report from external-corpus on $branch yet; push a checkpoint and let it run (or --no-oracle \"<why>\")"
        set -- $rep
        python3 scripts/corpus_verify.py headline "$1" --check-readme || die "README count does not match the canonical report ($2)"
        count="$(report_summary "$1") ([canonical report]($2))"
      fi
    fi
    if [ -n "$(git status --porcelain --untracked-files=no)$(git ls-files --others --exclude-standard -- src crates tests docs scripts)" ]; then
      stage; git commit -q -m "wip(step${n:-}): GREEN — ready to ship" -m "$summary" || die "commit failed"
    fi
    out=$(git push -u origin HEAD 2>&1) || fail "git push: $out"
    pr=$(gh pr view --json number -q .number 2>/dev/null)
    if [ -z "$pr" ]; then
      title=$( [ -n "$n" ] && step_block "$n" | head -1 | sed -E 's/^- \[.\] \*\*([^*]*)\*\*.*/PLAN step \1/' || git log -1 --format=%s)
      out=$(gh pr create --base main --title "$title" --body "$(printf 'Shipped with scripts/gate.sh --ship.\n\n%s' "$summary")" 2>&1) ||
        fail "gh pr create: $out"
      pr=$(gh pr view --json number -q .number)
    fi
    url=$(gh pr view "$pr" --json url -q .url)
    if [ -n "$n" ] && ! is_ticked "$n"; then
      rewrite_block "$n" "      *Proof:* [PR #$pr]($url): $count.${proof:+ $proof}" 1
      git commit -q -m "ship(step$n): tick with proof" -- "$PLAN" || die "tick commit failed"
      ticked=1
      out=$(git push -q 2>&1) || fail "git push (tick): $out"
    fi
    out=$(gh pr merge "$pr" --auto --squash 2>&1) || fail "gh pr merge --auto: $out"
    echo "gate: shipped: $url — auto-merge is set, GitHub merges it when the required CI is green. Do not wait for CI; you are done." ;;
esac
