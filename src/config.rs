//! The denylist/skip-list engine described in `blocklist.md`. The baseline
//! lists are compiled into the binary and are non-empty; the config file
//! can only add to them or name specific entries to remove. A blocklist
//! that is empty until the user writes a config protects nobody, because
//! nobody writes a security config they don't know they need.
//!
//! Policy (see `docs/reference/security.md` for the user-facing version):
//! deny any name where a value the user did not review can cause code to
//! run, a lookup path to be redirected, or a trust boundary to be relaxed
//! -- in the shell, the dynamic loader, or any commonly-installed language
//! runtime or CLI. Permit names that only select among resources the user
//! already controls. When in doubt, deny; `allow` in the config file is
//! the escape hatch.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A single deny/allow rule: either an exact name or a trailing-`*`
/// prefix. Anchored -- `Prefix` matches from position 0 only, never a
/// substring, so a sloppy rule can never widen into `MY_PATH`/`PATHOLOGY`
/// matching `PATH`/`LD_`.
#[derive(Debug, Clone)]
enum NameRule {
    Exact(String),
    Prefix(String), // stored without the trailing '*'
}

impl NameRule {
    fn parse(raw: &str) -> Self {
        match raw.strip_suffix('*') {
            Some(prefix) => NameRule::Prefix(prefix.to_string()),
            None => NameRule::Exact(raw.to_string()),
        }
    }

    fn matches(&self, key: &str) -> bool {
        match self {
            NameRule::Exact(n) => n == key,
            NameRule::Prefix(p) => key.starts_with(p.as_str()),
        }
    }
}

/// A directory-skip rule, matched as a path prefix against the
/// canonicalized path (except the one exact-only `/` case).
#[derive(Debug, Clone)]
enum DirRule {
    Exact(PathBuf),
    Prefix(PathBuf),
}

impl DirRule {
    fn matches(&self, dir: &Path) -> bool {
        match self {
            DirRule::Exact(p) => dir == p,
            DirRule::Prefix(p) => dir.starts_with(p),
        }
    }
}

/// Expands a leading `~` or `~/...` against `$HOME`. Left as-is (not
/// matched against anything meaningful) if `$HOME` is unset.
fn expand_tilde(path: &str) -> PathBuf {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return PathBuf::from(path);
    };
    if path == "~" {
        return home;
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(path)
}

pub struct Config {
    deny_extra: Vec<NameRule>,
    allow: Vec<NameRule>,
    skip_dirs: Vec<DirRule>,
    unskip_dirs: Vec<DirRule>,
}

impl Config {
    /// Compiled-in defaults only, no user config overlay. Used for the
    /// malformed-config fallback and by tests that want a known-fixed
    /// baseline.
    pub fn defaults() -> Self {
        Self {
            deny_extra: Vec::new(),
            allow: Vec::new(),
            skip_dirs: compiled_skip_dirs(),
            unskip_dirs: Vec::new(),
        }
    }

    /// No restrictions at all -- not even the compiled-in skip list. For
    /// tests in *other* modules that exercise discovery/diff mechanics
    /// and whose fixtures typically live under `/tmp` (via `tempfile`),
    /// which the real defaults deliberately skip; using `defaults()`
    /// there would make those tests fail for an unrelated reason.
    #[cfg(test)]
    pub fn unrestricted() -> Self {
        Self {
            deny_extra: Vec::new(),
            allow: Vec::new(),
            skip_dirs: Vec::new(),
            unskip_dirs: Vec::new(),
        }
    }

    /// Like `unrestricted`, but with an explicit ad-hoc skip list -- for
    /// tests that want to verify skip-list *behavior* in isolation
    /// without going through actual TOML file I/O (that round-trip is
    /// already covered by this module's own config-loading tests).
    #[cfg(test)]
    pub fn unrestricted_with_skip(paths: &[PathBuf]) -> Self {
        Self {
            deny_extra: Vec::new(),
            allow: Vec::new(),
            skip_dirs: paths.iter().cloned().map(DirRule::Prefix).collect(),
            unskip_dirs: Vec::new(),
        }
    }

