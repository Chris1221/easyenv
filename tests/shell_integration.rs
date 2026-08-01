//! End-to-end tests that actually launch interactive `bash`/`zsh` with
//! easyenv's real hook installed, drive a scripted command sequence via
//! stdin, and assert on what the shell echoed back. These exercise the
//! full shell-hook -> `easyenv export` -> `eval` loop, not just the diff
//! engine in isolation.
//!
//! Fixtures use `scratch_dir()` (a tempdir under `<repo>/test-scratch`)
//! rather than the default `TempDir::new()` (which lands under `/tmp`):
//! the compiled-in security defaults deliberately skip `/tmp`
//! (world-writable, shared machines), so a fixture placed there would be
//! silently ignored by the real binary under test -- the same thing a
//! real user hitting this default would see, just not what we want for
//! testing unrelated behavior. `CARGO_TARGET_TMPDIR` (`<repo>/target/tmp`)
//! doesn't work either, for the same reason: `target` is itself one of
//! the compiled-in vendored-directory names.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn easyenv_bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_easyenv"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// A fresh tempdir outside both `/tmp` and any vendored-component name --
/// see the module-level doc comment for why plain `TempDir::new()` or
/// `CARGO_TARGET_TMPDIR` don't work now that the security defaults skip
/// both.
fn scratch_dir() -> TempDir {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-scratch");
    std::fs::create_dir_all(&base).unwrap();
    TempDir::new_in(&base).unwrap()
}

fn write_env(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

/// Runs `script` as stdin to an interactive `bash` or `zsh` with easyenv's
/// hook installed, starting in `start_dir`, and returns everything the
/// shell wrote to stdout. `shell` is `"bash"` or `"zsh"`.
fn run_in_shell(shell: &str, start_dir: &Path, script: &str) -> String {
    let bin_dir = easyenv_bin_dir();
    let rc_dir = scratch_dir();

    let mut cmd = match shell {
        "bash" => {
            let rc_path = rc_dir.path().join("bashrc");
            write_env(
                &rc_path,
                &format!(
                    "export PATH=\"{}:$PATH\"\neval \"$(easyenv hook bash)\"\n",
                    bin_dir.display()
                ),
            );
            let mut cmd = Command::new("bash");
            cmd.arg("--noprofile")
                .arg("--rcfile")
                .arg(&rc_path)
                .arg("-i");
            cmd
        }
        "zsh" => {
            write_env(
                &rc_dir.path().join(".zshrc"),
                &format!(
                    "export PATH=\"{}:$PATH\"\neval \"$(easyenv hook zsh)\"\n",
                    bin_dir.display()
                ),
            );
            let mut cmd = Command::new("zsh");
            cmd.env("ZDOTDIR", rc_dir.path()).arg("-i");
            cmd
        }
        other => panic!("unsupported shell in test harness: {other}"),
    };

    cmd.current_dir(start_dir)
        .env_remove("EASYENV_STATE")
        .env_remove("EASYENV_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {shell}: {e}"));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Registers one `#[test] fn bash()` and one `#[test] fn zsh()` in a module
/// named `$test_name`, both calling the shared scenario function `$scenario`.
macro_rules! for_each_shell {
    ($test_name:ident, $scenario:ident) => {
        mod $test_name {
            use super::*;

            #[test]
            fn bash() {
                $scenario("bash");
            }

            #[test]
            fn zsh() {
                $scenario("zsh");
            }
        }
    };
}

fn scenario_override_inherit_restore_unset(shell: &str) {
    let tmp = scratch_dir();
    let parent = tmp.path().join("parent");
    let child = parent.join("child");
    let sibling = tmp.path().join("sibling");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    write_env(&parent.join(".env"), "FOO=parent_value\nSHARED=parent\n");
    write_env(&child.join(".env"), "FOO=child_value\n");

    let script = format!(
        "echo \"M1 FOO=$FOO SHARED=$SHARED\"\n\
         cd child\n\
         echo \"M2 FOO=$FOO SHARED=$SHARED\"\n\
         cd ..\n\
         echo \"M3 FOO=$FOO SHARED=$SHARED\"\n\
         cd {sibling}\n\
         echo \"M4 FOO=[$FOO] SHARED=[$SHARED]\"\n\
         exit\n",
        sibling = sibling.display(),
    );

    let out = run_in_shell(shell, &parent, &script);

    assert!(
        out.contains("M1 FOO=parent_value SHARED=parent"),
        "parent dir should load its own .env; got:\n{out}"
    );
    assert!(
        out.contains("M2 FOO=child_value SHARED=parent"),
        "child dir should override FOO but inherit SHARED; got:\n{out}"
    );
    assert!(
        out.contains("M3 FOO=parent_value SHARED=parent"),
        "cd'ing back out should restore parent's FOO, not leave child's value; got:\n{out}"
    );
    assert!(
        out.contains("M4 FOO=[] SHARED=[]"),
        "a directory with no relevant .env should have both vars fully unset; got:\n{out}"
    );
}
for_each_shell!(
    override_inherit_restore_unset,
    scenario_override_inherit_restore_unset
);

fn scenario_shell_starts_already_inside_env_dir(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("already_here");
    std::fs::create_dir_all(&dir).unwrap();
    write_env(&dir.join(".env"), "FOO=loaded_on_startup\n");

    // No `cd` at all in the script -- the shell's cwd is set at spawn time.
    let script = "echo \"M1 FOO=$FOO\"\nexit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 FOO=loaded_on_startup"),
        "starting a shell already inside a .env directory must load on the \
         very first prompt with no cd required; got:\n{out}"
    );
}
for_each_shell!(
    shell_starts_already_inside_env_dir,
    scenario_shell_starts_already_inside_env_dir
);

