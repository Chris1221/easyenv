pub mod bash;
pub mod zsh;

use crate::diff::Op;
use clap::ValueEnum;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ShellKind {
    Bash,
    Zsh,
}

impl ShellKind {
    pub fn format_ops(&self, ops: &[Op], new_state_token: &str) -> String {
        match self {
            ShellKind::Bash => bash::format_ops(ops, new_state_token),
            ShellKind::Zsh => zsh::format_ops(ops, new_state_token),
        }
    }

    pub fn hook_script(&self) -> &'static str {
        match self {
            ShellKind::Bash => bash::HOOK_SCRIPT,
            ShellKind::Zsh => zsh::HOOK_SCRIPT,
        }
    }

    pub fn init_snippet(&self) -> &'static str {
        match self {
            ShellKind::Bash => "eval \"$(easyenv hook bash)\"",
            ShellKind::Zsh => "eval \"$(easyenv hook zsh)\"",
        }
    }

    pub fn rc_file_hint(&self) -> &'static str {
        match self {
            ShellKind::Bash => "~/.bashrc",
            ShellKind::Zsh => "~/.zshrc",
        }
    }
}

/// Only keys that are valid POSIX shell identifiers are safe to interpolate
/// unquoted into `export KEY=...` / `unset KEY` statements. `.env` keys are
/// allowed to contain `.` (per dotenvy), which is not a legal shell
/// identifier character -- reject anything outside `[A-Za-z_][A-Za-z0-9_]*`
/// defensively rather than emit broken or dangerous shell code.
pub fn is_valid_shell_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Single-quotes a value for safe interpolation into POSIX shell source,
/// escaping embedded single quotes via the standard `'\''` technique.
pub fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Shared POSIX export/unset formatting used by both bash and zsh, since
/// their statement syntax for this purpose is identical. The state token is
/// always re-exported (even when no keys are currently managed) so the
/// fast-path signature check has something to compare against on the next
/// invocation.
pub(crate) fn posix_format_ops(ops: &[Op], new_state_token: &str) -> String {
    let mut out = String::new();
    for op in ops {
        match op {
            Op::Export(k, v) => {
                out.push_str(&format!("export {}={}\n", k, shell_single_quote(v)));
            }
            Op::Unset(k) => {
                out.push_str(&format!("unset {}\n", k));
            }
        }
    }
    out.push_str(&format!(
        "export EASYENV_STATE={}\n",
        shell_single_quote(new_state_token)
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_shell_keys() {
        assert!(is_valid_shell_key("FOO"));
        assert!(is_valid_shell_key("_FOO"));
        assert!(is_valid_shell_key("FOO_BAR_123"));
    }

    #[test]
    fn invalid_shell_keys() {
        assert!(!is_valid_shell_key("FOO.BAR"));
        assert!(!is_valid_shell_key("123FOO"));
        assert!(!is_valid_shell_key(""));
        assert!(!is_valid_shell_key("FOO BAR"));
        assert!(!is_valid_shell_key("FOO;rm -rf /"));
    }

    #[test]
    fn quotes_embedded_single_quotes() {
        assert_eq!(shell_single_quote("it's"), "'it'\\''s'");
    }
}