    fn from_raw(raw: RawConfig) -> Self {
        let deny_extra = raw
            .env
            .deny_extra
            .iter()
            .map(|s| NameRule::parse(s))
            .collect();
        let allow = raw.env.allow.iter().map(|s| NameRule::parse(s)).collect();
        let mut skip_dirs = compiled_skip_dirs();
        skip_dirs.extend(
            raw.dirs
                .skip_extra
                .iter()
                .map(|s| DirRule::Prefix(expand_tilde(s))),
        );
        let unskip_dirs = raw
            .dirs
            .unskip
            .iter()
            .map(|s| DirRule::Prefix(expand_tilde(s)))
            .collect();
        Self {
            deny_extra,
            allow,
            skip_dirs,
            unskip_dirs,
        }
    }

    /// `allow` always wins over both the compiled-in defaults and
    /// `deny_extra`, so a user can always get back to unrestricted
    /// behavior for a specific name if they mean to.
    pub fn is_denied_key(&self, key: &str) -> bool {
        if self.allow.iter().any(|r| r.matches(key)) {
            return false;
        }
        is_denied_by_compiled_defaults(key) || self.deny_extra.iter().any(|r| r.matches(key))
    }

    /// `unskip` always wins over both the compiled-in defaults and
    /// `skip_extra`.
    pub fn is_skipped_dir(&self, dir: &Path) -> bool {
        if self.unskip_dirs.iter().any(|r| r.matches(dir)) {
            return false;
        }
        self.skip_dirs.iter().any(|r| r.matches(dir))
    }
}

/// Not user-configurable (blocklist.md specifies no escape hatch for this
/// one): a `.env` inside a downloaded dependency is never something the
/// user wrote. Matched as a path *component*, not a substring, so
/// `my-vendor-tools` does not match.
pub fn has_vendored_component(dir: &Path) -> bool {
    const VENDORED: &[&str] = &[
        "node_modules",
        ".venv",
        "venv",
        "site-packages",
        "vendor",
        ".tox",
        "target",
    ];
    dir.components().any(|c| match c {
        std::path::Component::Normal(name) => {
            VENDORED.iter().any(|v| name == std::ffi::OsStr::new(*v))
        }
        _ => false,
    })
}

// --- blocklist.md §3.1: compiled-in skipped directories ---------------

const SKIP_PREFIXES: &[&str] = &[
    "/tmp",
    "/var/tmp",
    "/dev/shm",
    // Required on macOS: /tmp is a symlink to /private/tmp, and
    // discover_env_files canonicalizes, so the path actually matched
    // against is the /private/ form. Omitting these silently disables
    // the /tmp rule on every Mac.
    "/private/tmp",
    "/private/var/tmp",
    "/etc",
    "/usr",
    "/opt",
    "/srv",
    "/var",
    "/proc",
    "/sys",
    "/dev",
    "/mnt",
    "/media",
    "/Volumes",
];

fn compiled_skip_dirs() -> Vec<DirRule> {
    let mut dirs: Vec<DirRule> = SKIP_PREFIXES
        .iter()
        .map(|p| DirRule::Prefix(PathBuf::from(p)))
        .collect();
    // Exact match only -- a .env at the filesystem root applies to
    // literally everything and is never intentional.
    dirs.push(DirRule::Exact(PathBuf::from("/")));
    for rel in ["~/Downloads", "~/Desktop"] {
        dirs.push(DirRule::Prefix(expand_tilde(rel)));
    }
    dirs
}

// --- blocklist.md §2: compiled-in denied variable names ----------------
//
// Organized to mirror the doc's own section numbers so future additions
// are easy to locate against the source of truth.

