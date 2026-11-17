#!/bin/bash
# Checks Cryo Engine on this Mac and measures its speed. Double-click it in Finder, or run
# `bash scripts/check-mac.command` in a terminal. Nothing is committed or published.
#
#  1. builds the editor (release) and runs the automatic tests;
#  2. creates the two example games in a temporary folder and runs each one for 20 seconds
#     with the frame-time benchmark (a window opens and closes by itself: leave it alone);
#  3. writes everything to mac-report.txt at the top of the repository.
cd "$(dirname "$0")/.." || exit 1
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
REPORT="$PWD/mac-report.txt"
WORK="$PWD/target/check"   # inside the repository, so the logs are easy to find

if ! command -v cargo >/dev/null; then
  echo "Rust is not installed. Install it with:"
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  echo "then run this script again."
  echo "Rust not installed" > "$REPORT"
  read -r -p "Press Enter to close." _
  exit 1
fi

{
  echo "== Cryo Engine check on macOS, $(date)"
  echo "macOS $(sw_vers -productVersion) ($(uname -m)), $(sysctl -n machdep.cpu.brand_string 2>/dev/null)"
  system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Chipset Model|VRAM|Resolution" | sed 's/^ */  /'
  echo "rust: $(rustc --version)"
} > "$REPORT"

echo "== 1/3  Build and tests (a few minutes the first time) =="
mkdir -p "$WORK"
if cargo build --release > "$WORK/build.log" 2>&1; then
  echo "build: ok" | tee -a "$REPORT"
else
  echo "BUILD FAILED:" | tee -a "$REPORT"
  tail -30 "$WORK/build.log" | tee -a "$REPORT"
fi
cargo test --release 2>&1 | grep -E "test result|FAILED|panicked" | tee -a "$REPORT"

echo "== 2/3  Benchmarks (20 s each; a window opens and closes by itself) =="
rm -rf "$WORK/outpost" "$WORK/ashen"
for game in outpost ashen; do
  echo "-- $game" | tee -a "$REPORT"
  CRYO_NO_TOUR=1 RUST_BACKTRACE=1 ./target/release/cryo "$WORK/$game" "--$game" --bench=20 > "$WORK/$game.log" 2>&1
  echo "exit code: $?" | tee -a "$REPORT"
  if [ -f "$WORK/$game/bench-report.txt" ]; then
    cat "$WORK/$game/bench-report.txt" >> "$REPORT"
  else
    echo "no report; last lines of the log:" >> "$REPORT"
    tail -20 "$WORK/$game.log" >> "$REPORT"
  fi
done

echo "== 3/3  Done =="
cat "$REPORT"
echo
echo "Report saved in mac-report.txt."
[ -n "$CRYO_CHECK_NO_WAIT" ] || read -r -t 20 -p "Press Enter to close (closes by itself in 20 s)." _
