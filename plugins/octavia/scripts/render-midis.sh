#!/usr/bin/env bash
# Renders every MIDI in midi/ into renders/midi/, with the settings each
# piece is known to need. Extra options are passed on to every render
# (e.g. scripts/render-midis.sh --player student).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --quiet
out=renders/midi
mkdir -p "$out"
for f in midi/*.mid; do
  name=$(basename "$f" .mid)
  case $name in
    rainbow) opts="--instrument violin --bright 0.3 --transpose -12" ;;
    angel) opts="--track 8 --bright 0.5" ;;
    crystalize|roundtable) opts="--instrument violin" ;;
    beethoven2) opts="--track 5" ;;
    *) opts="" ;;
  esac
  # shellcheck disable=SC2086
  ./target/release/octavia "$f" $opts "$@" -o "$out" | grep -E '^[^ ].*[0-9.]+ s, ' | cut -d'|' -f1
done