fn scenario_editing_env_live_is_picked_up_without_cd(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_env(&dir.join(".env"), "FOO=before\n");

    // A short sleep guards against coarse filesystem mtime resolution; not
    // needed on ext4 (nanosecond resolution) but keeps the test robust
    // across filesystems/CI environments.
    let script = format!(
        "echo \"M1 FOO=$FOO\"\n\
         sleep 1\n\
         echo 'FOO=after' > {env_path}\n\
         echo \"M2 FOO=$FOO\"\n\
         exit\n",
        env_path = dir.join(".env").display(),
    );

    let out = run_in_shell(shell, &dir, &script);

    assert!(
        out.contains("M1 FOO=before"),
        "initial load failed; got:\n{out}"
    );
    assert!(
        out.contains("M2 FOO=after"),
        "editing .env while sitting in its directory (no cd) must be picked \
         up on the next prompt; got:\n{out}"
    );
}
for_each_shell!(
    editing_env_live_is_picked_up_without_cd,
    scenario_editing_env_live_is_picked_up_without_cd
);

fn scenario_malformed_env_does_not_crash_shell(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("malformed");
    std::fs::create_dir_all(&dir).unwrap();
    write_env(
        &dir.join(".env"),
        "GOOD=1\nthis is not valid\nALSO_GOOD=2\n",
    );

    let script = "echo \"M1 GOOD=$GOOD ALSO_GOOD=$ALSO_GOOD STATUS=$?\"\nexit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 GOOD=1 ALSO_GOOD=2 STATUS=0"),
        "a malformed line must not block sibling keys in the same file or \
         corrupt the shell's exit status; got:\n{out}"
    );
}
for_each_shell!(
    malformed_env_does_not_crash_shell,
    scenario_malformed_env_does_not_crash_shell
);

#[cfg(unix)]
fn scenario_symlinked_directory_resolves_real_target(shell: &str) {
    let tmp = scratch_dir();
    let real = tmp.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    write_env(&real.join(".env"), "FOO=via_symlink\n");
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let script = "echo \"M1 FOO=$FOO\"\nexit\n";
    let out = run_in_shell(shell, &link, script);

    assert!(
        out.contains("M1 FOO=via_symlink"),
        "a symlinked working directory should resolve to the real target's \
         .env; got:\n{out}"
    );
}
#[cfg(unix)]
for_each_shell!(
    symlinked_directory_resolves_real_target,
    scenario_symlinked_directory_resolves_real_target
);

fn scenario_exit_status_of_failing_command_is_preserved(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_env(&dir.join(".env"), "FOO=1\n");

    let script = "false\necho \"M1 STATUS=$?\"\nexit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 STATUS=1"),
        "the hook must never clobber the exit status of the user's last \
         command; got:\n{out}"
    );
}
for_each_shell!(
    exit_status_of_failing_command_is_preserved,
    scenario_exit_status_of_failing_command_is_preserved
);

