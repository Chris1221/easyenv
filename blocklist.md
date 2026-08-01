# easyenv — default blocklists and config file

Supersedes **P0-1's escape hatch** and **all of P1-1** in `easyenv-security-review.md`.
The rest of that document (P0-2, P1-2, P1-3, P1-4, P2-*) stands unchanged.

Design in one line: **the baseline blocklists are compiled into the binary and are
non-empty; the config file can only add to them or name specific entries to remove.**
A blocklist that is empty until the user writes a config protects nobody, because
nobody writes a security config they don't know they need.

---

## 1. Config file

**Location.** `$XDG_CONFIG_HOME/easyenv/config.toml`, falling back to
`~/.config/easyenv/config.toml`. Resolve this **once at process start, from the
process environment only.** `XDG_CONFIG_HOME` and `HOME` are both on the denied-names
list below precisely so a `.env` cannot relocate the file that governs it. Never
honour a config path supplied by a `.env`, and do not add a `--config` flag that the
shell hook could be induced to pass.

Missing file is not an error — it means "defaults only".

**Schema.**

```toml
[env]
# Added to the compiled-in denied set.
deny_extra = ["MY_INTERNAL_THING", "COMPANY_*"]

# Removed from the compiled-in denied set. Reintroduces the underlying risk
# for exactly these names; everything else stays denied.
allow = ["AWS_ENDPOINT_URL", "NODE_OPTIONS"]

[dirs]
# Added to the compiled-in skip set.
skip_extra = ["~/Sync", "/net"]

# Removed from the compiled-in skip set.
unskip = ["~/Downloads"]
```

**Resolution order:** compiled-in defaults → apply `deny_extra` / `skip_extra` →
apply `allow` / `unskip`. `allow` and `unskip` win, so a user can always get back to
current behaviour if they mean to.

**Failure handling.** A malformed config is a hard error on `easyenv status` and a
loud stderr warning on `easyenv export` — but `export` must then fall back to
**compiled-in defaults**, never to "no restrictions". Fail closed. Unknown keys in
the TOML get a warning, not an error, so config files survive version skew.

**Wildcards.** Support a trailing `*` only (`COMPANY_*`), matched as a prefix. No
regex, no glob library. Anchor everything — a substring match would deny `MY_PATH`
along with `PATH`, which is the obvious way to get this wrong.

---

## 2. Denied variable names — compiled-in default

Matching rules:

- Exact names are matched **case-sensitively** except where the table says otherwise.
- Prefix entries end in `_` and are matched from position 0 only.
- Proxy variables and `npm_config_*` must be matched **case-insensitively**, because
  the consuming tools honour both cases.

### 2.1 Prefixes

| Prefix | Why |
| --- | --- |
| `LD_` | Dynamic loader. `LD_PRELOAD`, `LD_AUDIT`, `LD_LIBRARY_PATH` — arbitrary code in every child process. |
| `DYLD_` | macOS equivalent. SIP blunts it for system binaries, not for yours. |
| `BASH_FUNC_` | Function import from the environment; the Shellshock shape. |
| `EASYENV_` | Prevents a `.env` forging state or flipping easyenv's own settings. |
| `XDG_` | Redirects config/data/cache lookup for a very large number of tools — including easyenv's own config file. |
| `OPENSSL_` | `OPENSSL_CONF` loads engines/providers; that is code execution. |
| `GIT_CONFIG_` | `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_COUNT`/`KEY_n`/`VALUE_n` — injects git config, including `core.pager` and `core.sshCommand`. |
| `NPM_CONFIG_` *(ci)* | Registry, prefix, userconfig, `script-shell`. Supply-chain and exec. |
| `CARGO_TARGET_` | `CARGO_TARGET_<TRIPLE>_RUNNER` executes an arbitrary command on `cargo run`/`test`. |
| `CARGO_REGISTRIES_` | Redirects crate sources. |
| `PERL5` | `PERL5OPT`, `PERL5LIB`, `PERL5DB`. |

### 2.2 Shell execution and parsing

`PROMPT_COMMAND`, `PS0`, `PS1`, `PS2`, `PS3`, `PS4`, `PROMPT`, `RPROMPT`, `RPS1`,
`RPS2`, `BASH_ENV`, `ENV`, `SHELLOPTS`, `BASHOPTS`, `ZDOTDIR`, `FPATH`, `CDPATH`,
`IFS`, `GLOBIGNORE`, `BASH_XTRACEFD`, `HISTFILE`, `HISTIGNORE`, `HISTCONTROL`,
`HISTTIMEFORMAT`

