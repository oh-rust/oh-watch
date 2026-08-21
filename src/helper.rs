use std::sync::atomic::{AtomicUsize, Ordering};
use std::path::Path;
use std::process::Command;

pub static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
pub fn next_id_incr() -> usize {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        println!(
            "[oh-watch] [{}] [{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            $crate::helper::NEXT_ID.load(std::sync::atomic::Ordering::SeqCst),
            format_args!($($arg)*)
        )
    };
}

#[macro_export]
macro_rules! elog {
    ($($arg:tt)*) => {
        eprintln!(
            "[oh-watch] [{}] [{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            $crate::helper::NEXT_ID.load(std::sync::atomic::Ordering::SeqCst),
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
pub fn go_deps_dirs()-> Vec<String> {
    let output = Command::new("go")
        .args([
            "list",
            "-f",
            "{{if and (not .Standard) .Module}}{{.Module.Path}} => {{.Module.Dir}}{{end}}",
            "-deps",
            "./...",
        ]).output().ok()
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