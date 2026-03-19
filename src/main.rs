use clap::Parser;
use colored::*;
use command_group::{CommandGroup, GroupChild};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::HashMap, io, process::Command, thread, time::Duration, time::SystemTime};

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

        let unstaged_files = match git_unstaged_files() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[oh-watch] git error: {}", e);
                sleep(interval);
                continue;
            }
        };

        let current_state = parse_git_status(&unstaged_files, &exts);

        if !child.is_none() && !has_changed(&last_state, &current_state) {
            sleep(interval);
            continue;
        }

        println!("{}", "[oh-watch] Detected changes.".red());

        // kill 旧进程
        if let Some(c) = child.take() {
            kill_process(c);
            child = None;
        }

        let msg = format!("[oh-watch] Starting: {:?}", cmd);
        println!("{}", msg.green());

        let mut command = Command::new(&cmd[0]);
        if cmd.len() > 1 {
            command.args(&cmd[1..]);
        }

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
        kill_process(c);
    }
}

fn kill_process(mut c: GroupChild) {
    let msg = format!("[oh-watch] Stopping previous process (pid={:?}) ...", c.id());
    println!("{}", msg.red());

    if let Err(e) = c.kill() {
        eprintln!("[oh-watch] failed to kill process (pid={}), err: {}", c.id(), e);
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

fn git_unstaged_files() -> Result<Vec<String>, io::Error> {
    let output = Command::new("git").args(["status", "-su"]).output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = filter_git_m_not_staged(&stdout);

    Ok(files)
}

fn filter_git_m_not_staged(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }

            let status = &line[0..2];
            // let path = line[3..].trim().to_string();

            let first = status.chars().next()?;
            let second = status.chars().nth(1)?;

            if matches!(first, ' ' | '?' | 'A') && matches!(second, 'M' | 'A' | '?') {
                Some(line.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn parse_git_status(output: &Vec<String>, exts: &Option<Vec<String>>) -> HashMap<String, FileState> {
    let mut map = HashMap::new();

    for line in output {
        // porcelain 格式：XY path
        if line.len() < 3 {
            continue;
        }

        let path = line[3..].trim().to_string();

        // 后缀过滤
        if let Some(exts) = exts {
            let matched = exts.iter().any(|ext| path.ends_with(ext));
            if !matched {
                continue;
            }
        }

        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[oh-watch] Failed to get metadata for file: {}，err: {:?}", path, e);
                if e.kind() == std::io::ErrorKind::NotFound {
                    map.insert(
                        path,
                        FileState {
                            mtime: SystemTime::UNIX_EPOCH,
                        },
                    );
                }
                continue;
            }
        };

        let mtime = match metadata.modified() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[oh-watch] Failed to get modified for file: {}, err: {:?}", path, e);
                continue;
            }
        };

        map.insert(path, FileState { mtime });
    }

    map
}

fn has_changed(old: &HashMap<String, FileState>, new: &HashMap<String, FileState>) -> bool {
    if old.len() != new.len() {
        return true;
    }

    for (path, new_fs) in new {
        match old.get(path) {
            Some(old_fs) => {
                if new_fs.mtime != old_fs.mtime {
                    let msg = format!("[oh-watch] file {} changed", path);
                    println!("{}", msg.bright_blue());
                    return true;
                }
            }
            None => {
                let msg = format!("[oh-watch] file {} added", path);
                println!("{}", msg.bright_blue());
                return true;
            }
        }
    }

    false
}

fn sleep(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