// §2.1 prefixes, case-sensitive, matched from position 0 only.
const DENY_PREFIXES: &[&str] = &[
    "LD_",               // dynamic loader: LD_PRELOAD, LD_AUDIT, LD_LIBRARY_PATH
    "DYLD_",             // macOS equivalent
    "BASH_FUNC_",        // function import from the environment (Shellshock shape)
    "EASYENV_",          // prevents a .env forging state or flipping easyenv's own settings
    "XDG_", // redirects config/data/cache lookup for many tools, incl. easyenv's own config
    "OPENSSL_", // OPENSSL_CONF loads engines/providers: code execution
    "GIT_CONFIG_", // GIT_CONFIG_GLOBAL, GIT_CONFIG_COUNT/KEY_n/VALUE_n: injects git config
    "CARGO_TARGET_", // CARGO_TARGET_<TRIPLE>_RUNNER executes an arbitrary command
    "CARGO_REGISTRIES_", // redirects crate sources
    "PERL5", // PERL5OPT, PERL5LIB, PERL5DB
];

// §2.1, matched case-insensitively (the consuming tools honour both cases).
const DENY_PREFIXES_CI: &[&str] = &["NPM_CONFIG_"];

// §2.2 shell execution and parsing, §2.3 loader/locale/process env,
// §2.4 tool invocation, §2.5 TLS/network (non-proxy names),
// §2.6 language runtimes, §2.7 cloud/infrastructure.
const DENY_EXACT: &[&str] = &[
    // §2.2
    "PROMPT_COMMAND",
    "PS0",
    "PS1",
    "PS2",
    "PS3",
    "PS4",
    "PROMPT",
    "RPROMPT",
    "RPS1",
    "RPS2",
    "BASH_ENV",
    "ENV",
    "SHELLOPTS",
    "BASHOPTS",
    "ZDOTDIR",
    "FPATH",
    "CDPATH",
    "IFS",
    "GLOBIGNORE",
    "BASH_XTRACEFD",
    "HISTFILE",
    "HISTIGNORE",
    "HISTCONTROL",
    "HISTTIMEFORMAT",
    // §2.3
    "PATH",
    "HOME",
    "SHELL",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LOCPATH",
    "NLSPATH",
    "GCONV_PATH",
    "TERMINFO",
    "TERMINFO_DIRS",
    "TERMCAP",
    "MANPATH",
    // §2.4
    "PAGER",
    "MANPAGER",
    "EDITOR",
    "VISUAL",
    "BROWSER",
    "LESS",
    "LESSOPEN",
    "LESSCLOSE",
    "LESSSECURE",
    "MORE",
    "SUDO_ASKPASS",
    "SUDO_EDITOR",
    "SSH_ASKPASS",
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "GIT_EDITOR",
    "GIT_ASKPASS",
    "GIT_PROXY_COMMAND",
    "GIT_CONFIG",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_TEMPLATE_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_OBJECT_DIRECTORY",
    "GIT_NAMESPACE",
    // §2.5 (non-proxy; proxy names are in DENY_EXACT_CI below)
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "CURL_CA_BUNDLE",
    "CURL_HOME",
    "REQUESTS_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "NODE_TLS_REJECT_UNAUTHORIZED",
    "PYTHONHTTPSVERIFY",
    "GIT_SSL_CAINFO",
    "GIT_SSL_NO_VERIFY",
    "SSLKEYLOGFILE",
    // §2.6 Python
    "PYTHONPATH",
    "PYTHONHOME",
    "PYTHONSTARTUP",
    "PYTHONEXECUTABLE",
    "PYTHONBREAKPOINT",
    "PYTHONINSPECT",
    "PYTHONUSERBASE",
    "PYTHONWARNINGS",
    "PIP_INDEX_URL",
    "PIP_EXTRA_INDEX_URL",
    "PIP_TRUSTED_HOST",
    "PIP_CONFIG_FILE",
    "PIP_TARGET",
    "UV_INDEX_URL",
    "UV_EXTRA_INDEX_URL",
    "UV_PYTHON",
    "UV_CONFIG_FILE",
    "CONDA_ENVS_PATH",
    "CONDARC",
    // §2.6 Node/JS
    "NODE_OPTIONS",
    "NODE_PATH",
    "NODE_REPL_EXTERNAL_MODULE",
    "BUN_INSTALL",
    "BUN_CONFIG_REGISTRY",
    "DENO_DIR",
    "DENO_INSTALL_ROOT",
    // §2.6 Ruby
    "RUBYOPT",
    "RUBYLIB",
    "RUBYPATH",
    "GEM_HOME",
    "GEM_PATH",
    "BUNDLE_GEMFILE",
    "BUNDLE_PATH",
    // §2.6 JVM
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "JDK_JAVA_OPTIONS",
    "JAVA_HOME",
    "CLASSPATH",
    // §2.6 Rust
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    // §2.6 Go
    "GOPROXY",
    "GOSUMDB",
    "GONOSUMDB",
    "GOPRIVATE",
    "GOFLAGS",
    "GOPATH",
    "GOROOT",
    "GOTOOLCHAIN",
    // §2.6 R
    "R_HOME",
    "R_PROFILE",
    "R_PROFILE_USER",
    "R_ENVIRON",
    "R_ENVIRON_USER",
    "R_LIBS",
    "R_LIBS_USER",
    "R_LIBS_SITE",
    // §2.6 other
    "LUA_PATH",
    "LUA_CPATH",
    "PHPRC",
    "PHP_INI_SCAN_DIR",
    "JULIA_LOAD_PATH",
    "JULIA_DEPOT_PATH",
    // §2.7 cloud/infrastructure
    "AWS_CONFIG_FILE",
    "AWS_SHARED_CREDENTIALS_FILE",
    "AWS_CA_BUNDLE",
    "AWS_EC2_METADATA_SERVICE_ENDPOINT",
    "AWS_METADATA_SERVICE_ENDPOINT",
    "AWS_ENDPOINT_URL",
    "KUBECONFIG",
    "DOCKER_HOST",
    "DOCKER_CONFIG",
    "DOCKER_CERT_PATH",
    "DOCKER_TLS_VERIFY",
    "CONTAINER_HOST",
    "CLOUDSDK_CONFIG",
    "AZURE_CONFIG_DIR",
    "VAULT_ADDR",
    "VAULT_CACERT",
    "TF_CLI_CONFIG_FILE",
    "HELM_REPOSITORY_CONFIG",
];

