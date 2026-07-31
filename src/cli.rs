use crate::shell::ShellKind;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "easyenv",
    version,
    about = "Automatic per-directory .env loading"
)]
pub struct Cli {
    /// Emit diagnostics to stderr (never stdout, so eval'd output stays clean).
    #[arg(long, global = true)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Print the one-line snippet to add to your shell's rc file.
    Init { shell: ShellKind },
    /// Print the hook-function-definition script, eval'd once at shell startup.
    Hook { shell: ShellKind },
    /// Hot path: compute and print the shell statements needed for the current directory.
    Export { shell: ShellKind },
    /// Human-readable: which .env files are active for the current directory.
    Status,
}
