#!/usr/bin/env bash
# Tapes for the faceplate mock from every MIDI in midi/, into
# docs/design/tapes/local/ (git-ignored: MIDI files may be copyrighted).
# Same per-piece settings as render-midis.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
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
  scripts/tape.py --local "$f" -- $opts "$@"
done
