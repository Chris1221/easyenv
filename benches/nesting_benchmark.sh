#!/usr/bin/env bash
# Benchmarks the "cd into a directory N levels deep" hot path for easyenv,
# direnv, and autoenv, across a range of nesting depths.
#
# All three tools are timed doing the same well-defined thing: a cold load
# (first visit, nothing cached) of a chain of directories each contributing
# a config file, ending at depth N. This deliberately excludes each tool's
# warm/fast-path behavior (easyenv and direnv both have one; autoenv has
# none) -- see docs/reference/benchmarks.md for why that scope was chosen.
#
# Requires:
#   - a release build of easyenv (EASYENV_BIN, default: target/release/easyenv)
#   - direnv on PATH
#   - a clone of https://github.com/hyperupcall/autoenv (AUTOENV_DIR)
#
# Writes benches/results.csv (tool,depth,trial,nanoseconds).
set -euo pipefail

TRIALS="${TRIALS:-15}"
DEPTHS=(0 1 2 4 8 16 32 64 128)
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EASYENV_BIN="${EASYENV_BIN:-$REPO_ROOT/target/release/easyenv}"
AUTOENV_DIR="${AUTOENV_DIR:-}"
OUT_CSV="${OUT_CSV:-$REPO_ROOT/benches/results.csv}"

if ! command -v direnv >/dev/null 2>&1; then
  echo "error: direnv not found on PATH" >&2
  exit 1
fi
if [ ! -x "$EASYENV_BIN" ]; then
  echo "error: easyenv release binary not found at $EASYENV_BIN (run: cargo build --release)" >&2
  exit 1
fi
if [ -z "$AUTOENV_DIR" ] || [ ! -f "$AUTOENV_DIR/activate.sh" ]; then
  echo "error: AUTOENV_DIR must point at a clone of https://github.com/hyperupcall/autoenv" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/autoenv-state"

# Builds $1 nested directories under $2, each containing a file named $3
# with an export line for that level, and echoes the deepest directory's
# path (or $2 itself, unmodified, when depth is 0).
build_chain() {
  local depth="$1" root="$2" filename="$3"
  mkdir -p "$root"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    mkdir -p "$dir"
    printf 'export BENCH_VAR_%d=level_%d\n' "$level" "$level" >"$dir/$filename"
  done
  echo "$dir"
}

# direnv requires each .envrc to be explicitly trusted once; do that here,
# outside the timed region.
allow_direnv_chain() {
  local depth="$1" root="$2"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    direnv allow "$dir" >/dev/null 2>&1
  done
}

echo "tool,depth,trial,nanoseconds" >"$OUT_CSV"

for depth in "${DEPTHS[@]}"; do
  echo "== depth $depth ==" >&2

  root="$WORK/easyenv-$depth"
  leaf=$(build_chain "$depth" "$root" ".env")
  for ((trial = 1; trial <= TRIALS; trial++)); do
    start=$(date +%s%N)
    (cd "$leaf" && env -u EASYENV_STATE "$EASYENV_BIN" export bash >/dev/null)
    end=$(date +%s%N)
    echo "easyenv,$depth,$trial,$((end - start))" >>"$OUT_CSV"
  done

  root="$WORK/direnv-$depth"
  leaf=$(build_chain "$depth" "$root" ".envrc")
  allow_direnv_chain "$depth" "$root"
  for ((trial = 1; trial <= TRIALS; trial++)); do
    start=$(date +%s%N)
    (cd "$leaf" && env -u DIRENV_DIFF -u DIRENV_WATCHES -u DIRENV_DIR direnv export bash >/dev/null 2>&1)
    end=$(date +%s%N)
    echo "direnv,$depth,$trial,$((end - start))" >>"$OUT_CSV"
  done

  root="$WORK/autoenv-$depth"
  leaf=$(build_chain "$depth" "$root" ".env")
  for ((trial = 1; trial <= TRIALS; trial++)); do
    ns=$(
      AUTOENV_LEAF="$leaf" AUTOENV_DIR="$AUTOENV_DIR" \
        AUTOENV_AUTH_FILE="$WORK/autoenv-state/authorized" \
        AUTOENV_NOTAUTH_FILE="$WORK/autoenv-state/notauthorized" \
        AUTOENV_ASSUME_YES=1 \
        bash <<'INNER'
source "$AUTOENV_DIR/activate.sh"
enable_autoenv
start=$(date +%s%N)
cd "$AUTOENV_LEAF" >/dev/null
end=$(date +%s%N)
echo $((end - start))
INNER
    )
    echo "autoenv,$depth,$trial,$ns" >>"$OUT_CSV"
  done
done

echo "wrote $OUT_CSV" >&2
