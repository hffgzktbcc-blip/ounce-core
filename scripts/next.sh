#!/usr/bin/env bash
# Entry point for every agent: prints where the work stands (step, checkpoint, branch, oracles).
# Usage: scripts/next.sh [step]. Sourced by scripts/gate.sh for the PLAN.md helpers below.
# Plain bash 3.2 (macOS), git, gh, python3.

PLAN=PLAN.md

# Lines of step N's block in PLAN.md (or the file given as $2).
step_block() {
  awk -v n="$1" '
    $0 ~ "^- \\[[ x]\\] \\*\\*" n "\\." { on = 1; print; next }
    on && (/^- \[/ || /^## / || /^$/) { exit }
    on { print }' "${2:-$PLAN}"
}
first_unchecked() { sed -n 's/^- \[ \] \*\*\([0-9][0-9a-z]*\)\..*/\1/p' "${1:-$PLAN}" | head -1; }
is_ticked() { step_block "$1" "${2:-$PLAN}" | head -1 | grep -q '^- \[x\]'; }
checkpoint_of() { step_block "$1" "${2:-$PLAN}" | grep -E '^ +(Checkpoint \(|Next:|Tried/ruled out:|Decisions:)'; }
main_plan() { git show origin/main:PLAN.md > "${TMPDIR:-/tmp}/ounce-main-plan.md" 2>/dev/null && echo "${TMPDIR:-/tmp}/ounce-main-plan.md"; }

agent_name() {
  if [ -n "${OUNCE_AGENT:-}" ]; then echo "$OUNCE_AGENT"; elif [ -n "${CLAUDECODE:-}" ]; then echo claude
  elif [ -n "${CODEX_HOME:-}${CODEX_SANDBOX:-}" ]; then echo codex; else echo "${USER:-agent}"; fi
}

# Rewrite step N's block: drop its checkpoint lines, append $TAIL (ENVIRON), tick it when TICK=1.
rewrite_block() {
  TAIL=$2 TICK=${3:-0} awk -v n="$1" '
    $0 ~ "^- \\[[ x]\\] \\*\\*" n "\\." { on = 1; if (ENVIRON["TICK"] == 1) sub(/^- \[ \]/, "- [x]"); print; next }
    on && (/^- \[/ || /^## / || /^$/) { print ENVIRON["TAIL"]; on = 0 }
    on && /^ +(Checkpoint \(|Next:|Tried\/ruled out:|Decisions:)/ { next }
    { print }
    END { if (on) print ENVIRON["TAIL"] }' "$PLAN" > "$PLAN.tmp" && mv "$PLAN.tmp" "$PLAN"
}

# write_checkpoint N "<RED|GREEN — summary>" [next] [tried] [decision]: Tried/Decisions accumulate, Next is replaced.
write_checkpoint() {
  local n=$1 old nxt tail
  old=$(checkpoint_of "$n")
  nxt=${3:+      Next: $3}
  [ -n "$nxt" ] || nxt=$(echo "$old" | grep -m1 '^ *Next:')
  [ -n "$nxt" ] || nxt="      Next: (not recorded)"
  tail="      Checkpoint ($(date -u '+%F %H:%MZ'), $(agent_name), $(git rev-parse --short HEAD)): $2
$nxt"
  old=$(echo "$old" | grep -E '^ *(Tried/ruled out|Decisions):')
  [ -n "$old" ] && tail="$tail
$old"
  [ -n "${4:-}" ] && tail="$tail
      Tried/ruled out: $4"
  [ -n "${5:-}" ] && tail="$tail
      Decisions: $5"
  rewrite_block "$n" "$tail"
}

# The step this branch checkpointed (a checkpoint that main does not have), if any.
branch_step() {
  local mp n; mp=$(main_plan) || return 0
  for n in $(sed -n 's/^- \[[ x]\] \*\*\([0-9][0-9a-z]*\)\..*/\1/p' "$PLAN"); do
    if checkpoint_of "$n" | grep -q 'Checkpoint (' && [ "$(checkpoint_of "$n")" != "$(checkpoint_of "$n" "$mp")" ]; then
      echo "$n"; return 0
    fi
  done
  # A step ticked on this branch but not yet on main (shipped, waiting for GitHub to merge).
  for n in $(sed -n 's/^- \[x\] \*\*\([0-9][0-9a-z]*\)\..*/\1/p' "$PLAN"); do
    is_ticked "$n" "$mp" || { echo "$n"; return 0; }
  done
}
current_step() { local s; s=${STEP:-$(branch_step)}; [ -n "$s" ] || s=$(first_unchecked "$(main_plan || echo "$PLAN")"); echo "$s"; }

# Path to the newest Ounce-Verify canonical report (this branch's runs first), downloaded once into target/next.
# Prints "<path> <run url> <branch>"; nothing if none is reachable.
latest_report() {
  local branch=$1 only_branch=${2:-} id url head dir
  read -r id url head < <( { gh run list --workflow external-corpus.yml --branch "$branch" -L 10 \
        --json databaseId,url,headBranch,status -q '.[]|select(.status=="completed")|"\(.databaseId) \(.url) \(.headBranch)"'
      [ -n "$only_branch" ] || gh run list --workflow external-corpus.yml -L 20 \
        --json databaseId,url,headBranch,status -q '.[]|select(.status=="completed")|"\(.databaseId) \(.url) \(.headBranch)"'
    } 2>/dev/null | while read -r i u h; do
      d=target/next/report-$i
      [ -f "$d/canonical-report.json" ] || gh run download "$i" -n ounce-verify-canonical-report -D "$d" >/dev/null 2>&1
      [ -f "$d/canonical-report.json" ] && { echo "$i $u $h"; break; }
    done)
  [ -n "${id:-}" ] && echo "target/next/report-$id/canonical-report.json $url $head"
}

report_summary() {
  python3 - "$1" <<'EOF'
import json, sys
progs = {}
for r in json.load(open(sys.argv[1]))["records"]:
    progs.setdefault(r["case_id"].split("/")[1], set()).add(r["verdict"])
bad = sorted(p for p, v in progs.items() if v != {"pass"})
print(f"{len(progs) - len(bad)}/{len(progs)} pass; failing: {', '.join(bad) or 'none'}")
EOF
}

# A shipped step whose PR checks failed: put the checkpoint back (with the CI error) and stop auto-merge.
untick_after_red_ci() {
  local n=$1 pr=$2 failed ship
  failed=$(gh pr checks "$pr" --json name,state,link -q '.[]|select(.state=="FAILURE")|"\(.name) \(.link)"' 2>/dev/null)
  [ -n "$failed" ] || return 0
  ship=$(git log -1 --format=%H --grep="^ship(step$n)" origin/main..HEAD)
  [ -n "$ship" ] && git diff --quiet -- "$PLAN" || { echo "  CI failed on #$pr but $PLAN is dirty or has no ship commit; fix by hand."; return 0; }
  gh pr merge "$pr" --disable-auto >/dev/null 2>&1
  git revert --no-commit "$ship" &&
    write_checkpoint "$n" "RED — CI failed on #$pr: $(echo "$failed" | head -3 | tr '\n' ';')" &&
    git commit -q -m "wip(step$n): RED — CI failed after ship, tick removed" -- "$PLAN" && git push -q
  echo "  CI failed on #$pr: tick removed on the branch, error written to the checkpoint, auto-merge off."
}

next_main() {
  cd "$(git rev-parse --show-toplevel)" || exit 1
  git fetch -q origin main 2>/dev/null
  local n branch pr mp cp_age
  n=$(current_step); branch=$(git branch --show-current)
  mp=$(main_plan || echo "$PLAN")
  pr=$(gh pr view --json number -q .number 2>/dev/null)
  if [ -n "$pr" ] && is_ticked "$n" && ! is_ticked "$n" "$mp"; then untick_after_red_ci "$n" "$pr"; fi

  echo "== ounce-core: next =="
  step_block "$n" | head -1 | sed -E 's/^- \[(.)\] \*\*([^*]*)\*\*.*/Step \2  [\1]/'
  step_block "$n" | grep -m1 '\*Done when:\*' | sed 's/^ */  /'
  if checkpoint_of "$n" >/dev/null; then checkpoint_of "$n" | sed 's/^ */  /'
  elif is_ticked "$n"; then step_block "$n" | grep '\*Proof:\*' | sed 's/^ */  /'
  else echo "  Checkpoint: none yet"; fi
  echo "Branch: ${branch:-detached}  last: $(git log -1 --format='%h %s (%cr)')"
  [ "$branch" = main ] && echo "  On main: make a branch first (git switch -c step$n-<topic>)."
  echo "  dirty: $(git status --porcelain | wc -l | tr -d ' ') files $(git status --porcelain | head -6 | awk '{print $2}' | tr '\n' ' ')"
  cp_age=$(git log -1 --format=%ct --grep='^wip(' origin/main..HEAD 2>/dev/null)
  if [ -n "$cp_age" ] && [ $(( $(date +%s) - cp_age )) -lt 1800 ]; then
    echo "  WARNING: last checkpoint $(( ($(date +%s) - cp_age) / 60 )) min ago: another agent may be active on this branch."
  fi
  [ -n "$pr" ] && echo "PR: $(gh pr view "$pr" --json number,state,autoMergeRequest,url -q '"#\(.number) \(.state) auto-merge \(if .autoMergeRequest then "on (GitHub merges when CI is green; do not wait for it)" else "off" end) \(.url)"')"
  local rep; rep=$(latest_report "${branch:-main}")
  if [ -n "$rep" ]; then set -- $rep; echo "Oracles (${3:-?}, $2): $(report_summary "$1")"; else echo "Oracles: no canonical report reachable (gh auth?)"; fi
  echo "Open PRs: $(gh pr list --json number,headRefName -q '.[]|"#\(.number) \(.headRefName)"' 2>/dev/null | tr '\n' ' ')"
  echo "Deeper: docs/HANDOFF_HISTORY.md, docs/*-oracle.md, docs/corpus-oracles.md, git log origin/main..HEAD"
  echo "Save: scripts/gate.sh --checkpoint \"<what/why>\" --next \"<action>\"   Finish: scripts/gate.sh --ship"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then STEP=${1:-${STEP:-}} next_main; fi
