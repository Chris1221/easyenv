#!/usr/bin/env bash
# Benchmarks the "cd into a directory N levels deep" hot path for easyenv,
# direnv, autoenv, shadowenv, mise, and zsh-autoenv, across a range of
# nesting depths.
#
# All six tools are timed doing the same well-defined thing: a cold load
# (first visit, nothing cached) of a chain of directories each contributing
# a config file, ending at depth N. This deliberately excludes each tool's
# warm/fast-path behavior where it has one -- see docs/reference/benchmarks.md
# for why that scope was chosen, and for the two genuine architectural
# differences this benchmark has to work around: shadowenv and zsh-autoenv
# both require an *explicit* per-directory opt-in to inherit a parent's
# config (a `.shadowenv.d/parent` symlink, and an `autoenv_source_parent`
# call, respectively) -- unlike easyenv/direnv/mise/autoenv, which nest
# automatically. The fixture builders below set that opt-in up at every
# level so the comparison is still apples-to-apples on "does the merged
# result end up correct," even though the tools differ on whether nesting
# is automatic.
#
# Requires:
#   - a release build of easyenv (EASYENV_BIN, default: target/release/easyenv)
#   - direnv on PATH
#   - a clone of https://github.com/hyperupcall/autoenv (AUTOENV_DIR)
#   - the shadowenv binary (SHADOWENV_BIN, default: shadowenv on PATH)
#   - the mise binary (MISE_BIN, default: mise on PATH)
#   - zsh on PATH, plus a clone of https://github.com/Tarrasch/zsh-autoenv (ZSH_AUTOENV_DIR)
#
# Writes benches/results.csv (tool,depth,trial,nanoseconds).
set -euo pipefail

TRIALS="${TRIALS:-15}"
DEPTHS=(0 1 2 4 8 16 32 64 128)
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EASYENV_BIN="${EASYENV_BIN:-$REPO_ROOT/target/release/easyenv}"
AUTOENV_DIR="${AUTOENV_DIR:-}"
SHADOWENV_BIN="${SHADOWENV_BIN:-shadowenv}"
MISE_BIN="${MISE_BIN:-mise}"
ZSH_AUTOENV_DIR="${ZSH_AUTOENV_DIR:-}"
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
if ! command -v "$SHADOWENV_BIN" >/dev/null 2>&1; then
  echo "error: shadowenv not found (set SHADOWENV_BIN or put it on PATH)" >&2
  exit 1
fi
if ! command -v "$MISE_BIN" >/dev/null 2>&1; then
  echo "error: mise not found (set MISE_BIN or put it on PATH)" >&2
  exit 1
fi
if ! command -v zsh >/dev/null 2>&1; then
  echo "error: zsh not found on PATH" >&2
  exit 1
fi
if [ -z "$ZSH_AUTOENV_DIR" ] || [ ! -f "$ZSH_AUTOENV_DIR/autoenv.plugin.zsh" ]; then
  echo "error: ZSH_AUTOENV_DIR must point at a clone of https://github.com/Tarrasch/zsh-autoenv" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/autoenv-state" "$WORK/zsh-autoenv-state"

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

# mise.toml per level. Unlike build_chain's single-file-per-directory
# tools, mise nests automatically (no per-level opt-in needed, confirmed by
# testing against the real binary) so this mirrors build_chain closely.
build_chain_mise() {
  local depth="$1" root="$2"
  mkdir -p "$root"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    mkdir -p "$dir"
    {
      echo "[env]"
      printf 'BENCH_VAR_%d = "level_%d"\n' "$level" "$level"
    } >"$dir/mise.toml"
  done
  echo "$dir"
}

# mise requires each mise.toml to be explicitly trusted once; `mise trust`
# takes the config file directly, no need to cd into each directory.
trust_mise_chain() {
  local depth="$1" root="$2"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    "$MISE_BIN" trust "$dir/mise.toml" >/dev/null 2>&1
  done
}

