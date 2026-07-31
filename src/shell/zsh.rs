use super::posix_format_ops;
use crate::diff::Op;

pub fn format_ops(ops: &[Op], new_state_token: &str) -> String {
    posix_format_ops(ops, new_state_token)
}

/// Registered in both `chpwd_functions` (fires immediately on `cd`) and
/// `precmd_functions` (fires on every prompt). The `precmd_functions`
/// registration is what makes a shell that starts up already inside a
/// `.env` directory load on its very first prompt, since `chpwd_functions`
/// alone does not fire on shell init.
pub const HOOK_SCRIPT: &str = r#"_easyenv_hook() {
  eval "$(easyenv export zsh)"
}
typeset -ag chpwd_functions
if [[ -z "${chpwd_functions[(r)_easyenv_hook]}" ]]; then
  chpwd_functions+=(_easyenv_hook)
fi
typeset -ag precmd_functions
if [[ -z "${precmd_functions[(r)_easyenv_hook]}" ]]; then
  precmd_functions+=(_easyenv_hook)
fi
"#;
