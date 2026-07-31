# easyenv

[![ci](https://github.com/Chris1221/easyenv/actions/workflows/ci.yml/badge.svg)](https://github.com/Chris1221/easyenv/actions/workflows/ci.yml)
[![docs](https://github.com/Chris1221/easyenv/actions/workflows/docs.yml/badge.svg)](https://github.com/Chris1221/easyenv/actions/workflows/docs.yml)

**easyenv** loads a directory's `.env` file into your shell the moment you `cd` into it, and unloads it the moment you `cd` back out. That's the whole feature.

```console
$ cd ~/projects/api
$ echo $DATABASE_URL
postgres://localhost/api_dev

$ cd ~/projects/frontend
$ echo $DATABASE_URL

```

No `.envrc`, no per-directory trust prompts, no leftover variables from the last project you were in. If a parent directory also has a `.env`, easyenv loads that too — the child directory's values win on conflicting keys, everything else from the parent still comes through. You install it once and forget it's there.

📖 Full documentation, tutorials, and the CLI reference: **[chrisbcole.me/easyenv](http://chrisbcole.me/easyenv/)**

## Quickstart

```console
$ curl -LO https://github.com/Chris1221/easyenv/releases/latest/download/easyenv-<tag>-x86_64-unknown-linux-gnu.tar.gz
$ tar xzf easyenv-*.tar.gz && install -m 755 easyenv ~/.local/bin/easyenv
# or: cargo install --path . --locked, from a clone

$ easyenv init bash                 # or: zsh
# add the printed line to your ~/.bashrc (or ~/.zshrc), then restart your shell

$ cd ~/some/project
$ echo 'API_KEY=demo-123' > .env
$ cd .. && cd -   # or just cd out and back in for real
$ echo $API_KEY
demo-123
```

Prebuilt archives for Linux (x86_64/aarch64, gnu/musl), macOS (Intel/Apple Silicon), and Windows are on the [releases page](https://github.com/Chris1221/easyenv/releases). See the [installation guide](http://chrisbcole.me/easyenv/getting-started/installation/) and [quickstart tutorial](http://chrisbcole.me/easyenv/getting-started/quickstart/) for the full walkthrough.

## How it compares

| | **easyenv** | [direnv](https://direnv.net/) | [autoenv](https://github.com/hyperupcall/autoenv) | sourcing `.env` by hand |
|---|---|---|---|---|
| Loads `.env` automatically, no boilerplate | ✅ | ❌ — needs an `.envrc` per directory that explicitly calls `dotenv` | ✅ | ❌ |
| Unloads automatically on `cd` out | ✅ | ✅ | ❌ — variables leak into the next directory unless you configure a `.env.leave`/hook yourself | ❌ |
| No per-directory trust/allow step | ✅ | ❌ — requires `direnv allow` the first time (and again on every edit) | ✅ | n/a |
| Parent directories merge automatically, child overrides parent | ✅ | only via an explicit `source_up` call in every child `.envrc` | ❌ | ❌ |
| Zero configuration files | ✅ | ❌ (`.envrc` per directory) | ✅ | n/a |
| Runtime | single Rust binary | Go binary | Bash script | — |

The short version: direnv is powerful and general (it can run arbitrary shell, not just `.env` files) but that generality is exactly why it needs an explicit `.envrc` and a trust step per directory — friction easyenv is designed to have none of. autoenv gets the "load on `cd` in" half right but doesn't unload without extra setup, so variables from one project bleed into the next. easyenv only does the one job — load/unload `.env` on `cd`, with nested overrides — and does it with no config and no prompts.

## Performance

Cold-load latency (time from `cd` to variables being set) as directory nesting gets deeper:

![Cold-load latency vs. directory nesting depth: easyenv stays under 6ms out to 128 levels, direnv grows to ~78ms, autoenv grows to over a second](docs/assets/benchmark-nesting.png)

easyenv stays flat at a few milliseconds out to 128 levels of nested `.env` files; direnv grows more noticeably; autoenv — which shells out to external commands per ancestor directory and has no caching — passes **a full second** at 128 levels. Methodology, caveats, and how to reproduce it: [Benchmarks](http://chrisbcole.me/easyenv/reference/benchmarks/).

## How it works

There's no daemon. A shell hook (`eval "$(easyenv hook bash)"` in your rc file) runs on every prompt and calls the `easyenv` binary, which discovers every `.env` from your current directory up to the filesystem root, merges them with child-overrides-parent precedence, diffs the result against what it previously set, and prints the minimal `export`/`unset` statements for your shell to `eval`. A cheap signature (directory + each `.env`'s mtime/size) lets it skip all of that instantly when nothing has changed, which is what keeps "run on every prompt" from being noticeable.

Full design write-up: [How it works](http://chrisbcole.me/easyenv/reference/how-it-works/).

## Status

Early. Bash and zsh are supported; fish and PowerShell are on the roadmap. Releases are cross-compiled and published automatically (see [Release process](#release-process) below); package manager distribution (Homebrew, Scoop/winget) is not set up yet. See [`PLAN.md`](PLAN.md) for the full roadmap.

## Development

```console
$ cargo test              # unit tests + end-to-end shell integration tests (bash & zsh)
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt
```

The shell integration tests in `tests/shell_integration.rs` actually launch interactive `bash`/`zsh` with the real hook installed and drive them through scripted `cd`/edit sequences — they're the same tests that run in CI.

### Commit messages

This repo follows [Conventional Commits](https://www.conventionalcommits.org/) (`feat: ...`, `fix: ...`, `docs: ...`, etc.), enforced on every pull request by the `commitlint` CI check. To get the same check locally before you commit:

```console
$ git config core.hooksPath .githooks
```

### Docs

The site at [chrisbcole.me/easyenv](http://chrisbcole.me/easyenv/) is built from `docs/` with [MkDocs Material](https://squidfunk.github.io/mkdocs-material/) and deployed automatically on push to `main`. To preview locally:

```console
$ pip install -r requirements-docs.txt
$ mkdocs serve
```

### Release process

Pushing a tag matching `v[0-9]+.*` (e.g. `v0.1.0`) triggers `.github/workflows/release.yml`, which cross-compiles `easyenv` for Linux (x86_64/aarch64, gnu/musl), macOS (Intel/Apple Silicon), and Windows (x86_64), and publishes each as a checksummed archive on a new GitHub Release ([`taiki-e/create-gh-release-action`](https://github.com/taiki-e/create-gh-release-action) + [`taiki-e/upload-rust-binary-action`](https://github.com/taiki-e/upload-rust-binary-action)):

```console
$ git tag v0.1.0
$ git push origin v0.1.0
```