# .shadowenv.d/*.lisp per level. Unlike the other tools, shadowenv does NOT
# nest automatically -- it requires an explicit `.shadowenv.d/parent`
# symlink at every level pointing at the immediate parent's `.shadowenv.d`
# (confirmed by testing against the real binary). Given this chain's
# structure (each level directly nested inside the previous), that target
# is always the same relative path, `../../.shadowenv.d`, regardless of
# depth. The topmost level has no true ancestor in this synthetic chain, so
# it gets no parent symlink, matching every other tool's chain builder.
build_chain_shadowenv() {
  local depth="$1" root="$2"
  mkdir -p "$root"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    mkdir -p "$dir/.shadowenv.d"
    printf '(env/set "BENCH_VAR_%d" "level_%d")\n' "$level" "$level" \
      >"$dir/.shadowenv.d/000_bench.lisp"
    if ((level > 1)); then
      ln -sfn "../../.shadowenv.d" "$dir/.shadowenv.d/parent"
    fi
  done
  echo "$dir"
}

# shadowenv requires each directory to be explicitly trusted once;
# `shadowenv trust` only operates on the current directory, no path arg.
trust_shadowenv_chain() {
  local depth="$1" root="$2"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    (cd "$dir" && "$SHADOWENV_BIN" trust >/dev/null 2>&1)
  done
}

# .autoenv.zsh per level. Like shadowenv, zsh-autoenv does NOT nest
# automatically by default -- each level must explicitly call
# `autoenv_source_parent` at the top of its .autoenv.zsh to inherit the
# parent's variables (confirmed by testing against the real plugin).
build_chain_zsh_autoenv() {
  local depth="$1" root="$2"
  mkdir -p "$root"
  local dir="$root"
  local level
  for ((level = 1; level <= depth; level++)); do
    dir="$dir/d$level"
    mkdir -p "$dir"
    {
      if ((level > 1)); then
        echo "autoenv_source_parent"
      fi
      printf 'export BENCH_VAR_%d=level_%d\n' "$level" "$level"
    } >"$dir/.autoenv.zsh"
  done
  echo "$dir"
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

  root="$WORK/shadowenv-$depth"
  leaf=$(build_chain_shadowenv "$depth" "$root")
  trust_shadowenv_chain "$depth" "$root"
  for ((trial = 1; trial <= TRIALS; trial++)); do
    start=$(date +%s%N)
    (cd "$leaf" && "$SHADOWENV_BIN" hook --shellpid $$ >/dev/null 2>&1)
    end=$(date +%s%N)
    echo "shadowenv,$depth,$trial,$((end - start))" >>"$OUT_CSV"
  done

  root="$WORK/mise-$depth"
  leaf=$(build_chain_mise "$depth" "$root")
  trust_mise_chain "$depth" "$root"
  for ((trial = 1; trial <= TRIALS; trial++)); do
    start=$(date +%s%N)
    (cd "$leaf" && env -u __MISE_DIFF -u __MISE_SESSION "$MISE_BIN" hook-env -s bash >/dev/null 2>&1)
    end=$(date +%s%N)
    echo "mise,$depth,$trial,$((end - start))" >>"$OUT_CSV"
  done

  root="$WORK/zsh-autoenv-$depth"
  leaf=$(build_chain_zsh_autoenv "$depth" "$root")
  for ((trial = 1; trial <= TRIALS; trial++)); do
    ns=$(
      ZSH_AUTOENV_LEAF="$leaf" ZSH_AUTOENV_PLUGIN_DIR="$ZSH_AUTOENV_DIR" \
        ZSH_AUTOENV_ROOT="$root" ZSH_AUTOENV_DEPTH="$depth" \
        AUTOENV_AUTH_FILE="$WORK/zsh-autoenv-state/authorized" \
        zsh <<'INNER'
source "$ZSH_AUTOENV_PLUGIN_DIR/autoenv.plugin.zsh"
dir="$ZSH_AUTOENV_ROOT"
for ((level = 1; level <= ZSH_AUTOENV_DEPTH; level++)); do
  dir="$dir/d$level"
  _autoenv_authorize "$dir/.autoenv.zsh"
done
start=$(date +%s%N)
cd "$ZSH_AUTOENV_LEAF" >/dev/null
end=$(date +%s%N)
echo $((end - start))
INNER
    )
    echo "zsh-autoenv,$depth,$trial,$ns" >>"$OUT_CSV"
  done
done

echo "wrote $OUT_CSV" >&2
