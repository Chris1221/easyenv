# CLI reference

```
easyenv [--debug] <COMMAND>
```

| Flag | Effect |
|---|---|
| `--debug` | Emit diagnostics to stderr (e.g. which fast-path branch was taken). Never affects stdout, so it's safe to use even though `hook`/`export` output gets `eval`'d. |

`EASYENV_DEBUG=1` in the environment has the same effect as `--debug`, useful when the flag can't easily be threaded through the shell hook.

## `easyenv init <shell>`

Prints the one-line snippet to add to your shell's rc file, along with the rc file's conventional path, when run in a terminal:

```console
$ easyenv init bash
Add this line to your ~/.bashrc:

    eval "$(easyenv hook bash)"

Then restart your shell (or `source` the rc file).
```

When stdout isn't a terminal (e.g. piped to a file or another script), it prints just the snippet — nothing else — so it's safe to script against.

`<shell>` is `bash` or `zsh`.

## `easyenv hook <shell>`

Prints the shell-specific hook function definition. This is what the `init` snippet actually `eval`s once, at shell startup — it registers the function that fires on every prompt (bash's `PROMPT_COMMAND`) or on every `cd` plus every prompt (zsh's `chpwd_functions` + `precmd_functions`). You won't normally run this yourself.

## `easyenv export <shell>`

The hot path: computes and prints the `export`/`unset` statements needed to bring the shell's environment in line with the current directory's merged `.env` files, relative to what easyenv previously set. This is what the hook function calls on every prompt. Prints nothing at all when nothing has changed since the last invocation.

You generally don't call this directly either — it's meant to be `eval`'d by the hook, not read by a human. Use `status` for that.

## `easyenv status`

Human-readable: which `.env` files are active for the current directory (root-first) and which file each resolved variable came from. Doesn't change your environment.

```console
$ easyenv status
Active .env files for /home/you/project (root-first):
  /home/you/.env
  /home/you/project/.env

Resolved variables:
  API_KEY=xyz  (from /home/you/project/.env)
  SHARED=abc  (from /home/you/.env)
```

If nothing is active:

```console
$ easyenv status
No .env files active for /home/you/somewhere/else
```