#[cfg(unix)]
fn write_fake_easyenv_shim(dir: &Path) {
    let shim = dir.join("easyenv");
    std::fs::write(&shim, "#!/bin/sh\necho FAKE_EASYENV_SHIM_RAN\n").unwrap();
    let mut perms = std::fs::metadata(&shim).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&shim, perms).unwrap();
}

/// Like `run_in_shell`, except the rc file's hook-fetch line uses the
/// absolute path to the real binary directly (simulating a properly
/// installed hook, e.g. via `install.sh` or `easyenv init`'s corrected
/// output), and `$PATH` is set to *only* `shim_dir` -- the real binary's
/// directory is deliberately absent from `PATH` entirely, so any
/// PATH-resolved lookup of `easyenv` can only ever find the fake shim.
#[cfg(unix)]
fn run_in_shell_with_hijacked_path(
    shell: &str,
    start_dir: &Path,
    script: &str,
    shim_dir: &Path,
) -> String {
    let real_bin = easyenv_bin_dir().join("easyenv");
    let rc_dir = scratch_dir();

    let mut cmd = match shell {
        "bash" => {
            let rc_path = rc_dir.path().join("bashrc");
            write_env(
                &rc_path,
                &format!(
                    "export PATH=\"{}\"\neval \"$('{}' hook bash)\"\n",
                    shim_dir.display(),
                    real_bin.display()
                ),
            );
            let mut cmd = Command::new("bash");
            cmd.arg("--noprofile")
                .arg("--rcfile")
                .arg(&rc_path)
                .arg("-i");
            cmd
        }
        "zsh" => {
            write_env(
                &rc_dir.path().join(".zshrc"),
                &format!(
                    "export PATH=\"{}\"\neval \"$('{}' hook zsh)\"\n",
                    shim_dir.display(),
                    real_bin.display()
                ),
            );
            let mut cmd = Command::new("zsh");
            cmd.env("ZDOTDIR", rc_dir.path()).arg("-i");
            cmd
        }
        other => panic!("unsupported shell in test harness: {other}"),
    };

    cmd.current_dir(start_dir)
        .env_remove("EASYENV_STATE")
        .env_remove("EASYENV_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {shell}: {e}"));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Regression test for the hook self-hijack: since the fetched hook script
/// bakes in an absolute path to the real binary (see
/// `ShellKind::resolve_easyenv_invocation`), a directory whose `.env` sets
/// `PATH` to somewhere else entirely must not cause the *next* prompt's
/// `easyenv export` call to run whatever `easyenv` resolves to on the new
/// PATH -- it must keep calling the real binary directly.
#[cfg(unix)]
fn scenario_hook_survives_path_hijack(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_env(&dir.join(".env"), "FOO=real_value\n");

    let shim_dir = tmp.path().join("shim");
    std::fs::create_dir_all(&shim_dir).unwrap();
    write_fake_easyenv_shim(&shim_dir);

    let script = "echo \"M1 FOO=$FOO\"\nexit\n";
    let out = run_in_shell_with_hijacked_path(shell, &dir, script, &shim_dir);

    assert!(
        out.contains("M1 FOO=real_value"),
        "the hook must call the real easyenv binary via its baked-in \
         absolute path, not whatever `easyenv` resolves to on PATH; got:\n{out}"
    );
    assert!(
        !out.contains("FAKE_EASYENV_SHIM_RAN"),
        "a fake `easyenv` earlier on PATH must never run in place of the \
         real binary; got:\n{out}"
    );
}
#[cfg(unix)]
for_each_shell!(
    hook_survives_path_hijack,
    scenario_hook_survives_path_hijack
);

// --- Denylist: a hostile .env must not achieve code execution --------
//
// These are the actual exploit scenarios from security.md/blocklist.md,
// exercised against the real hook -> eval loop, not just the config
// engine's unit tests. Each writes a `.env` that would, if applied,
// either run an arbitrary command (proven by a marker file appearing) or
// redirect a lookup path (proven by echoing the variable back), and
// asserts that a legitimate sibling key still loads normally alongside
// the denied one.