// §2.5 proxy variables -- matched case-insensitively, since the consuming
// tools honour both `http_proxy` and `HTTP_PROXY`.
const DENY_EXACT_CI: &[&str] = &[
    "http_proxy",
    "https_proxy",
    "ftp_proxy",
    "all_proxy",
    "no_proxy",
];

fn is_denied_by_compiled_defaults(key: &str) -> bool {
    if DENY_PREFIXES.iter().any(|p| key.starts_with(p)) {
        return true;
    }
    if DENY_PREFIXES_CI
        .iter()
        .any(|p| key.len() >= p.len() && key[..p.len()].eq_ignore_ascii_case(p))
    {
        return true;
    }
    if DENY_EXACT.contains(&key) {
        return true;
    }
    DENY_EXACT_CI.iter().any(|n| key.eq_ignore_ascii_case(n))
}

// --- TOML config file loading -------------------------------------------

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    env: RawEnvSection,
    #[serde(default)]
    dirs: RawDirsSection,
    #[serde(flatten)]
    extra: toml::Table,
}

#[derive(Debug, Deserialize, Default)]
struct RawEnvSection {
    #[serde(default)]
    deny_extra: Vec<String>,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(flatten)]
    extra: toml::Table,
}

#[derive(Debug, Deserialize, Default)]
struct RawDirsSection {
    #[serde(default)]
    skip_extra: Vec<String>,
    #[serde(default)]
    unskip: Vec<String>,
    #[serde(flatten)]
    extra: toml::Table,
}

