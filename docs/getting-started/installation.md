# Installation

!!! note "Prebuilt binaries are coming"
    easyenv is early in development. There's no `curl | sh` installer or package manager release yet — for now, build it from source with Cargo. This page will be replaced with one-line install instructions once releases exist.

## 1. Install Rust (if you don't have it)

```console
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## 2. Build easyenv

```console
$ git clone https://github.com/Chris1221/easyenv.git
$ cd easyenv
$ cargo build --release
```

The binary lands at `target/release/easyenv`. Put it on your `PATH`, e.g.:

```console
$ install -m 755 target/release/easyenv ~/.local/bin/easyenv
```

Make sure `~/.local/bin` is on your `PATH` (check with `echo $PATH`).

## 3. Wire up your shell

Run `easyenv init <shell>` to see the one-line snippet for your shell:

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
