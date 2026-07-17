use colored::Colorize;
use command_group::GroupChild;
use std::process::Command;

pub fn shell_spawn(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        if let Some(bash) = git_bash() {
            let mut cmd = Command::new(bash);
            cmd.args(["-c", command]);
            return cmd;
        }

        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    }
}

#[cfg(target_os = "windows")]
use std::env;
use std::path::Path;
fn git_bash() -> Option<String> {
    if let Some(shell) = env::var("SHELL").ok() {
        let shell = shell.trim_matches('"').to_string();
        if shell.ends_with("\\bash.exe") {
            return Some(shell);
        }
    }

    let path = env::var("GIT_BASH").ok()?;
    let path = path.trim_matches('"').to_string();
    if Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(unix)]
fn graceful_stop(c: &GroupChild) -> std::io::Result<()> {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };

    killpg(Pid::from_raw(c.id() as i32), Signal::SIGINT).map_err(std::io::Error::other)
}

pub fn kill(mut c: GroupChild) {
    let msg = format!(
        "[oh-watch] Stopping previous process (pid={:?}) ...",
        c.id()
    );
    println!("{}", msg.red());

    // Unix 下先尝试优雅退出
    #[cfg(unix)]
    {
        use std::{thread, time::Duration, time::Instant};
        let _ = graceful_stop(&c);
        let deadline = Instant::now() + Duration::from_secs(3);

        loop {
            match c.try_wait() {
                Ok(Some(status)) => {
                    println!("[oh-watch] process exited gracefully: {}", status);
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("[oh-watch] process wait failed: {}", e);
                    return;
                }
            }

            if Instant::now() >= deadline {
                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        println!("[oh-watch] graceful shutdown timeout, force killing...");
    }

    if let Err(e) = c.kill() {
        eprintln!(
            "[oh-watch] failed to kill process (pid={}), err: {}",
            c.id(),
            e
        );
    } else {
        println!("[oh-watch] process killed (pid={})", c.id());
    }

    match c.wait() {
        Ok(status) => {
            println!("[oh-watch] process wait exited: {}", status);
        }
        Err(e) => {
            eprintln!("[oh-watch] process wait failed: {}", e);
        }
    }
}
