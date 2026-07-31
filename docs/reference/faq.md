# FAQ

## My shell feels slow after installing easyenv

It shouldn't. If it does, run with diagnostics on to see what's happening:

```console
$ EASYENV_DEBUG=1 easyenv export bash
```

Also check how many `.env` files exist between your current directory and the filesystem root — an unusually deep chain adds one cheap `stat` per level, but this is normally unnoticeable.

## Variables aren't loading

- Confirm the hook is actually installed: `type _easyenv_hook` (bash) or `which _easyenv_hook` (zsh) should show a function, not "not found."
- Confirm you restarted your shell (or `source`d the rc file) after running `easyenv init`.
- Run `easyenv status` in the directory in question — it tells you exactly which `.env` files it sees and where each resolved key came from, without changing anything.
- Check the key itself is a valid shell identifier (`[A-Za-z_][A-Za-z0-9_]*`). `.env` keys containing characters like `.` are skipped with a warning, since they can't be exported as shell variables.

## Does easyenv touch variables I set myself?

Only if a `.env` file defines the same key. In that case, your original value is remembered and restored the moment you leave every directory that overrides it — easyenv never permanently clobbers anything you set yourself.

## What happens with a malformed `.env` line?

That single line is skipped (with a warning printed to stderr), and every other valid line in that file — and every other `.env` file in the chain — still loads normally. A bad line never crashes your shell or corrupts your last command's exit status.

## Does it expand `$VARIABLES` inside `.env` values?

Not currently. `.env` values are treated as opaque literal strings, matching plain dotenv semantics — `FOO=$BAR` sets `FOO` to the literal text `$BAR`, it doesn't substitute `BAR`'s value. This is a deliberate scope decision, partly because expansion opens ordering questions (what if the variable being referenced is itself being unloaded in the same step?).

## Does it work in fish or PowerShell?

Not yet — bash and zsh are the current focus. Fish and PowerShell support is on the roadmap.

## Why not just use direnv?

direnv doesn't support `.env` files without an explicit `.envrc` that calls `dotenv`, and it requires an explicit `direnv allow` trust step per directory. easyenv is `.env`-native and prompt-free by design — see the [home page](../index.md) for the fuller comparison.