Notes for the implementer:

- `PS1` matters because bash performs command substitution in prompt strings by
  default (`promptvars` is on). zsh only does so under `PROMPT_SUBST`, but deny both.
- `ZDOTDIR` is the sharpest one for zsh users: it relocates where **new shells** read
  `.zshrc` from. A `.env` setting it owns every terminal you open afterwards.
- `FPATH` controls zsh autoload resolution — arbitrary function bodies.
- `SHELLOPTS`/`BASHOPTS` are readonly in a running bash, so emitting an `export` for
  them also produces a shell error inside the hook's `eval` on every prompt. Denying
  them fixes a cosmetic bug as well as a security one.
- `HISTFILE` is integrity/exfiltration rather than execution — it can redirect your
  shell history into a file inside the repository, which then gets committed.

### 2.3 Loader, locale, and process environment

`PATH`, `HOME`, `SHELL`, `TMPDIR`, `TMP`, `TEMP`, `LOCPATH`, `NLSPATH`,
`GCONV_PATH`, `TERMINFO`, `TERMINFO_DIRS`, `TERMCAP`, `MANPATH`

`HOME` deserves special mention — redirecting it means every tool that reads
`~/.gitconfig`, `~/.netrc`, `~/.aws/credentials`, or `~/.ssh/config` reads from a
directory the attacker controls. It is one of the highest-value names on this list
and one of the least obvious.

`LOCPATH`, `NLSPATH`, and `GCONV_PATH` are glibc module-loading paths — genuine
code-execution vectors with a long history.

### 2.4 Tool invocation

`PAGER`, `MANPAGER`, `EDITOR`, `VISUAL`, `BROWSER`, `LESS`, `LESSOPEN`, `LESSCLOSE`,
`LESSSECURE`, `MORE`, `SUDO_ASKPASS`, `SUDO_EDITOR`, `SSH_ASKPASS`, `SSH_AUTH_SOCK`,
`SSH_AGENT_PID`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_EXTERNAL_DIFF`, `GIT_PAGER`,
`GIT_EDITOR`, `GIT_ASKPASS`, `GIT_PROXY_COMMAND`, `GIT_CONFIG`, `GIT_DIR`,
`GIT_WORK_TREE`, `GIT_TEMPLATE_DIR`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`,
`GIT_OBJECT_DIRECTORY`, `GIT_NAMESPACE`

Deliberately **not** denied: `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`,
`GIT_COMMITTER_NAME`, `GIT_COMMITTER_EMAIL`. These are inert and per-project
identity is a legitimate `.env` use. This is why the spec denies specific `GIT_*`
names rather than the whole prefix.

### 2.5 TLS and network interception

*(proxy names matched case-insensitively — deny both `http_proxy` and `HTTP_PROXY`)*

`http_proxy`, `https_proxy`, `ftp_proxy`, `all_proxy`, `no_proxy`, `SSL_CERT_FILE`,
`SSL_CERT_DIR`, `CURL_CA_BUNDLE`, `CURL_HOME`, `REQUESTS_CA_BUNDLE`,
`NODE_EXTRA_CA_CERTS`, `NODE_TLS_REJECT_UNAUTHORIZED`, `PYTHONHTTPSVERIFY`,
`GIT_SSL_CAINFO`, `GIT_SSL_NO_VERIFY`, `SSLKEYLOGFILE`

`SSLKEYLOGFILE` is worth including: it makes any TLS library that honours it write
session keys to a path the attacker chose, which decrypts your traffic after the
fact.

### 2.6 Language runtimes

**Python:** `PYTHONPATH`, `PYTHONHOME`, `PYTHONSTARTUP`, `PYTHONEXECUTABLE`,
`PYTHONBREAKPOINT`, `PYTHONINSPECT`, `PYTHONUSERBASE`, `PYTHONWARNINGS`,
`PIP_INDEX_URL`, `PIP_EXTRA_INDEX_URL`, `PIP_TRUSTED_HOST`, `PIP_CONFIG_FILE`,
`PIP_TARGET`, `UV_INDEX_URL`, `UV_EXTRA_INDEX_URL`, `UV_PYTHON`, `UV_CONFIG_FILE`,
`CONDA_ENVS_PATH`, `CONDARC`

`PYTHONBREAKPOINT` is the sleeper here — it names an arbitrary importable callable
invoked by `breakpoint()`. `PYTHONWARNINGS` can also reach arbitrary imports.

Deliberately **not** denied: `PYTHONUNBUFFERED`, `PYTHONDONTWRITEBYTECODE`. Inert
and extremely common in dev `.env` files.

