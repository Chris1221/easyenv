# easyenv

**easyenv** loads a directory's `.env` file into your shell the moment you `cd` into it, and unloads it the moment you `cd` back out. That's the whole feature.

```console
$ cd ~/projects/api
$ echo $DATABASE_URL
postgres://localhost/api_dev

$ cd ~/projects/frontend
$ echo $DATABASE_URL

```

No `.envrc`, no `direnv allow`, no per-directory trust prompts, no leftover variables from the last project you were in. You install it once and forget it's there.

```console
$ curl -fsSL https://raw.githubusercontent.com/Chris1221/easyenv/main/install.sh | bash
```

Detects your platform, downloads and checksum-verifies the right release binary, installs it to `~/.local/bin` (no `sudo`), and asks before touching your shell's rc file. Full details: [Installation](getting-started/installation.md).

## It stays fast, even deep in a directory tree

![Cold-load latency vs. directory nesting depth: easyenv stays under 6ms out to 128 levels, direnv grows to ~78ms, autoenv grows to over a second](assets/benchmark-nesting.png)

That's cold-load latency — the time from `cd` to variables being set — as nesting gets deeper. easyenv stays at a few milliseconds out to 128 nested `.env` files. direnv grows noticeably. autoenv, which has no caching and shells out to external commands per ancestor directory, passes **a full second** at 128 levels. Full methodology and how to reproduce it: [Benchmarks](reference/benchmarks.md).

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

## How it merges nested directories

If a parent directory also has a `.env`, easyenv loads that too — the child directory's values win on conflicting keys, but everything else from the parent is still there:

```console
$ cat ~/projects/.env
SHARED_TOKEN=abc123

$ cat ~/projects/api/.env
DATABASE_URL=postgres://localhost/api_dev

$ cd ~/projects/api
$ echo $SHARED_TOKEN $DATABASE_URL
abc123 postgres://localhost/api_dev
```

Leaving `api/` and returning to `projects/` restores `DATABASE_URL` to whatever it was before (unset, in this case) while keeping `SHARED_TOKEN` loaded.

## Where to go next

- [Installation](getting-started/installation.md) — build the binary and wire up your shell
- [Quickstart](getting-started/quickstart.md) — five-minute walkthrough
- [Tutorials](tutorials/nested-directories.md) — nested `.env` precedence and live edits
- [How it works](reference/how-it-works.md) — the shell-hook + diff/state design under the hood
