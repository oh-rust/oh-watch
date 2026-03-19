# oh-watch

## 概述
oh-watch 是一个基于 Git 状态变化的自动重启工具，通过定时轮询检测文件变化，当检测到项目中有文件发生修改时，自动重启指定命令。

## 功能特性
1. 🔍 基于 Git 状态检测变化
  - 监听 git status --porcelain -uall 输出
  - 识别未暂存修改文件（unstaged）
  - 识别未跟踪文件（untracked）
2. 📁 支持文件后缀过滤
  - 通过 -e go,html 指定监听的文件类型
  - 不关心的文件将被忽略
3. ⏱ 定时轮询机制 
  - 通过 -i 指定轮询间隔（毫秒）
  - 适用于不支持文件系统监听的环境（如 WSL、网络磁盘）
4. 🔄 自动重启命令
  - 检测到文件变化后自动重启目标进程
  - 支持任意命令（如 go run main.go）
5. ⚙️ 子进程管理
 - 自动停止旧进程
 - 监控子进程异常退出并自动重启
6. 📦 轻量且无依赖监听机制
  - 不依赖 inotify / fswatch

通过 Git + 轮询实现跨平台稳定性

## 安装

```bash
cargo install --git https://github.com/oh-rust/oh-watch --branch master
```

## 使用方法
### 1. 参数说明
```bash
#oh-watch -help
Usage: oh-watch [OPTIONS] -- <CMD>...

Arguments:
  <CMD>...  Command to run (use -- before command)

Options:
  -e, --ext <EXT>            File extensions to watch (comma-separated),e.g. "js,css", empty = all [default: ""]
  -i, --interval <INTERVAL>  Polling interval in milliseconds [default: 800]
  -h, --help                 Print help
  -V, --version              Print version
```

### 2. 使用
```bash
oh-watch -- go run main.go
```