fn scenario_prompt_command_injection_is_blocked(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let marker = tmp.path().join("prompt_command_pwned_marker");
    write_env(
        &dir.join(".env"),
        &format!(
            "PROMPT_COMMAND=\"touch {}\"\nGOOD_VAR=ok\n",
            marker.display()
        ),
    );

    let script = "echo \"M1 GOOD_VAR=$GOOD_VAR\"\nexit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 GOOD_VAR=ok"),
        "a legitimate sibling key must still load; got:\n{out}"
    );
    assert!(
        !marker.exists(),
        "PROMPT_COMMAND from a .env must never execute, even though the \
         hook's own eval runs in the live interactive shell"
    );
}
for_each_shell!(
    prompt_command_injection_is_blocked,
    scenario_prompt_command_injection_is_blocked
);

fn scenario_ps1_injection_is_blocked(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let marker = tmp.path().join("ps1_pwned_marker");
    write_env(
        &dir.join(".env"),
        &format!("PS1=\"$(touch {})\"\nGOOD_VAR=ok\n", marker.display()),
    );

    let script = "echo \"M1 GOOD_VAR=$GOOD_VAR\"\nexit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 GOOD_VAR=ok"),
        "a legitimate sibling key must still load; got:\n{out}"
    );
    assert!(
        !marker.exists(),
        "PS1 from a .env must never execute via command substitution \
         (bash performs it in prompt strings by default)"
    );
}
for_each_shell!(ps1_injection_is_blocked, scenario_ps1_injection_is_blocked);

fn scenario_path_from_env_is_denied(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_env(&dir.join(".env"), "PATH=/nonexistent/evil\nGOOD_VAR=ok\n");

    let script = "echo \"M1 GOOD_VAR=$GOOD_VAR \
                   PATH_CHANGED=$([ \"$PATH\" = /nonexistent/evil ] && echo yes || echo no)\"\n\
                   exit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 GOOD_VAR=ok"),
        "a legitimate sibling key must still load; got:\n{out}"
    );
    assert!(
        out.contains("PATH_CHANGED=no"),
        "PATH must never be set from a .env; got:\n{out}"
    );
}
for_each_shell!(path_from_env_is_denied, scenario_path_from_env_is_denied);

fn scenario_bash_env_is_denied(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let evil_script = tmp.path().join("evil.sh");
    let marker = tmp.path().join("bash_env_pwned_marker");
    write_env(&evil_script, &format!("touch {}\n", marker.display()));
    write_env(
        &dir.join(".env"),
        &format!("BASH_ENV={}\nGOOD_VAR=ok\n", evil_script.display()),
    );

    let script = "echo \"M1 GOOD_VAR=$GOOD_VAR BASH_ENV=[$BASH_ENV]\"\n\
                   bash -c 'true'\n\
                   exit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 GOOD_VAR=ok BASH_ENV=[]"),
        "BASH_ENV must never be set from a .env; got:\n{out}"
    );
    assert!(
        !marker.exists(),
        "a nested non-interactive bash must not source a BASH_ENV script \
         that was never exported in the first place"
    );
}
for_each_shell!(bash_env_is_denied, scenario_bash_env_is_denied);

fn scenario_shellopts_denial_does_not_break_legitimate_loading(shell: &str) {
    let tmp = scratch_dir();
    let dir = tmp.path().join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    // SHELLOPTS is readonly in a running bash -- exporting it would
    // itself produce a shell error inside the hook's eval on every
    // prompt if it weren't denied. Denying it fixes that cosmetic bug as
    // well as the security one; the main thing to prove here is that a
    // legitimate sibling key still loads fine regardless.
    write_env(&dir.join(".env"), "SHELLOPTS=xtrace\nGOOD_VAR=ok\n");

    let script = "echo \"M1 GOOD_VAR=$GOOD_VAR\"\nexit\n";
    let out = run_in_shell(shell, &dir, script);

    assert!(
        out.contains("M1 GOOD_VAR=ok"),
        "a legitimate sibling key must still load even though SHELLOPTS \
         in the same file is denied; got:\n{out}"
    );
}
for_each_shell!(
    shellopts_denial_does_not_break_legitimate_loading,
    scenario_shellopts_denial_does_not_break_legitimate_loading
);