**Node/JS:** `NODE_OPTIONS`, `NODE_PATH`, `NODE_REPL_EXTERNAL_MODULE`,
`BUN_INSTALL`, `BUN_CONFIG_REGISTRY`, `DENO_DIR`, `DENO_INSTALL_ROOT`

**Ruby:** `RUBYOPT`, `RUBYLIB`, `RUBYPATH`, `GEM_HOME`, `GEM_PATH`, `BUNDLE_GEMFILE`,
`BUNDLE_PATH`

**JVM:** `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS`, `JDK_JAVA_OPTIONS`, `JAVA_HOME`,
`CLASSPATH`

**Rust:** `RUSTC`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `CARGO_HOME`,
`RUSTUP_HOME`, `RUSTUP_TOOLCHAIN`

**Go:** `GOPROXY`, `GOSUMDB`, `GONOSUMDB`, `GOPRIVATE`, `GOFLAGS`, `GOPATH`,
`GOROOT`, `GOTOOLCHAIN`

`GOTOOLCHAIN` can cause the go command to download and execute a different toolchain.

**R:** `R_HOME`, `R_PROFILE`, `R_PROFILE_USER`, `R_ENVIRON`, `R_ENVIRON_USER`,
`R_LIBS`, `R_LIBS_USER`, `R_LIBS_SITE`

`R_PROFILE_USER` sources arbitrary R at startup. Given your day job this one will
matter to your users more than most lists would suggest.

**Other:** `LUA_PATH`, `LUA_CPATH`, `PHPRC`, `PHP_INI_SCAN_DIR`, `JULIA_LOAD_PATH`,
`JULIA_DEPOT_PATH`

### 2.7 Cloud and infrastructure

`AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE`, `AWS_CA_BUNDLE`,
`AWS_EC2_METADATA_SERVICE_ENDPOINT`, `AWS_METADATA_SERVICE_ENDPOINT`,
`AWS_ENDPOINT_URL`, `KUBECONFIG`, `DOCKER_HOST`, `DOCKER_CONFIG`,
`DOCKER_CERT_PATH`, `DOCKER_TLS_VERIFY`, `CONTAINER_HOST`, `CLOUDSDK_CONFIG`,
`AZURE_CONFIG_DIR`, `VAULT_ADDR`, `VAULT_CACERT`, `TF_CLI_CONFIG_FILE`,
`HELM_REPOSITORY_CONFIG`

Deliberately **not** denied: `AWS_PROFILE`, `AWS_REGION`,
`GOOGLE_APPLICATION_CREDENTIALS`, `GOOGLE_CLOUD_PROJECT`. These select among
resources the user already controls rather than redirecting to attacker
infrastructure, and all four are overwhelmingly common in legitimate dev `.env`
files. Denying them would generate enough noise that people would disable the
feature — which is the real failure mode for a security default.

### 2.8 The one genuine conflict: `AWS_ENDPOINT_URL`

This is denied above, and it is also the single most common legitimate `.env` entry
for anyone using LocalStack or MinIO. The risk is real — pointing the SDK at an
attacker host means your credentials are signed and sent there — but so is the
friction.

Handle it in the warning text rather than by weakening the default:

```
easyenv: warning: refusing to set "AWS_ENDPOINT_URL" from /path/to/.env
  This can redirect signed AWS requests to another host. If you use
  LocalStack or MinIO, add it to `allow` in ~/.config/easyenv/config.toml.
```

Every denial warning should name the specific config key that would permit it. A
denial the user can't act on is a denial they'll work around badly.

---

## 3. Skipped directories — compiled-in default

Matched as **path prefixes against the canonicalized path**, after `~` expansion.
A `.env` in a skipped directory is not collected, but **the walk continues upward**
past it.

### 3.1 Defaults

| Path | Why |
| --- | --- |
| `/tmp`, `/var/tmp`, `/dev/shm` | World-writable. Any local user can plant a `.env` that applies to everyone who works beneath it. |
| `/private/tmp`, `/private/var/tmp` | **Required on macOS.** `/tmp` is a symlink to `/private/tmp`, and `discover_env_files` canonicalizes, so the path you match against will be the `/private/` form. Omitting these silently disables the `/tmp` rule on every Mac. |
| `/` (exact match only, not prefix) | A `.env` at the filesystem root applies to literally everything. Never intentional. |
| `/etc`, `/usr`, `/opt`, `/srv`, `/var` | System directories. A `.env` here is either accidental or planted. |
| `/proc`, `/sys`, `/dev` | Pseudo-filesystems; nothing valid to read. |
| `/mnt`, `/media`, `/Volumes` | Removable and mounted media — USB sticks, mounted DMGs, network shares. Classic delivery vector. |
| `~/Downloads`, `~/Desktop` | Where extracted archives land. "Unzip it and look inside" is the most likely way a user meets a hostile `.env`. |

