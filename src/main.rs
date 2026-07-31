mod cli;
mod diff;
mod discover;
mod dotenv;
mod fastpath;
mod merge;
mod shell;
mod state;

use clap::Parser;
use cli::{Cli, Command};
use diff::compute_diff;
use discover::discover_env_files;
use dotenv::parse_file;
use fastpath::compute_signature;
use merge::{EnvLayer, merge_layers};
use shell::{ShellKind, is_valid_shell_key};
use state::EasyenvState;
use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;

fn main() {
    let cli = Cli::parse();
    let debug = cli.debug || std::env::var_os("EASYENV_DEBUG").is_some();

    match cli.command {
        Command::Init { shell } => run_init(shell),
        Command::Hook { shell } => print!("{}", shell.hook_script()),
        Command::Export { shell } => run_export(shell, debug),
        Command::Status => run_status(),
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

fn run_status() {
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("easyenv: failed to read current directory: {e}");
            std::process::exit(1);
        }
    };
    let env_files = discover_env_files(&cwd);
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
        let origin = layers
            .iter()
            .rev()
            .find(|l| l.vars.contains_key(k))
            .map(|l| l.source.display().to_string())
            .unwrap_or_default();
        println!("  {k}={v}  (from {origin})");
    }
}

/// The hot path, invoked by the shell hook on every relevant prompt/cd.
/// Must only ever write shell code (or nothing) to stdout; all diagnostics
/// go to stderr.
fn run_export(shell: ShellKind, debug: bool) {
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            println!("# easyenv: failed to read current directory: {e}");
            return;
        }
    };

    let prev_state = std::env::var("EASYENV_STATE")
        .ok()
        .and_then(|token| EasyenvState::decode(&token))
        .unwrap_or_else(EasyenvState::empty);

    let env_files = discover_env_files(&cwd);
    let signature = compute_signature(&cwd, &env_files);

    // Dominant case: nothing has changed since the last invocation (same
    // cwd, same .env files with the same mtime/size). Skip parse/merge/diff
    // entirely and print nothing.
    if signature == prev_state.signature {
        if debug {
            eprintln!("easyenv: fast path hit, no change");
        }
        return;
    }

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
        if is_valid_shell_key(k) {
            true
        } else {
            eprintln!("easyenv: warning: skipping key {k:?}, not a valid shell identifier");
            false
        }
    });

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
