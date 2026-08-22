mod helper;
mod process;

use crate::helper::{is_go_project, is_rust_project};
use clap::Parser;
use colored::*;
use command_group::CommandGroup;
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{path::Path, thread, time, time::Duration};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// File extensions to watch (comma-separated),e.g. "js,css", empty = all
    #[arg(short, long, default_value = "")]
    ext: String,

    #[arg(skip)]
    ext_set: HashSet<String>,

    /// Dirs to watch (comma-separated)
    #[arg(short, long, default_value_t = default_dir())]
    dir: String,

    #[arg(short, long, default_value_t=default_ignore())]
    ignore: String,

    #[arg(skip)]
    ignore_glob_set: Option<GlobSet>,

    /// Command to run (use -- before command)
    #[arg(last = true, required = true)]
    cmd: Vec<String>,
}

fn default_ignore() -> String {
    let mut ignore = String::from("**/.*,**/.*/**,**/*.log,**~");
    if is_rust_project() {
        ignore.push_str(",**/target/**,**/Cargo.lock,**/Cargo.toml");
    }
    let root = std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
        .replace("\\", "/");
    for i in helper::read_gitignore() {
        let mut str = String::new();
        str.push_str(root.as_str());
        str.push_str("/");
        if !i.contains("*") {
            str.push_str(i.as_str());
            let p = Path::new(i.as_str().trim_start_matches("/"));
            if p.is_dir() {
                str.push_str("/**");
            }
        } else {
            if !i.starts_with("**/") {
                str.push_str("**/");
            }
            str.push_str(i.as_str());
        }
        str = str.replace("//", "/");
        if str.ends_with("/") {
            str.push_str("**")
        }
        ignore.push_str(",");
        ignore.push_str(&str);
    }
    ignore
}

fn default_dir() -> String {
    if is_go_project() {
        return helper::go_deps_dirs().join(",");
    }
    if is_rust_project() {
        return String::from("src");
    }
    String::from(".")
}

impl Args {
    fn setup(&mut self) {
        self.ext_set = HashSet::new();
        for ext in self.ext.split(',').map(|s| s.trim()) {
            if ext.is_empty() {
                continue;
            }
            self.ext_set.insert(ext.to_string());
        }
        self.ignore_glob_set = None;

        if !self.ignore.is_empty() {
            let mut ignore = GlobSetBuilder::new();
            for p in self.ignore.split(',').map(|s| s.trim()) {
                if p.is_empty() {
                    continue;
                }
                ignore.add(Glob::new(&p).expect(format!("invalid ignore rule: {}", p).as_str()));
            }

            self.ignore_glob_set = Some(ignore.build().expect("invalid ignore rule"));
        }
    }

    fn get_dirs(&self) -> Vec<&Path> {
        if self.dir.is_empty() {
            vec![Path::new(".")]
        } else {
            self.dir
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(Path::new)
                .collect()
        }
    }

    fn get_ignore(&self) -> Vec<String> {
        if self.ignore.is_empty() {
            vec![]
        } else {
            self.ignore
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
    }

    fn is_ignore_match(&self, path: PathBuf) -> bool {
        if let Some(ignore_glob_set) = &self.ignore_glob_set {
            let p = path.to_string_lossy().replace('\\', "/");
            let ret = ignore_glob_set.is_match(Path::new(&p));
            // log!("is_ignore_match：{}, match={}",p,ret);
            return ret;
        }
        false
    }

    fn is_ext_match(&self, path: PathBuf) -> bool {
        if self.ext_set.is_empty() {
            return true;
        }
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            return self.ext_set.contains(ext);
        }
        true
    }

    fn is_match(&self, paths: Vec<PathBuf>) -> bool {
        for path in paths {
            if self.is_ignore_match(path.clone()) {
                continue;
            }
            if self.is_ext_match(path.clone()) {
                helper::set_last_file(path.file_name().unwrap().to_str().unwrap_or(""));
                return true;
            }
        }
        false
    }

