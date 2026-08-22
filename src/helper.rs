use colored::Colorize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
pub fn next_id_incr() -> usize {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

static LAST_FILE: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

fn get_last_file() -> String {
    LAST_FILE.lock().unwrap().clone()
}

pub fn set_last_file(file: impl Into<String>) {
    let file = file.into();

    let file = if file.chars().count() > 16 {
        let start: String = file.chars().take(12).collect();

        let end: String = file
            .chars()
            .rev()
            .take(3)
            .collect::<String>()
            .chars()
            .rev()
            .collect();

        format!("{start}*{end}")
    } else {
        file
    };

    *LAST_FILE.lock().unwrap() = file;
}

pub fn log_prefix() -> String {
    let tm = format!("[{}]", chrono::Local::now().format("%H:%M:%S"));
    let num_file = format!(
        "[{}-{}]",
        NEXT_ID.load(std::sync::atomic::Ordering::SeqCst),
        get_last_file()
    );
    format!(
        "{} {} {}",
        "[oh-watch]".cyan(),
        tm.bright_black(),
        num_file.yellow()
    )
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        println!(
            "{} {}",
            $crate::helper::log_prefix(),
            format_args!($($arg)*)
        )
    };
}

#[macro_export]
macro_rules! elog {
    ($($arg:tt)*) => {
        eprintln!(
            "{} {}",
            $crate::helper::log_prefix(),
            format_args!($($arg)*)
        )
    };
}

pub fn is_go_project() -> bool {
    Path::new("go.mod").is_file()
}

pub fn is_rust_project() -> bool {
    Path::new("Cargo.toml").is_file()
}

// go list -f '{{if and (not .Standard) .Module}}{{.Module.Path}} => {{.Module.Dir}}{{end}}' -deps ./...
pub fn go_deps_dirs() -> Vec<String> {
    let output = Command::new("go")
        .args([
            "list",
            "-f",
            "{{if and (not .Standard) .Module}}{{.Module.Path}} => {{.Module.Dir}}{{end}}",
            "-deps",
            "./...",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout)
        .unwrap_or_default();

    let mut seen = std::collections::HashSet::new();
    let lines: Vec<String> = String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|line| line.split_once(" => ").map(|(_, dir)| dir.trim()))
        .filter(|line| !line.is_empty() && !line.contains('@'))
        .filter(|dir| seen.insert(*dir))
        .map(str::to_owned)
        .collect();
    lines
}

pub fn read_gitignore() -> Vec<String> {
    std::fs::read_to_string(".gitignore")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

pub fn filter_dir(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|path| path.is_file()).collect()
}