pub struct ConfigLoadResult {
    pub config: Config,
    /// Unknown-key warnings; never populated when `parse_error` is set
    /// (an unparseable file has no keys to report on).
    pub warnings: Vec<String>,
    /// Set when the file existed but failed to parse. `config` is then
    /// the compiled-in-only fallback: callers decide whether that's a
    /// hard error (`status`) or a warn-and-continue (`export`), but
    /// either way it is never "no restrictions."
    pub parse_error: Option<String>,
    pub path: PathBuf,
}

/// Resolved once, from the process environment only -- this must never
/// be influenced by a `.env`, since that would let a `.env` relocate the
/// file that governs it. `XDG_CONFIG_HOME`/`HOME` are also on the denied
/// variable list above for the same reason.
pub fn resolve_config_path() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("easyenv").join("config.toml");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".config").join("easyenv").join("config.toml")
}

/// Testable entry point: tests pass a fixture path directly rather than
/// mutating process-wide environment variables, which would be unsound
/// across tests run in parallel in one binary.
pub fn load_config_from(path: &Path) -> ConfigLoadResult {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            // Missing file is not an error -- it means defaults only.
            return ConfigLoadResult {
                config: Config::defaults(),
                warnings: Vec::new(),
                parse_error: None,
                path: path.to_path_buf(),
            };
        }
    };

    match toml::from_str::<RawConfig>(&contents) {
        Ok(raw) => {
            let mut warnings = Vec::new();
            for key in raw.extra.keys() {
                warnings.push(format!(
                    "easyenv: warning: unknown config key {key:?} in {}",
                    path.display()
                ));
            }
            for key in raw.env.extra.keys() {
                warnings.push(format!(
                    "easyenv: warning: unknown config key \"env.{key}\" in {}",
                    path.display()
                ));
            }
            for key in raw.dirs.extra.keys() {
                warnings.push(format!(
                    "easyenv: warning: unknown config key \"dirs.{key}\" in {}",
                    path.display()
                ));
            }
            ConfigLoadResult {
                config: Config::from_raw(raw),
                warnings,
                parse_error: None,
                path: path.to_path_buf(),
            }
        }
        Err(e) => ConfigLoadResult {
            config: Config::defaults(),
            warnings: Vec::new(),
            parse_error: Some(e.to_string()),
            path: path.to_path_buf(),
        },
    }
}

