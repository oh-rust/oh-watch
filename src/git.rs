use crate::FileState;
use colored::Colorize;
use std::collections::HashMap;
use std::io;
use std::process::Command;
use std::time::SystemTime;

pub fn unstaged_files() -> Result<Vec<String>, io::Error> {
    let output = Command::new("git").args(["status", "-su"]).output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = filter_git_m_not_staged(&stdout);

    Ok(files)
}

//
//  git status -su
//   XY 文件路径
//   MM file.html   ->  已修改（modified），且已 git add,而且工作区有修改（未 add）
//
// 第一列（X）的含义: 暂存区（index）状态
//   标志	含义
//      （空格）	暂存区无变化
//     M	已修改（modified），且已 git add
//     A	已新增（added），已加入暂存区
//     D	已删除（deleted），已暂存
//     R	重命名（renamed）
//     C	复制（copied）
//     U	冲突（unmerged）
//
//  第二列（Y）的含义:工作区（working tree）状态
//     标志	含义
//     （空格）	工作区无变化
//     M	工作区有修改（未 add）
//     D	工作区已删除
//     ?	未跟踪文件（配合 -u）
//     U	冲突
//
//  ?? 是一个整体，表示“未跟踪文件（untracked）”,既不在暂存区，也不在版本库中 —— 完全是 Git 不认识的新文件
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

            if matches!(first, ' ' | '?' | 'A' | 'M') {
                if matches!(second, 'M' | 'A' | '?') {
                    Some(line.to_string())
                } else {
                    None
                }
            } else if matches!(first, 'R') {
                // R -> rename
                // R  old_name.html -> new_name.html
                if let Some(pos) = line.split("->").nth(1) {
                    let result = format!("R{} {}", " ", pos.trim());
                    Some(result)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect()
}

pub fn parse_status(
    output: &Vec<String>,
    exts: &Option<Vec<String>>,
) -> HashMap<String, FileState> {
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
                eprintln!(
                    "[oh-watch] Failed to get metadata for file: {}，err: {:?}",
                    path, e
                );
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
                eprintln!(
                    "[oh-watch] Failed to get modified for file: {}, err: {:?}",
                    path, e
                );
                continue;
            }
        };

        map.insert(path, FileState { mtime });
    }

    map
}

pub fn has_changed(old: &HashMap<String, FileState>, new: &HashMap<String, FileState>) -> bool {
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
