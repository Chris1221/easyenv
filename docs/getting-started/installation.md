# Installation

## 1. Get the binary

=== "Quick install (recommended)"

    ```console
    $ curl -fsSL https://raw.githubusercontent.com/Chris1221/easyenv/main/install.sh | bash
    ```

    This script:

    1. Detects your OS and CPU architecture
    2. Downloads the matching archive from the [latest release](https://github.com/Chris1221/easyenv/releases/latest) and verifies its sha256 checksum
    3. Installs the binary to `~/.local/bin` (no `sudo`)
    4. Detects whether you run bash or zsh, shows you the exact line(s) it wants to add to your rc file, and **asks for confirmation before touching anything** — it never edits a file without you explicitly saying yes, and it's a no-op if you've already run it before

    Read the script before piping it to `bash` if you'd like — it's a plain, commented shell script: [`install.sh`](https://github.com/Chris1221/easyenv/blob/main/install.sh).

    Override where it installs or which release/target it fetches with `EASYENV_INSTALL_DIR`, `EASYENV_VERSION`, or `EASYENV_TARGET`.

=== "Download a release manually"

    Grab the archive for your platform from the [latest release](https://github.com/Chris1221/easyenv/releases/latest), then extract and install it:

    ```console
    $ curl -LO https://github.com/Chris1221/easyenv/releases/latest/download/easyenv-<tag>-x86_64-unknown-linux-gnu.tar.gz
    $ tar xzf easyenv-<tag>-x86_64-unknown-linux-gnu.tar.gz
    $ install -m 755 easyenv ~/.local/bin/easyenv
    ```

    Replace `x86_64-unknown-linux-gnu` with your platform:

    | Platform | Target |
    |---|---|
    | Linux x86_64 | `x86_64-unknown-linux-gnu` (or `-musl` for a fully static binary) |
    | Linux ARM64 | `aarch64-unknown-linux-gnu` (or `-musl`) |
    | macOS Intel | `x86_64-apple-darwin` |
    | macOS Apple Silicon | `aarch64-apple-darwin` |
    | Windows x86_64 | `x86_64-pc-windows-msvc` (`.zip`) |

    Each archive ships with a `.sha256` checksum alongside it on the release page, worth verifying before you install anything downloaded from the internet.

    Make sure the install directory (e.g. `~/.local/bin`) is on your `PATH`.

=== "Build from source with Cargo"

    ```console
    $ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # if you don't have Rust
    $ git clone https://github.com/Chris1221/easyenv.git
    $ cd easyenv
    $ cargo build --release
    $ install -m 755 target/release/easyenv ~/.local/bin/easyenv
    ```

    Useful if you're on a platform without a prebuilt archive, or want to build from a specific commit.

!!! info "Windows"
    A Windows binary is published, but the shell integration below (`easyenv init`/`hook`) currently only implements bash and zsh — PowerShell support is on the roadmap. The binary itself works standalone (`easyenv status`, etc.) on Windows today. `install.sh` is a bash script and won't run natively on Windows outside WSL.

## 2. Wire up your shell

If you used the quick install script above and said yes to the prompt, you're already done — skip to the [Quickstart](quickstart.md).

Otherwise, run `easyenv init <shell>` to see the one-line snippet for your shell:

=== "Bash"

    ```console
    $ easyenv init bash
    ```

    Add the printed line to `~/.bashrc`:

    ```bash
    eval "$(easyenv hook bash)"
    ```

=== "Zsh"

    ```console
    $ easyenv init zsh
    ```

    Add the printed line to `~/.zshrc`:

    ```zsh
    eval "$(easyenv hook zsh)"
    ```

Restart your shell (or `source` the rc file) and you're done — continue to the [Quickstart](quickstart.md).

!!! info "Fish and PowerShell"
    Not supported yet. They're on the roadmap; bash and zsh are the current focus.
