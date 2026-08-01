mod cli;
mod config;
mod diff;
mod discover;
mod dotenv;
mod fastpath;
mod merge;
mod shell;
mod state;

use clap::Parser;
use cli::{Cli, Command};
use config::{Config, load_config};
use diff::compute_diff;
use discover::discover_env_files;
use dotenv::parse_file;
use fastpath::compute_signature;
use merge::{EnvLayer, merge_layers, origin_of};
use shell::{ShellKind, is_valid_shell_key};
use state::EasyenvState;
use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::Path;

/// Caps how many denial/warning lines a single invocation prints to
/// stderr, so a hostile or broken `.env` with hundreds of denied keys
/// can't flood the terminal on every `cd`.
struct WarningBudget {
    remaining: u32,
    suppressed: u32,
}

impl WarningBudget {
    fn new(cap: u32) -> Self {
        Self {
            remaining: cap,
            suppressed: 0,
        }
    }

    fn emit(&mut self, msg: impl AsRef<str>) {
        if self.remaining > 0 {
            eprintln!("{}", msg.as_ref());
            self.remaining -= 1;
        } else {
            self.suppressed += 1;
        }
    }

    fn finish(&self) {
        if self.suppressed > 0 {
            eprintln!("easyenv: ... and {} more", self.suppressed);
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let debug = cli.debug || std::env::var_os("EASYENV_DEBUG").is_some();

    match cli.command {
        Command::Init { shell } => run_init(shell),
        Command::Hook { shell } => print!("{}", shell.hook_script()),
        Command::Export { shell } => {
            let result = load_config();
            for w in &result.warnings {
                eprintln!("{w}");
            }
            if let Some(err) = &result.parse_error {
                eprintln!(
                    "easyenv: warning: malformed config at {}: {err} -- \
                     falling back to compiled-in defaults",
                    result.path.display()
                );
            }
            run_export(shell, debug, &result.config, &result.path);
        }
        Command::Status => {
            let result = load_config();
            for w in &result.warnings {
                eprintln!("{w}");
            }
            if let Some(err) = &result.parse_error {
                eprintln!(
                    "easyenv: error: malformed config at {}: {err}",
                    result.path.display()
                );
                std::process::exit(1);
            }
            run_status(&result.config);
        }
    }
}

fn run_init(shell: ShellKind) {
    if std::io::stdout().is_terminal() {
        println!("Add this line to your {}:\n", shell.rc_file_hint());
        println!("    {}\n", shell.init_snippet());
        println!("Then restart your shell (or `source` the rc file).");
    } else {
        println!("{}", shell.init_snippet());
    }
}

fn run_status(config: &Config) {
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("easyenv: failed to read current directory: {e}");
            std::process::exit(1);
        }
    };
    let env_files = discover_env_files(&cwd, config);
    if env_files.is_empty() {
        println!("No .env files active for {}", cwd.display());
        return;
    }
    println!("Active .env files for {} (root-first):", cwd.display());
    let mut layers = Vec::new();
    for path in &env_files {
        let parsed = parse_file(path);
        for w in &parsed.warnings {
            eprintln!("easyenv: warning: {w}");
        }
        println!("  {}", path.display());
        layers.push(EnvLayer {
            source: path.clone(),
            vars: parsed.vars,
        });
    }
    let merged = merge_layers(&layers);
    println!("\nResolved variables:");
    for (k, v) in &merged {
        let origin = origin_of(&layers, k)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // Denied/invalid keys are shown, not silently omitted, so users
        // can debug "why isn't my variable loading" without reading docs.
        let marker = if !is_valid_shell_key(k) {
            " [INVALID: not a valid shell identifier]"
        } else if config.is_denied_key(k) {
            " [DENIED: see docs/reference/security.md]"
        } else {
            ""
        };
        println!("  {k}={v}  (from {origin}){marker}");
    }
}

/// The hot path, invoked by the shell hook on every relevant prompt/cd.
/// Must only ever write shell code (or nothing) to stdout; all diagnostics
/// go to stderr.
fn run_export(shell: ShellKind, debug: bool, config: &Config, config_path: &Path) {
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            println!("# easyenv: failed to read current directory: {e}");
            return;
        }
    };

    let mut prev_state = std::env::var("EASYENV_STATE")
        .ok()
        .and_then(|token| EasyenvState::decode(&token))
        .unwrap_or_else(EasyenvState::empty);

    let env_files = discover_env_files(&cwd, config);
    let signature = compute_signature(&cwd, &env_files, config_path);

    // Dominant case: nothing has changed since the last invocation (same
    // cwd, same .env files and config file, same mtime/size). Skip
    // parse/merge/diff entirely and print nothing.
    if signature == prev_state.signature {
        if debug {
            eprintln!("easyenv: fast path hit, no change");
        }
        return;
    }

    let mut budget = WarningBudget::new(10);

    // A denylist entry added after some EASYENV_STATE was already
    // persisted could otherwise replay a now-denied key through the
    // restore path in compute_diff, unfiltered. Drop (not unset) denied
    // keys found in stale state -- as if easyenv had never touched them.
    prev_state.managed.retain(|k, _| {
        if config.is_denied_key(k) {
            budget.emit(format!(
                "easyenv: warning: dropping denied key {k:?} found in stale state \
                 (see docs/reference/security.md)"
            ));
            false
        } else {
            true
        }
    });

    let mut layers = Vec::new();
    for path in &env_files {
        let parsed = parse_file(path);
        for w in &parsed.warnings {
            eprintln!("easyenv: warning: {w}");
        }
        layers.push(EnvLayer {
            source: path.clone(),
            vars: parsed.vars,
        });
    }
    let mut target = merge_layers(&layers);
    target.retain(|k, _| {
        if !is_valid_shell_key(k) {
            budget.emit(format!(
                "easyenv: warning: skipping key {k:?}, not a valid shell identifier"
            ));
            return false;
        }
        if config.is_denied_key(k) {
            let origin = origin_of(&layers, k)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            budget.emit(format!(
                "easyenv: warning: refusing to set {k:?} from {origin}\n  \
                 This name is denied by default -- see docs/reference/security.md. \
                 If you need it, add it to `allow` in ~/.config/easyenv/config.toml."
            ));
            return false;
        }
        true
    });
    budget.finish();

    let keys_of_interest: HashSet<&str> = target
        .keys()
        .map(String::as_str)
        .chain(prev_state.managed.keys().map(String::as_str))
        .collect();
    let current_shell_snapshot: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| keys_of_interest.contains(k.as_str()))
        .collect();

    let (ops, new_managed) = compute_diff(&prev_state, &current_shell_snapshot, &target);

    // Always persist the new signature (even when nothing is currently
    // managed) so the fast path above can engage on the next invocation --
    // otherwise a directory tree with no `.env` files anywhere would redo
    // discovery's full stat walk on every single cd forever.
    let new_state = EasyenvState {
        managed: new_managed,
        signature,
    };
    let new_token = new_state.encode();

    print!("{}", shell.format_ops(&ops, &new_token));
}
