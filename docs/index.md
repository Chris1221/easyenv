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

## Why not direnv or autoenv?

- **direnv** doesn't support `.env` files out of the box — it wants a `.envrc` that explicitly calls `dotenv`, plus a trust/allow step per directory.
- **autoenv** loads variables on `cd` in, but doesn't unload them on `cd` out without extra configuration, so variables leak between projects.

easyenv does both halves of the job automatically, with zero configuration and zero prompts.

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
