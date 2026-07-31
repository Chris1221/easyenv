# Benchmarks: cost of directory nesting

One of easyenv's two design goals is speed, specifically that the "on `cd`" mechanism should stay fast no matter how deep a directory tree gets. This page measures that directly against [direnv](https://direnv.net/) and [autoenv](https://github.com/hyperupcall/autoenv).

![Cold-load latency vs. directory nesting depth: easyenv stays under 6ms out to 128 levels, direnv grows to ~78ms, autoenv grows to over a second](../assets/benchmark-nesting.png)

At shallow nesting the three tools are within a similar order of magnitude. As nesting depth grows, easyenv stays essentially flat (a few milliseconds, dominated by process spawn rather than the directory walk), direnv grows noticeably but sub-linearly, and autoenv's latency grows roughly linearly with depth, reaching **over a second at 128 levels of nesting**.

## What's actually measured

For each tool, at each nesting depth, a fresh chain of directories is built (each level contributing that tool's config file), and the **cold load** — the first time that exact directory is visited, nothing cached — is timed:

- **easyenv**: `easyenv export bash`, run with `EASYENV_STATE` unset, cwd set to the deepest directory. This is exactly what the shell hook `eval`s on every prompt.
- **direnv**: `direnv export bash`, run the same way. This is direnv's own equivalent hot-path subcommand — its `PROMPT_COMMAND` hook calls it directly, same as easyenv's.
- **autoenv**: the actual `cd` builtin override autoenv installs (`autoenv_cd` → `autoenv_init`), timed from immediately before the `cd` into the deepest directory to immediately after — bash startup and sourcing `activate.sh` happen before the timer starts, so only the mechanism itself is measured.

Each depth is repeated (15 trials by default); the plot shows the median with error bars spanning min–max.

## Why this is a fair-but-partial comparison

This deliberately measures **cold** loads only, for a well-defined reason: it's the one scenario all three tools handle in a directly comparable way. It is *not* the full picture:

- **easyenv and direnv both have a warm fast path** — a signature/hash check that skips re-reading files entirely when nothing has changed, which is the common case in real usage (sitting at a prompt, nothing edited). Neither this benchmark nor the plot shows that faster warm-path number; both tools are faster in practice than this chart suggests for repeat visits to the same directory.
- **autoenv has no equivalent fast path** — every `cd` re-walks and re-sources from scratch, which is a real, structural difference, not a benchmark artifact. Its `autoenv_init` also shells out to external commands (`df`, `awk`, `sed`) once per ancestor directory, which is the main driver of its steep growth curve.
- direnv's higher flat baseline compared to easyenv (even at depth 0) reflects that `.envrc` is evaluated as an actual bash script, not parsed as static `KEY=value` pairs — a more general but more expensive mechanism.

## Reproducing this

```console
$ cargo build --release
$ git clone https://github.com/hyperupcall/autoenv.git /tmp/autoenv
$ AUTOENV_DIR=/tmp/autoenv ./benches/nesting_benchmark.sh
$ pip install -r requirements-benches.txt
$ python3 benches/plot_nesting_benchmark.py
```

This regenerates `benches/results.csv` and `docs/assets/benchmark-nesting.png`. Requires `direnv` on `PATH` (`apt install direnv`, `brew install direnv`, etc.).

!!! note "CI-generated numbers carry more noise than a dedicated machine"
    This benchmark also runs in CI (see `.github/workflows/benchmark.yml`) so the chart stays current automatically. Shared CI runners are noisier than a dedicated machine — treat the exact numbers as indicative of relative scaling behavior between tools, not as precise absolute latencies. Run it locally for stable numbers.