pub fn load_config() -> ConfigLoadResult {
    load_config_from(&resolve_config_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Config {
        Config::defaults()
    }

    #[test]
    fn permits_common_legitimate_names() {
        let c = defaults();
        for key in [
            "MY_PATH",
            "PATHOLOGY",
            "LDAP_URL",
            "HOMEBREW_PREFIX",
            "NODE_ENV",
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
            "PYTHONUNBUFFERED",
            "PYTHONDONTWRITEBYTECODE",
            "AWS_REGION",
            "AWS_PROFILE",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GOOGLE_CLOUD_PROJECT",
            "DATABASE_URL",
        ] {
            assert!(!c.is_denied_key(key), "{key} should be permitted");
        }
    }

    #[test]
    fn denies_known_dangerous_exact_names() {
        let c = defaults();
        for key in [
            "PROMPT_COMMAND",
            "PS1",
            "BASH_ENV",
            "PATH",
            "IFS",
            "AWS_ENDPOINT_URL",
        ] {
            assert!(c.is_denied_key(key), "{key} should be denied");
        }
    }

    #[test]
    fn denies_by_anchored_prefix_not_substring() {
        let c = defaults();
        assert!(c.is_denied_key("LD_PRELOAD"));
        assert!(c.is_denied_key("DYLD_INSERT_LIBRARIES"));
        assert!(c.is_denied_key("EASYENV_STATE"));
        assert!(c.is_denied_key("EASYENV_ANYTHING"));
        assert!(c.is_denied_key("PERL5OPT"));
        // Anchoring: these must NOT match despite containing a denied
        // prefix/substring elsewhere in the name.
        assert!(!c.is_denied_key("MY_LD_THING"));
        assert!(!c.is_denied_key("NOT_EASYENV_RELATED"));
    }

    #[test]
    fn case_insensitive_matches() {
        let c = defaults();
        assert!(c.is_denied_key("http_proxy"));
        assert!(c.is_denied_key("HTTP_PROXY"));
        assert!(c.is_denied_key("npm_config_registry"));
        assert!(c.is_denied_key("NPM_CONFIG_REGISTRY"));
        // Not PATH, not case-insensitive collateral damage.
        assert!(!c.is_denied_key("Path"));
        assert!(!c.is_denied_key("path"));
    }

    #[test]
    fn config_round_trip_allow_and_deny_extra() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[env]\ndeny_extra = [\"MY_INTERNAL_THING\", \"COMPANY_*\"]\nallow = [\"PATH\"]\n",
        )
        .unwrap();

        let result = load_config_from(&path);
        assert!(result.parse_error.is_none());
        assert!(
            !result.config.is_denied_key("PATH"),
            "allow should permit PATH"
        );
        assert!(result.config.is_denied_key("MY_INTERNAL_THING"));
        assert!(result.config.is_denied_key("COMPANY_SECRET"));
        assert!(!result.config.is_denied_key("COMPANYFOO")); // no separating char, still a prefix match actually
    }

    #[test]
    fn config_round_trip_skip_extra_and_unskip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[dirs]\nskip_extra = [\"/custom/skip\"]\nunskip = [\"~/Downloads\"]\n",
        )
        .unwrap();

        let result = load_config_from(&path);
        assert!(result.parse_error.is_none());
        assert!(result.config.is_skipped_dir(Path::new("/custom/skip/sub")));
        assert!(!result.config.is_skipped_dir(&expand_tilde("~/Downloads")));
    }

    #[test]
    fn missing_config_file_yields_defaults_no_error() {
        let result = load_config_from(Path::new("/nonexistent/easyenv/config.toml"));
        assert!(result.parse_error.is_none());
        assert!(result.config.is_denied_key("PROMPT_COMMAND"));
    }

    #[test]
    fn malformed_config_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not [valid toml").unwrap();

        let result = load_config_from(&path);
        assert!(result.parse_error.is_some());
        // The single most important property: even with a broken config,
        // the compiled-in denylist still applies. Never "no restrictions."
        assert!(result.config.is_denied_key("PROMPT_COMMAND"));
    }

    #[test]
    fn unknown_keys_warn_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "future_top_level_key = true\n[env]\nfuture_env_key = true\ndeny_extra = [\"FOO\"]\n",
        )
        .unwrap();

        let result = load_config_from(&path);
        assert!(result.parse_error.is_none());
        assert!(!result.warnings.is_empty());
        assert!(result.config.is_denied_key("FOO"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_tmp_canonicalizes_to_private() {
        let c = defaults();
        let canonical = std::fs::canonicalize("/tmp").unwrap();
        assert!(c.is_skipped_dir(&canonical.join("x")));
    }

    #[test]
    fn vendored_components_are_anchored_to_path_segments() {
        assert!(has_vendored_component(Path::new("/a/node_modules/b")));
        assert!(has_vendored_component(Path::new("/a/.venv/b")));
        assert!(has_vendored_component(Path::new("/a/target/b")));
        assert!(!has_vendored_component(Path::new("/a/my-vendor-tools/b")));
        assert!(!has_vendored_component(Path::new("/a/targeting/b")));
    }

    #[test]
    fn compiled_skip_dirs_cover_documented_defaults() {
        let c = defaults();
        assert!(c.is_skipped_dir(Path::new("/tmp/x")));
        assert!(c.is_skipped_dir(Path::new("/var/tmp/x")));
        assert!(c.is_skipped_dir(Path::new("/private/tmp/x")));
        assert!(c.is_skipped_dir(Path::new("/")));
        assert!(c.is_skipped_dir(Path::new("/etc")));
        assert!(c.is_skipped_dir(Path::new("/mnt/x")));
        assert!(!c.is_skipped_dir(Path::new("/home/user/project")));
    }
}