### 3.2 Suggested but not default

Document these in the security page as one-line additions rather than shipping them
on, since each has legitimate users:

- `~/Dropbox`, `~/Google Drive`, `~/OneDrive`, `~/Library/Mobile Documents` —
  writable by anyone you've shared a folder with, and sync means a change lands
  without you doing anything.
- `/net`, `/Network` — automounted NFS. Also worth skipping for latency reasons; the
  upward stat walk over NFS is where your benchmark numbers go to die.

### 3.3 Vendored directories

Separate from the path-prefix list, and worth its own compiled-in rule: skip any
candidate whose path contains a component named `node_modules`, `.venv`, `venv`,
`site-packages`, `vendor`, `.tox`, or `target`.

Rationale: a `.env` inside a downloaded dependency is never something the user wrote,
and `cd node_modules/some-package` is a thing people do while debugging. Implement as
a component-name check on the path, not a substring match — `my-vendor-tools` must
not trigger it.

---

## 4. Warning ergonomics

Every denial prints to stderr on the non-fast-path invocation, which means once per
`cd` into the directory, not once per prompt. Two constraints:

- **Cap it.** Ten warnings per invocation, then `… and N more`. A hostile or
  broken `.env` with 500 denied keys must not flood the terminal.
- **Name the source file and the remedy**, per the `AWS_ENDPOINT_URL` example above.

`easyenv status` should show denied keys inline rather than omitting them, so users
can debug "why isn't my variable loading" without reading the docs:

```
  DATABASE_URL=postgres://...  (from /home/u/api/.env)
  PATH=/opt/bin                (from /home/u/api/.env) [DENIED: shell PATH]
```

---

## 5. Tests

Beyond the shell-integration tests already specified in P0-1:

1. **Anchoring.** `MY_PATH`, `PATHOLOGY`, `LDAP_URL`, `HOMEBREW_PREFIX`,
   `NODE_ENV`, `GIT_AUTHOR_NAME`, `PYTHONUNBUFFERED`, `AWS_REGION` must all be
   **permitted**. This is the test that catches a sloppy `contains()` or an
   unanchored prefix check, and it should fail loudly if someone widens a prefix
   later.
2. **Case sensitivity.** `http_proxy` and `HTTP_PROXY` both denied; `Path` and
   `path` not denied (they are not `PATH` and mean nothing to POSIX shells).
3. **Config round-trip.** `allow = ["PATH"]` permits `PATH`; `deny_extra` denies a
   custom name; `unskip = ["~/Downloads"]` restores loading there.
4. **Config cannot be relocated by a `.env`.** A `.env` setting `XDG_CONFIG_HOME`
   or `HOME` is denied, and the config path resolved at startup is unaffected.
5. **Malformed config fails closed.** Broken TOML → warning → compiled-in defaults
   still enforced. Assert that a `.env` setting `PROMPT_COMMAND` is still denied
   under a broken config. This is the single most important test in the file.
6. **macOS canonicalization.** `/tmp/x/.env` is skipped on macOS, where the
   canonical path is `/private/tmp/x`. Gate with `#[cfg(target_os = "macos")]`.
7. **Vendored components.** `a/node_modules/b/.env` skipped; `a/my-vendor-tools/.env`
   not skipped.

---

## 6. Note on completeness

This list will rot. New runtimes keep inventing new magic variables — `GOTOOLCHAIN`,
`GIT_CONFIG_COUNT`, and the `UV_*` family are all recent. Put the policy in
`docs/reference/security.md` next to the list so additions are decidable rather than
ad hoc:

> Deny any name where a value the user did not review can cause code to run, a
> lookup path to be redirected, or a trust boundary to be relaxed — in the shell,
> the dynamic loader, or any commonly-installed language runtime or CLI. Permit
> names that only select among resources the user already controls. When in doubt,
> deny; `allow` in the config file is the escape hatch.

And be explicit in the docs that this is a blocklist, with the honest caveat that
blocklists are never complete. The claim easyenv can defensibly make is "a hostile
`.env` cannot execute code through any known mechanism", not "cannot do anything" —
the former is true and checkable, the latter would be a promise you can't keep.