mod config;
mod helper;
mod process;

use colored::*;
use command_group::CommandGroup;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{thread, time, time::Duration};

#[tokio::main]
async fn main() {
    let mut args = config::parse();
    args.setup();

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        Config::default(),
    )
    .expect("init watcher failed");

    for dir in args.clone().get_dirs() {
        watcher.watch(dir, RecursiveMode::Recursive).expect(&format!("watcher {} failed", dir.display()));
    }
    let changed = Arc::new(AtomicBool::new(false));

    let changed_clone = changed.clone();
    let args_cloned = args.clone();
    let event_task = tokio::spawn(async move {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => args_cloned.handle_event(event, changed_clone.clone()).await,
                Err(err) => {
                    elog!("watch error: {err}");
                }
            }
        }
    });

    // 定时检查指定的文件列表是否有变化
    let pull_interval = args.get_pull_interval();
    if pull_interval > 0 {
        let mut args_cloned = args.clone();
        let changed_clone = changed.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(pull_interval));
            loop {
                ticker.tick().await;

                if args_cloned.pull_files_change() {
                    changed_clone.store(true, Ordering::Release);
                }
            }
        });
    }

    let mut child: Option<command_group::GroupChild> = None;

    let running = Arc::new(AtomicBool::new(true));
    {
        let r = running.clone();
        ctrlc::set_handler(move || {
            if process::IS_SENDING_SIGNAL.load(Ordering::SeqCst) {
                log!("Ignored signal caused by sending CTRL_BREAK to child");
                return;
            }
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

        if !args.try_build() {
            if let Some(le) = last_exit
                && le.elapsed().as_secs_f64() < 0.5
            {
                elog!("duration={:?}, Sleep 300 ...", le.elapsed());
                sleep(300);
            }
            continue;
        }

        if let Some(le) = last_exit
            && le.elapsed().as_secs_f64() < 1.0
        {
            elog!("duration={:?}, Sleep 700 ...", le.elapsed());
            sleep(700);
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

        if helper::is_go_project(){
            let _=helper::clean_go_tmp_dir();
        }

        let mut command = args.run_cmd();
        log!("Exec Command: {}", format!("{:?}", command).green());

        match command.group_spawn() {
            Ok(c) => {
                let elapsed = start.elapsed();
                let pid = c.id();
                child = Some(c);
                let msg = format!("Process started: {:?}, pid={}, cost={:?}", command, pid, elapsed);
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
