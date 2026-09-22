use crate::helper::{is_go_project, is_rust_project};
use crate::{elog, helper, log, process};
use clap::Parser;
use colored::*;
use command_group::CommandGroup;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Args {
    /// File extensions to watch (comma-separated),e.g. "js,css", empty = all
    #[arg(short, long, default_value = "")]
    ext: String,

    #[arg(skip)]
    ext_set: HashSet<String>,

    /// Dirs to watch (comma-separated)
    #[arg(short, long, default_value_t = default_dir())]
    dir: String,

    #[arg(short='i', long, default_value_t=default_ignore())]
    ignore: String,

    #[arg(skip)]
    ignore_glob_set: Option<GlobSet>,

    #[arg(skip)]
    pull_state: HashMap<String, time::SystemTime>,

    /// Polling interval for checking file changes, in milliseconds
    #[arg(short = 'I', long, default_value_t = 200)]
    interval: u64,

    /// Additional files to monitor using polling
    #[arg(short, long)]
    files: Vec<String>,

    /// Command to build, option
    #[arg(short, long, default_value = "")]
    build: String,

    /// Command to run (use -- before command)
    #[arg(last = true, required = true)]
    cmd: Vec<String>,
}

pub fn parse() -> Args {
    Args::parse()
}

fn default_ignore() -> String {
    let mut ignore = String::from("**/.*,**/.*/**,**/*.log,**~");
    if is_rust_project() {
        ignore.push_str(",**/target/**,**/Cargo.lock,**/Cargo.toml");
    }
    if is_go_project() {
        ignore.push_str(",**/*_test.go");
    }
    let root = std::env::current_dir().unwrap().to_str().unwrap().to_string().replace("\\", "/");
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
    pub fn setup(&mut self) {
        self.pull_state = HashMap::new();

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

        log!("Watching git changes..., Command= {:?}", self.cmd.clone());
    }

    pub fn get_pull_interval(&self) -> u64 {
        if self.interval < 1 || self.files.is_empty() {
            return 0;
        }
        self.interval
    }

    // 检查 files 是否有变化
    pub fn pull_files_change(&mut self) -> bool {
        if self.interval < 1 || self.files.is_empty() {
            return false;
        }

        let mut changed = false;
        for name in self.files.iter() {
            let modified = std::fs::metadata(name).and_then(|m| m.modified()).unwrap_or(time::SystemTime::UNIX_EPOCH);
            match self.pull_state.insert(name.clone(), modified) {
                Some(old) if old != modified => {
                    changed = true;
                }
                None => {
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }

    pub fn get_dirs(&self) -> Vec<&Path> {
        if self.dir.is_empty() {
            vec![Path::new(".")]
        } else {
            self.dir.split(',').map(str::trim).filter(|s| !s.is_empty()).map(Path::new).collect()
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

    pub async fn handle_event(&self, event: notify::Event, changed: Arc<AtomicBool>) {
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
                let msg = format!("(ignore) notify-event: {:?}, paths: {:?}", event.kind, event.paths);
                log!("{}", msg.bright_black());
            }
        }
    }

    pub fn try_build(&self) -> bool {
        if self.build.is_empty() {
            return true;
        }
        let mut c = process::shell_command(self.build.as_str());
        log!("{}", format!("try build: {:?}", c).yellow());
        let start = time::Instant::now();
        match c.group_status() {
            Ok(status) => {
                if status.success() {
                    log!("{}", format!("build {:?} success, cost={:?}", c, start.elapsed()).green());
                    true
                } else {
                    elog!("build {:?} failed, exit code {}, cost={:?}", c, status.code().unwrap(), start.elapsed());
                    false
                }
            }
            Err(err) => {
                elog!("build {:?} failed: {}", c, err);
                false
            }
        }
    }

    pub fn run_cmd(&self) -> std::process::Command {
        let mut cmd = process::shell_command(self.cmd.clone().join(" ").as_str());
        if is_go_project() {
            let dir = helper::go_tmp_dir();
            cmd.env("GOTMPDIR", &dir);
            let _ = fs::create_dir_all(&dir);
        }
        cmd
    }
}
