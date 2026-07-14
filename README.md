```markdown
# 📁 文件检索助手 (FileSeeker)

一个使用 Rust 编写的高性能、跨平台（当前主要面向 Windows）文件名搜索工具，灵感来源于 Voidtools 的 [Everything](https://www.voidtools.com/)。

[![License](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

## ✨ 核心功能

- **极速搜索**: 采用与 Everything 类似的索引机制，对文件名和路径进行毫秒级搜索。
- **图形界面 (GUI)**:
  - 基于 `eframe`/`egui` 构建，原生性能，界面美观。
  - 支持中文显示、多标签页、右键菜单、列排序与调整。
  - 默认最小化到系统托盘，支持开机自启。
- **高级搜索语法**:
  - **通配符**: `*`, `?`
  - **逻辑运算**: `AND` (空格), `OR` (`|`), `NOT` (`!`)
  - **搜索函数**: `size:`, `dm:`, `dc:`, `ext:`, `attrib:`, `runcount:` 等。
  - **内容宏**: `audio:`, `doc:`, `pic:`, `video:`, `zip:`, `exe:` 一键筛选常见文件类型。
- **灵活索引管理**:
  - 支持添加自定义文件夹或整个磁盘。
  - 异步非阻塞索引，不影响 UI 响应。
  - 实时文件监控 (`notify`)，文件系统变更即时同步。
- **文件共享与远程访问**:
  - 内建 **HTTP 服务器**，可通过浏览器在局域网内搜索和下载文件。
  - 内建 **FTP 服务器**，方便设备间传输文件。
  - 实现基础 **ETP (Everything Transfer Protocol)** 服务。
- **命令行工具 (CLI)**:
  - 提供 `es.exe` 风格的命令行接口。
  - 完整支持所有搜索参数、排序和结果导出。
- **附加工具箱**:
  - **批量重命名**: 支持查找替换、计数、大小写等规则。
  - **EFU 文件列表**: 支持导入导出 Everything 文件列表。
  - **运行历史**: 记录文件打开次数和搜索历史。
  - **右键属性**: 在搜索结果中直接打开 Windows 文件属性对话框。

## 🚀 快速开始

### 环境要求
- **Rust 工具链**: 稳定版 1.70+
- **操作系统**: 
  - **Windows**: 完整功能支持（GUI、托盘、自启动）。
  - *Linux/macOS*: GUI 和部分功能可用，系统托盘等功能可能受限。

### 编译与运行

1. **克隆仓库**
   ```bash
   git clone <你的仓库地址>
   cd FileSeeker
   ```

2. **构建发布版本**
   ```bash
   cargo build --release
   ```

3. **运行**
   - **图形界面**:
     ```bash
     ./target/release/FileSeeker.exe
     ```
   - **命令行模式**:
     ```bash
     ./target/release/FileSeeker.exe --cli -r "搜索.*" -sort size-descending -n 20
     ```
   - **最小化到托盘启动**:
     ```bash
     ./target/release/FileSeeker.exe --minimized
     ```

### 常用命令示例 (CLI)
```bash
# 基本搜索
FileSeeker.exe --cli myfile.txt

# 正则搜索，匹配路径，导出 CSV
FileSeeker.exe --cli -regex -match-path "\.jpg$" -csv

# 搜索大于 100MB 的视频文件
FileSeeker.exe --cli "video: size:>100mb"

# 搜索本周修改过的文档
FileSeeker.exe --cli "doc: dm:thisweek"
```

## 🗺️ 项目结构

```text
src/
├── main.rs                    # 程序入口，模式分发（GUI/CLI）
├── lib.rs                     # 库根节点，公共模块导出
├── types.rs                   # 核心数据类型（FileEntry, 搜索选项等）
├── config.rs                  # Everything.ini 风格配置管理
├── autostart.rs               # Windows 注册表自启动管理
├── tray/                      # 系统托盘模块
│   ├── mod.rs                 #   托盘主逻辑 (WinAPI 实现)
├── tray_helper.rs             # 独立托盘助手程序入口
├── engine/                    # 核心搜索引擎
│   ├── mod.rs                 #   引擎协调与 API
│   ├── searcher.rs            #   搜索算法与高级语法解析
│   ├── indexer.rs             #   文件系统索引器
│   ├── database.rs            #   数据库加载/保存
│   └── sorter.rs              #   结果排序逻辑
├── gui/                       # 图形界面
│   ├── mod.rs
│   ├── app.rs                 #   主窗口状态管理 (egui App)
│   ├── options_panel.rs       #   设置/选项面板
│   ├── results_panel.rs       #   搜索结果表格与右键菜单
│   └── search_panel.rs        #   搜索栏与过滤器
├── cli/                       # 命令行接口
│   └── mod.rs                 #   参数解析与 CLI 执行逻辑
├── http_server/               # HTTP 服务器
│   └── mod.rs                 #   基于 tiny_http 的 Web 服务
├── ftp/                       # FTP 服务器
│   └── mod.rs
├── etp/                       # ETP (Everything Transfer Protocol) 服务
│   └── mod.rs
├── file_list/                 # EFU 文件列表导入导出
│   └── mod.rs
├── rename/                    # 批量重命名引擎
│   └── mod.rs
├── history/                   # 搜索与运行历史记录
│   └── mod.rs
├── watcher/                   # 文件系统实时监控
│   └── mod.rs
└── sdk/                       # Everything SDK 风格接口封装
    └── mod.rs
```

## 🖥️ 使用指南

### 首次启动
1. 启动后会自动弹出 **欢迎向导**。
2. 点击 `选择文件夹` 添加您想快速搜索的常用目录，或点击 `💿 选择磁盘` 索引整个磁盘。
3. 点击 `✅ 开始索引` 等待索引完成。
4. 在搜索框输入关键字即可开始搜索。

### 索引与实时监控
- **手动重建**: 点击界面 `重建索引` 按钮，或在 `设置 → 高级` 中管理索引路径。
- **自动监控**: 在 `设置 → 通用` 中启用 `实时监控文件变化`，程序会自动捕获文件的增删改并更新索引。
- **增量索引间隔**: 可配置定期全量校验间隔，确保索引准确性。

### 文件共享
1. 进入 `设置 → 共享`。
2. 点击 `启动` HTTP 或 FTP 服务器。
3. 在同一局域网的其他设备上，通过显示的地址（如 `http://192.168.1.100:8080`）即可访问、搜索和下载您索引的文件。

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！有任何建议或问题，请在 GitHub 上提出。

## ⚖️ 开源协议

本项目基于 MIT 协议开源。详情见 [LICENSE](./LICENSE) 文件。 (假设使用 MIT)
```
