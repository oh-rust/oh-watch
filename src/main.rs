mod git;
mod process;

use clap::Parser;
use colored::*;
use command_group::CommandGroup;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::HashMap, thread, time::Duration, time::SystemTime};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// File extensions to watch (comma-separated),e.g. "js,css", empty = all
    #[arg(short, long, default_value = "")]
    ext: String,

    /// Polling interval in milliseconds
    #[arg(short, long, default_value_t = 800)]
    interval: u64,

    /// Command to run (use -- before command)
    #[arg(last = true, required = true)]
    cmd: Vec<String>,
}

#[derive(Clone, Debug)]
struct FileState {
    mtime: SystemTime,
}

fn main() {
    let args = Args::parse();

    let exts: Option<Vec<String>> = if args.ext.is_empty() {
        None
    } else {
        Some(
            args.ext
                .split(',')
                .map(|s| {
                    let ext = s.trim_start_matches('.');
                    return format!(".{}", ext);
                })
                .collect(),
        )
    };

    let interval = args.interval;
    let cmd = args.cmd;

    println!(
        "[oh-watch] Watching git changes..., Extensions= {:?}, Interval= {}ms, Command= {:?}",
        exts, interval, cmd
    );

    let mut last_state: HashMap<String, FileState> = HashMap::new();
    let mut child: Option<command_group::GroupChild> = None;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("[oh-watch] receive Ctrl+C");
        r.store(false, Ordering::SeqCst);
    })
    .expect("set Ctrl+C failed");

    while running.load(Ordering::SeqCst) {
        // 检查子进程是否异常退出
        if let Some(ref mut c) = child {
            match c.try_wait() {
                Ok(Some(status)) => {
                    println!("[oh-watch] Process exited: {}", status);
                    child = None;
                }
                Ok(None) => {}
                Err(e) => eprintln!("[oh-watch] Error checking child process: {}", e),
            }
        }

        let unstaged_files = match git::unstaged_files() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[oh-watch] git error: {}", e);
                sleep(interval);
                continue;
            }
        };

        let current_state = git::parse_status(&unstaged_files, &exts);

        if !child.is_none() && !git::has_changed(&last_state, &current_state) {
            sleep(interval);
            continue;
        }

        println!("{}", "[oh-watch] Detected changes.".red());

        // kill 旧进程
        if let Some(c) = child.take() {
            process::kill(c);
            child = None;
        }

        let msg = format!("[oh-watch] Starting: {:?}", cmd);
        println!("{}", msg.green());

        // let mut command = Command::new(&cmd[0]);// 这里换成 xshell
        // if cmd.len() > 1 {
        //     command.args(&cmd[1..]);
        // }
        let mut command = process::shell_spawn(cmd.join(" ").as_str());
        println!("Command: {:?}", command);

        match command.group_spawn() {
            Ok(c) => {
                let pid = c.id();
                child = Some(c);
                let msg = format!("[oh-watch] Process started: {:?}, pid={}", cmd, pid);
                println!("{}", msg.green());
            }
            Err(e) => eprintln!("[oh-watch] Failed to start: {}", e),
        }

        last_state = current_state;
    }

    println!("[oh-watch] Exiting...");
    if let Some(c) = child.take() {
        process::kill(c);
    }
}

fn sleep(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
