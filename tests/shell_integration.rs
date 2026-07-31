//! End-to-end tests that actually launch interactive `bash`/`zsh` with
//! easyenv's real hook installed, drive a scripted command sequence via
//! stdin, and assert on what the shell echoed back. These exercise the
//! full shell-hook -> `easyenv export` -> `eval` loop, not just the diff
//! engine in isolation.

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

fn write_env(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

/// Runs `script` as stdin to an interactive `bash` or `zsh` with easyenv's
/// hook installed, starting in `start_dir`, and returns everything the
/// shell wrote to stdout. `shell` is `"bash"` or `"zsh"`.
fn run_in_shell(shell: &str, start_dir: &Path, script: &str) -> String {
    let bin_dir = easyenv_bin_dir();
    let rc_dir = TempDir::new().unwrap();

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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