    async fn handle_event(&self, event: notify::Event, changed: Arc<AtomicBool>) {
        use notify::EventKind;
        match event.kind {
            EventKind::Create(_kind) => {
                let paths = helper::filter_dir(event.paths.clone());
                if self.is_match(paths.clone()) {
                    let msg = format!("(matched) created, paths: {:?}", paths);
                    log!("{}", msg.green());
                    changed.store(true, Ordering::SeqCst);
                } else {
                    let msg = format!("(ignore) created, paths: {:?}", event.paths);
                    log!("{}", msg.bright_black());
                }
            }

            EventKind::Modify(_kind) => {
                let paths = helper::filter_dir(event.paths.clone());
                if self.is_match(paths.clone()) {
                    let msg = format!("(matched) modified, paths: {:?}", paths);
                    log!("{}", msg.green());
                    changed.store(true, Ordering::SeqCst);
                } else {
                    let msg = format!("(ignore) modified, paths: {:?}", event.paths);
                    log!("{}", msg.bright_black());
                }
            }

            EventKind::Remove(_kind) => {
                if self.is_match(event.paths.clone()) {
                    let msg = format!("(matched) removed, paths: {:?}", event.paths);
                    log!("{}", msg.green());
                    changed.store(true, Ordering::SeqCst);
                } else {
                    let msg = format!("(ignore) removed, paths: {:?}", event.paths);
                    log!("{}", msg.bright_black());
                }
            }

            _ => {
                let msg = format!(
                    "(ignore) notify-event: {:?}, paths: {:?}",
                    event.kind, event.paths
                );
                log!("{}", msg.bright_black());
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let mut args = Args::parse(); // 如何让 args 变成全局变量

    let mut ignore = GlobSetBuilder::new();
    for p in args.get_ignore() {
        ignore.add(Glob::new(&p).expect(format!("invalid ignore rule: {}", p).as_str()));
    }

    args.setup();

    let cmd = args.cmd.clone();

    log!("Watching git changes..., Command= {:?}", cmd);

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        Config::default(),
    )
    .expect("init watcher failed");

    for dir in args.get_dirs() {
        watcher
            .watch(dir, RecursiveMode::Recursive)
            .expect(&format!("watcher {} failed", dir.display()));
    }
    let changed = Arc::new(AtomicBool::new(false));

    let changed_clone = changed.clone();
    let event_task = tokio::spawn(async move {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => args.handle_event(event, changed_clone.clone()).await,
                Err(err) => {
                    elog!("watch error: {err}");
                }
            }
        }
    });

    // let mut last_state: HashMap<String, FileState> = HashMap::new();
    let mut child: Option<command_group::GroupChild> = None;

    let running = Arc::new(AtomicBool::new(true));
    {
        let r = running.clone();
        ctrlc::set_handler(move || {
            log!("receive Ctrl+C");
            r.store(false, Ordering::SeqCst);
        })
        .expect("set Ctrl+C failed");
    }
    let mut last_exit: Option<std::time::Instant> = None;
    while running.load(Ordering::SeqCst) {
        // 检查子进程是否异常退出
        if let Some(ref mut c) = child {
            match c.try_wait() {
                Ok(Some(status)) => {
                    log!("Process exited: {}", status);
                    child = None;
                }
                Ok(None) => {}
                Err(e) => {
                    elog!("Error checking child process: {:?}", e);
                }
            }
        }

        if !child.is_none() && !changed.load(Ordering::SeqCst) {
            sleep(50);
            continue;
        }

        if let Some(le) = last_exit
            && le.elapsed().as_secs_f64() < 1.0
        {
            elog!("duration={:?}, Sleep 1000 ...", le.elapsed());
            sleep(1000);
        }

        last_exit = Some(time::Instant::now());

        helper::next_id_incr();

        log!("{}", "Detected changes.".red());

        let start = time::Instant::now();

        // kill 旧进程
        if let Some(c) = child.take() {
            process::kill(c);
            child = None;
        }

        let msg = format!("Starting: {:?}", cmd);
        log!("{}", msg.green());

        let mut command = process::shell_spawn(cmd.join(" ").as_str());
        log!("Command: {:?}", command);

        match command.group_spawn() {
            Ok(c) => {
                let elapsed = start.elapsed();
                let pid = c.id();
                child = Some(c);
                let msg = format!(
                    "Process started: {:?}, pid={}, cost={:?}",
                    cmd, pid, elapsed
                );
                log!("{}", msg.green());

                changed.store(false, Ordering::SeqCst);
            }
            Err(e) => elog!("Failed to start: {}", e),
        }
    }

    log!("Exiting...");
    event_task.abort();
    if let Some(c) = child.take() {
        process::kill(c);
    }
}

fn sleep(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
