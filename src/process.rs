use crate::{elog, log};
use colored::Colorize;
use command_group::GroupChild;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler};
use windows::core::BOOL;

#[cfg(windows)]
fn command_exists(command: &str) -> bool {
    std::process::Command::new("where.exe").arg(command).output().map(|output| output.status.success()).unwrap_or(false)
}

// Windows API 常量
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

pub fn shell_command(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        if let Some(shell) = env::var("MSYSTEM").ok() {
            if shell.eq("MINGW64") {
                let mut cmd = Command::new("sh");
                cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
                cmd.args(["-c", command]);
                return cmd;
            }
        }

        if let Some(bash) = git_bash() {
            let mut cmd = Command::new(bash);
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
            cmd.args(["-c", command]);
            return cmd;
        }

        if command_exists("powershell.exe") {
            let mut cmd = Command::new("powershell.exe");
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
            cmd.arg("-NoProfile").arg("-Command").arg(command);
            return cmd;
        }

        let mut cmd = Command::new("cmd");
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
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
        if shell.ends_with("\\bash.exe") || shell.ends_with("/bash") {
            return Some(shell);
        }
    }

    let path = env::var("GIT_BASH").ok()?;
    let path = path.trim_matches('"').to_string();
    if Path::new(&path).exists() { Some(path) } else { None }
}

#[cfg(unix)]
fn graceful_stop(c: &GroupChild) -> std::io::Result<()> {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };

    killpg(Pid::from_raw(c.id() as i32), Signal::SIGINT).map_err(std::io::Error::other)
}

#[cfg(windows)]
unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
    if ctrl_type == CTRL_BREAK_EVENT {
        BOOL(1) // 返回 1 表示 TRUE (拦截事件)
    } else {
        BOOL(0) // 返回 0 表示 FALSE (不拦截，交给后续 handler)
    }
}

// 定义全局标志位：标识当前是否正在给子进程发送控制信号
pub static IS_SENDING_SIGNAL: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
fn graceful_stop(c: &GroupChild) -> std::io::Result<()> {
    unsafe {
        // 1. 设置标志位：告诉父进程自己的 ctrlc handler 忽略本次信号
        IS_SENDING_SIGNAL.store(true, Ordering::SeqCst);

        // 1. 临时设置父进程忽略 CTRL_BREAK 信号
        SetConsoleCtrlHandler(Some(ctrl_handler), true).map_err(|_| std::io::Error::last_os_error())?;

        // 2. 发送信号给子进程组
        let send_res = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, c.id()).map_err(|_| std::io::Error::last_os_error());

        send_res.map_err(|_| std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn after_graceful_stop() {
    unsafe {
        // 5. 恢复标志位
        IS_SENDING_SIGNAL.store(false, Ordering::SeqCst);

        // 4. 恢复父进程默认 Handler
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), false);
    }
}

pub fn kill(mut c: GroupChild) {
    let pid = c.id();
    let msg = format!("Stopping previous process (pid={:?}) ...", pid);
    log!("{}", msg.red());

    // Unix 下先尝试优雅退出
    #[cfg(any(unix, windows))]
    {
        use std::{thread, time::Duration, time::Instant};

        let exited = {
            #[cfg(unix)]
            let graceful_result = graceful_stop(&c);

            #[cfg(windows)]
            let graceful_result = graceful_stop(&c);

            match graceful_result {
                Ok(()) => {
                    log!("sent graceful shutdown signal (pid={})", pid);
                }
                Err(e) => {
                    elog!("failed to send graceful shutdown signal (pid={}), err: {}", pid, e);
                }
            }
            let deadline = Instant::now() + Duration::from_secs(5);

            loop {
                match c.try_wait() {
                    Ok(Some(status)) => {
                        log!("process exited gracefully: {}", status.to_string());
                        break true;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        elog!("process wait failed: {}", e);
                        return;
                    }
                }

                if Instant::now() >= deadline {
                    break false;
                }

                thread::sleep(Duration::from_millis(50));
            }
        };

        #[cfg(windows)]
        after_graceful_stop();

        if exited {
            return;
        }
        log!("graceful shutdown timeout, force killing...");
    }

    if let Err(e) = c.kill() {
        elog!("failed to kill process (pid={}), err: {}", pid, e);
    } else {
        log!("process killed (pid={})", pid);
    }

    match c.wait() {
        Ok(status) => {
            log!("process wait exited: {}", status);
        }
        Err(e) => {
            elog!("process wait failed: {}", e);
        }
    }
}
