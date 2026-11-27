#!/usr/bin/env bash
# Re-renders the reference pieces Rhodexa liked, with their exact settings,
# and shows which notes are now played differently than last time.
#
#   scripts/refs.sh            render + compare with the last run
#   scripts/refs.sh --accept   make this run the new baseline
#
# Renders and decision logs go to renders/refs/ (not committed). MIDI files
# live in midi/ (not committed either); missing ones are skipped.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --quiet -p octavia-render
out=renders/refs
mkdir -p "$out"

# name | input | options
refs=(
  "rainbow-violin|midi/rainbow.mid|--instrument violin --bright 0.3 --transpose -12"
  "marmalade|midi/marmalade.mid|"
  "angel|midi/angel.mid|--track 8 --bright 0.5"
  "crystalize-violin|midi/crystalize.mid|--instrument violin"
  "beethoven-virus|midi/beethoven2.mid|--track 5"
  "prelude-inferred|scores/prelude.roll|"
  "lament|scores/lament.txt|"
  "megalovania|midi/megalovania.mid|"
)

changed=0
for r in "${refs[@]}"; do
  IFS='|' read -r name input opts <<<"$r"
  if [[ ! -f $input ]]; then
    echo "skip $name ($input missing)"
    continue
  fi
  dir=$out/$name
  mkdir -p "$dir"
  # shellcheck disable=SC2086
  ../../target/release/octavia "$input" $opts --explain -o "$dir" | grep -E '^ +[0-9]' >"$dir/decisions.txt"
  if [[ ${1:-} == --accept || ! -f $dir/baseline.txt ]]; then
    cp "$dir/decisions.txt" "$dir/baseline.txt"
    echo "$name: baseline saved ($(wc -l <"$dir/decisions.txt") notes)"
  elif diff -q "$dir/baseline.txt" "$dir/decisions.txt" >/dev/null; then
    echo "$name: unchanged"
  else
    n=$(diff "$dir/baseline.txt" "$dir/decisions.txt" | grep -c '^>' || true)
    echo "$name: $n notes played differently:"
    { diff "$dir/baseline.txt" "$dir/decisions.txt" | grep '^[<>]' | head -12 | sed 's/^/    /'; } || true
    changed=1
  fi
done
exit $changed
