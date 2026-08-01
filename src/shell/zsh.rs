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
///
/// `__EASYENV_BIN__` is substituted with the shell-quoted absolute path to
/// this binary -- see `bash::hook_script` for why this is a plain string
/// substitution rather than a `format!` template.
const HOOK_SCRIPT_TEMPLATE: &str = r#"_easyenv_hook() {
  eval "$(__EASYENV_BIN__ export zsh)"
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

pub fn hook_script(easyenv_invocation: &str) -> String {
    HOOK_SCRIPT_TEMPLATE.replace("__EASYENV_BIN__", easyenv_invocation)
}
