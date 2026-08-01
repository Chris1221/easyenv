use super::posix_format_ops;
use crate::diff::Op;

pub fn format_ops(ops: &[Op], new_state_token: &str) -> String {
    posix_format_ops(ops, new_state_token)
}

/// Registered on `PROMPT_COMMAND`, which fires before every prompt is drawn
/// (including the very first one at shell startup, so a shell that starts
/// up already inside a `.env` directory loads immediately).
///
/// Deliberately NOT gated on `$PWD` changing: a `.env` file can be edited
/// while sitting still in its own directory (no `cd` involved), and that
/// edit must be picked up on the very next prompt. A `$PWD` gate would skip
/// the hook entirely in that case. The binary's own fast-path signature
/// check (comparing cwd + every candidate `.env`'s mtime/size) is what
/// keeps the unconditional call cheap -- confirmed cheap enough in practice
/// since zsh's `precmd_functions` already fires unconditionally on every
/// prompt this same way. `$?` is saved/restored so a nonzero exit from
/// `easyenv` never leaks into the user's actual last-command exit status.
///
/// `__EASYENV_BIN__` is substituted with the shell-quoted absolute path to
/// this binary (see `ShellKind::resolve_easyenv_invocation`) so that a
/// `.env` setting `PATH` cannot hijack the very next hook invocation --
/// this is a plain string substitution rather than a `format!` template
/// because the script is full of shell `${...}` syntax that would need
/// brace-escaping under `format!`.
const HOOK_SCRIPT_TEMPLATE: &str = r#"_easyenv_hook() {
  local previous_exit_status=$?
  eval "$(__EASYENV_BIN__ export bash)"
  return $previous_exit_status
}
if [[ ";${PROMPT_COMMAND:-};" != *";_easyenv_hook;"* ]]; then
  PROMPT_COMMAND="_easyenv_hook${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
"#;

pub fn hook_script(easyenv_invocation: &str) -> String {
    HOOK_SCRIPT_TEMPLATE.replace("__EASYENV_BIN__", easyenv_invocation)
}
