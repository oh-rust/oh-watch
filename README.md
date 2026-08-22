# oh-watch

## 概述
oh-watch 是一个基于 fs notify 的自动重启工具，当监听到项目中有文件发生修改时，自动重启指定命令。

## 功能特性
1. 🔍 基于 fs notify 事件变化
   - 监听 Create、Modify、Remove 事件
2. 📁 支持文件后缀过滤
   - 通过 -e go,html 指定监听的文件类型
3. 支持忽略规则 
   - 默认会读取项目跟目录下的 `.gitignore` 文件的配置加入到忽略列表中
4. 🔄 自动重启命令
   - 检测到文件变化后自动重启目标进程
   - 支持任意命令（如 go run main.go）
5. ⚙️ 子进程管理
   - 自动停止旧进程
   - 监控子进程异常退出并自动重启

## 安装
```bash
cargo install oh-watch
```
或者
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
  -e, --ext <EXT>        File extensions to watch (comma-separated),e.g. "js,css", empty = all [default: ""]
  -d, --dir <DIR>        Dirs to watch (comma-separated) [default: .]
  -i, --ignore <IGNORE>  [default: **/.*,**/.*/**,**/*.log,**~]
  -h, --help             Print help
  -V, --version          Print version
```

### 2. 使用
```bash
oh-watch -- go run main.go
```