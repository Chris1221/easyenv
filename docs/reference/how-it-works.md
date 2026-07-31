# How it works

A standalone binary can't intercept `cd` — there's no OS hook for "the shell's working directory changed." The only real mechanism (the same one direnv, zoxide, and starship use) is **shell integration**: a small function registered in your shell that runs automatically and `eval`s whatever the binary prints.

## The hook

`easyenv init <shell>` gives you one line to add to your rc file:

```bash
eval "$(easyenv hook bash)"
```

This runs once, at shell startup, and registers a function:

- **bash**: added to `PROMPT_COMMAND`, which fires before every prompt is drawn — including the very first one, so a shell that starts up already inside a `.env` directory loads immediately.
- **zsh**: added to *both* `chpwd_functions` (fires immediately on `cd`) and `precmd_functions` (fires on every prompt). Both registrations matter: `chpwd_functions` alone wouldn't fire on shell startup, so `precmd_functions` is what makes "already inside a `.env` dir at shell start" work in zsh too.

Neither hook is gated on "did the directory actually change" — a `.env` file can be edited while you're sitting still in its directory, and that has to be picked up on the next prompt too. So the hook runs on every prompt, unconditionally, and relies on the binary itself being fast enough that this doesn't matter (see below).

## What `easyenv export <shell>` actually does

This is the subcommand the hook calls and `eval`s. Each run:

1. **Discovers** every `.env` from the current directory up to the filesystem root.
2. Computes a **fast-path signature** — a hash of the current directory plus each candidate `.env` file's modification time and size (not its contents). If this matches the signature from the last invocation, *nothing else happens* — no parsing, no diffing, no output. This is the dominant case (sitting at a prompt, nothing changed), and it's what keeps the "run unconditionally on every prompt" design cheap.
3. If the signature differs, it **parses** each `.env` file (via the battle-tested `dotenvy` parser) and **merges** them root-to-leaf, so closer directories override farther ones on conflicting keys while non-conflicting keys from every layer survive.
4. It **diffs** the merged result against what easyenv previously set (see below) and prints the minimal `export`/`unset` statements needed — nothing if there's no change to apply.

## Remembering what to undo

There's no daemon and no background process — easyenv is a plain short-lived binary invoked over and over by your shell. So it has to remember what it previously changed *inside the shell's own environment*, via a single variable: `EASYENV_STATE`.

For every variable easyenv currently manages, it tracks one thing: **the value that variable had before easyenv ever touched it** (or "it didn't exist"). That value is captured exactly once and carried forward unchanged across however many directories you move through — it's only restored (or unset, if it never existed) once the variable drops out of every `.env` in the chain. This single rule is what makes deeply nested overrides work correctly: if a parent sets `FOO=1` and a child overrides it to `FOO=2`, leaving the child restores `1` — not `2`, and not a blank unset — because `1` is what was true before easyenv started managing `FOO` at all.

`EASYENV_STATE` is encoded compactly (a small versioned, length-prefixed, base64'd token) so it survives round-tripping through a shell variable regardless of what characters your `.env` values contain.

## Why this is fast

The two goals in tension are "run on every single prompt, unconditionally" and "never introduce perceptible lag." The design leans on one fact: spawning the `easyenv` process is the dominant fixed cost regardless of what it does once running, and that cost is unavoidable in a no-daemon design. So the internal fast path (a handful of `stat` calls plus one hash comparison) is optimized to be negligible on top of that spawn — not to avoid spawning in the first place, which would require shell-side logic that can't correctly detect "I need to unload" without redoing the same directory walk anyway.
