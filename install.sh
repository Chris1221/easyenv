#!/usr/bin/env bash
# Installs easyenv: detects OS/arch, downloads the latest release from
# GitHub, verifies its checksum, installs to a user-writable directory (no
# sudo), and offers to wire up your shell's rc file (with confirmation --
# it will not edit anything without asking first).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Chris1221/easyenv/main/install.sh | bash
#
# Environment overrides:
#   EASYENV_INSTALL_DIR   where to install the binary (default: ~/.local/bin)
#   EASYENV_VERSION       a specific tag to install, e.g. v0.1.0 (default: latest)
#   EASYENV_TARGET        override the detected target triple
set -euo pipefail

REPO="Chris1221/easyenv"
INSTALL_DIR="${EASYENV_INSTALL_DIR:-$HOME/.local/bin}"
RC_MARKER="# Added by the easyenv install script"
# Global (not `local` to main) so the EXIT trap can still see it after
# main() returns on the success path -- a `local` would go out of scope
# by the time the script falls off the end and the trap actually fires,
# which is an unbound-variable error under `set -u`.
tmp_dir=""

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
error() {
  printf '\033[1;31merror:\033[0m %s\n' "$*" >&2
  exit 1
}

detect_target() {
  if [ -n "${EASYENV_TARGET:-}" ]; then
    echo "$EASYENV_TARGET"
    return
  fi

  local os arch libc
  os=$(uname -s)
  arch=$(uname -m)

  case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) error "Unsupported architecture: $arch (see https://github.com/$REPO/releases for manual downloads)" ;;
  esac

  case "$os" in
    Linux)
      # Default to the statically-linked musl build: it has no glibc version
      # dependency, so it runs on any Linux regardless of how old the host's
      # glibc is. Set EASYENV_TARGET to override (e.g. ...-unknown-linux-gnu).
      libc="musl"
      echo "${arch}-unknown-linux-${libc}"
      ;;
    Darwin)
      echo "${arch}-apple-darwin"
      ;;
    *)
      error "Unsupported OS: $os (see https://github.com/$REPO/releases for manual downloads)"
      ;;
  esac
}

resolve_tag() {
  if [ -n "${EASYENV_VERSION:-}" ]; then
    echo "$EASYENV_VERSION"
    return
  fi
  # curl's full output is captured into a variable first, then piped to
  # grep/sed separately -- piping curl directly into `grep -m1` lets grep
  # close the pipe as soon as it finds a match, which sends curl a SIGPIPE
  # and (with `pipefail`) aborts the whole script under `set -e`.
  local json
  json=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
  printf '%s\n' "$json" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
}

verify_checksum() {
  local file="$1" expected actual
  expected=$(awk '{print $1}' "${file}.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  else
    warn "no sha256sum or shasum found -- skipping checksum verification"
    return 0
  fi
  [ "$actual" = "$expected" ] || error "checksum mismatch for $file (expected $expected, got $actual)"
}

path_has() {
  case ":$PATH:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

print_manual_instructions() {
  echo
  echo "Add one of these to your shell's rc file:"
  echo "  bash (~/.bashrc): eval \"\$($INSTALL_DIR/easyenv hook bash)\""
  echo "  zsh  (~/.zshrc):  eval \"\$($INSTALL_DIR/easyenv hook zsh)\""
  if ! path_has "$INSTALL_DIR"; then
    echo
    echo "Also make sure $INSTALL_DIR is on your PATH, e.g.:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
}

setup_shell_hook() {
  local shell_name rc_file hook_line path_line reply

  shell_name=$(basename "${SHELL:-}")
  case "$shell_name" in
    bash) rc_file="$HOME/.bashrc" ;;
    zsh) rc_file="$HOME/.zshrc" ;;
    *)
      warn "Could not detect bash or zsh from \$SHELL ($SHELL:-unset)."
      print_manual_instructions
      return 0
      ;;
  esac

  if [ -f "$rc_file" ] && grep -qF "$RC_MARKER" "$rc_file" 2>/dev/null; then
    info "$rc_file already has an easyenv hook line -- leaving it alone."
    return 0
  fi

  hook_line="eval \"\$(\"$INSTALL_DIR/easyenv\" hook $shell_name)\""
  path_line="export PATH=\"$INSTALL_DIR:\$PATH\""

  echo
  info "To finish setup, easyenv needs this added to $rc_file:"
  echo
  if ! path_has "$INSTALL_DIR"; then
    printf '    %s\n' "$path_line"
  fi
  printf '    %s\n' "$hook_line"
  echo

  if [ ! -r /dev/tty ]; then
    warn "No terminal available to ask for confirmation -- add the line(s) above to $rc_file yourself."
    return 0
  fi

  printf 'Append the above to %s now? [y/N] ' "$rc_file"
  if ! read -r reply 2>/dev/null </dev/tty; then
    reply="n"
  fi
  case "$reply" in
    y | Y | yes | YES)
      {
        echo ""
        echo "$RC_MARKER"
        if ! path_has "$INSTALL_DIR"; then
          echo "$path_line"
        fi
        echo "$hook_line"
      } >>"$rc_file"
      info "Updated $rc_file. Restart your shell (or \`source $rc_file\`) to start using easyenv."
      ;;
    *)
      info "Skipped. Add the line(s) above to $rc_file whenever you're ready."
      ;;
  esac
}

main() {
  local target tag base_name archive checksum_asset url

  target=$(detect_target)
  info "Detected platform: $target"

  tag=$(resolve_tag)
  [ -n "$tag" ] || error "Could not determine the release to install."
  info "Installing $tag"

  base_name="easyenv-${tag}-${target}"
  archive="${base_name}.tar.gz"
  checksum_asset="${base_name}.sha256"
  url="https://github.com/$REPO/releases/download/${tag}"

  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' EXIT

  info "Downloading $archive"
  curl -fsSL -o "$tmp_dir/$archive" "$url/$archive" \
    || error "Download failed: $url/$archive (is there a release for $target?)"
  # The checksum asset is named after the archive's base name, not the
  # archive filename itself (e.g. foo.sha256, not foo.tar.gz.sha256).
  curl -fsSL -o "$tmp_dir/$archive.sha256" "$url/$checksum_asset" \
    || error "Checksum download failed: $url/$checksum_asset"

  info "Verifying checksum"
  (cd "$tmp_dir" && verify_checksum "$archive")

  info "Extracting"
  tar xzf "$tmp_dir/$archive" -C "$tmp_dir"

  mkdir -p "$INSTALL_DIR"
  install -m 755 "$tmp_dir/easyenv" "$INSTALL_DIR/easyenv"
  info "Installed to $INSTALL_DIR/easyenv"

  setup_shell_hook
}

main "$@"
