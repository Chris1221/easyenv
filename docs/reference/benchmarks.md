# Benchmarks: cost of directory nesting

One of easyenv's two design goals is speed, specifically that the "on `cd`" mechanism should stay fast no matter how deep a directory tree gets. This page measures that directly against five other tools that solve some version of the same problem: [direnv](https://direnv.net/), [autoenv](https://github.com/hyperupcall/autoenv), [shadowenv](https://github.com/Shopify/shadowenv) (Shopify), [mise](https://mise.jdx.dev/) (jdx), and [zsh-autoenv](https://github.com/Tarrasch/zsh-autoenv) (Tarrasch).

![Cold-load latency vs. directory nesting depth: easyenv and shadowenv stay fast and flat out to 128 levels; direnv grows moderately; autoenv, mise, and zsh-autoenv all grow sharply, with mise passing 2 seconds at depth 128](../assets/benchmark-nesting.png)

At shallow nesting most tools are within a similar order of magnitude. As nesting depth grows, easyenv and **shadowenv** (also written in Rust, also a diff/reversal-based design) stay fast and nearly flat. direnv grows moderately but sub-linearly. autoenv, **mise**, and **zsh-autoenv** all grow sharply with depth — at 128 levels, mise takes **over 2 seconds**, autoenv **~1.4–2 seconds**, and zsh-autoenv **~0.8 seconds**, while easyenv stays under 6ms.

## What's actually measured

For each tool, at each nesting depth, a fresh chain of directories is built (each level contributing that tool's config file), and the **cold load** — the first time that exact directory is visited, nothing cached — is timed:

- **easyenv**: `easyenv export bash`, run with `EASYENV_STATE` unset, cwd set to the deepest directory. This is exactly what the shell hook `eval`s on every prompt.
- **direnv**: `direnv export bash`, run the same way. This is direnv's own equivalent hot-path subcommand — its `PROMPT_COMMAND` hook calls it directly, same as easyenv's.
- **autoenv**: the actual `cd` builtin override autoenv installs (`autoenv_cd` → `autoenv_init`), timed from immediately before the `cd` into the deepest directory to immediately after — bash startup and sourcing `activate.sh` happen before the timer starts, so only the mechanism itself is measured.
- **shadowenv**: `shadowenv hook --shellpid $$`, its own equivalent hot-path subcommand (confirmed by inspecting `shadowenv init bash`'s actual output).
- **mise**: `mise hook-env -s bash`, its own equivalent hot-path subcommand (confirmed the same way, and matches mise's own docs, which discuss profiling this exact call via `MISE_TIMINGS`).
- **zsh-autoenv**: like autoenv, this lives entirely inside the shell (it overrides zsh's `chpwd` hook, no separate binary) — timed the same way, isolating just the `cd` after the plugin is sourced.

Each depth is repeated (15 trials by default); the plot shows the median with error bars spanning min–max.

## Two tools needed extra setup to make nesting work at all

easyenv, direnv, mise, and (plain) autoenv all discover ancestor config files automatically just by walking up the directory tree. Two of the newly added tools don't, by design:

- **shadowenv** requires an explicit `.shadowenv.d/parent` symlink in every directory, pointing at its parent's `.shadowenv.d`, to opt into inheriting anything from above.
- **zsh-autoenv** requires every `.autoenv.zsh` file (except the topmost) to explicitly call `autoenv_source_parent` at the top, or it doesn't inherit anything from its parent either.

The benchmark's fixture builders set both of these up at every level so the *merged result* is still correct and comparable — but this is a real, user-facing difference worth knowing about if you're choosing between these tools, not just a benchmarking footnote.

## Why this is a fair-but-partial comparison

This deliberately measures **cold** loads only, for a well-defined reason: it's the one scenario every tool handles in a directly comparable way. It is *not* the full picture:

- **easyenv, direnv, and mise all have a warm fast path** — a signature/hash/trust-cache check that skips re-reading files entirely when nothing has changed, which is the common case in real usage (sitting at a prompt, nothing edited). None of them show that faster warm-path number here; all three are faster in practice than this chart suggests for repeat visits to the same directory.
- **autoenv and zsh-autoenv have no equivalent fast path** — every `cd` re-walks and re-sources from scratch. autoenv's `autoenv_init` also shells out to external commands (`df`, `awk`, `sed`) once per ancestor directory, which is the main driver of its steep growth curve.
- **mise is a much broader tool** than the others here — a version manager and task runner as well as an env-var manager (successor to asdf + direnv combined). This benchmark isolates just its env-loading path (`hook-env`); the rest of its functionality isn't exercised or represented in this number.
- direnv's higher flat baseline compared to easyenv (even at depth 0) reflects that `.envrc` is evaluated as an actual bash script, not parsed as static `KEY=value` pairs — a more general but more expensive mechanism.
- Not every tool in this space is even benchmarkable on this axis: [quickenv](https://codeberg.org/untitaker/quickenv) takes a completely different approach (generating shim binaries on `PATH` instead of a shell hook), so it never loads variables into the shell at all — there's no comparable "time from `cd` to vars set" number to measure.

## Reproducing this

```console
$ cargo build --release
$ git clone https://github.com/hyperupcall/autoenv.git /tmp/autoenv
$ git clone https://github.com/Tarrasch/zsh-autoenv.git /tmp/zsh-autoenv
$ curl -fsSL https://mise.run | sh   # or: brew install mise
$ curl -fsSL -o /tmp/shadowenv https://github.com/Shopify/shadowenv/releases/latest/download/shadowenv-x86_64-unknown-linux-gnu && chmod +x /tmp/shadowenv

$ AUTOENV_DIR=/tmp/autoenv ZSH_AUTOENV_DIR=/tmp/zsh-autoenv SHADOWENV_BIN=/tmp/shadowenv \
  ./benches/nesting_benchmark.sh
$ pip install -r requirements-benches.txt
$ python3 benches/plot_nesting_benchmark.py
```

This regenerates `benches/results.csv` and `docs/assets/benchmark-nesting.png`. Requires `direnv` and `zsh` on `PATH` too (`apt install direnv zsh`, `brew install direnv zsh`, etc.); `mise`/`shadowenv` binary locations are overridable via `MISE_BIN`/`SHADOWENV_BIN` if not on `PATH`.

!!! note "CI-generated numbers carry more noise than a dedicated machine"
    This benchmark also runs in CI (see `.github/workflows/benchmark.yml`) so the chart stays current automatically. Shared CI runners are noisier than a dedicated machine — treat the exact numbers as indicative of relative scaling behavior between tools, not as precise absolute latencies. Run it locally for stable numbers.
