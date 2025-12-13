# yuyubot

yuyubot 是一个基于 Rust 构建的现代化桌面端 Bot 管理框架，专为 **Milky 协议** 设计。它提供了一个轻量级、高性能的图形化界面，用于管理 Bot 连接、插件运行及日志监控。

## ✨ 核心特性

- **🚀 现代化技术栈**
  - **后端**：Rust (Rocket + Tokio + Expectrl)，极致性能与内存安全。
  - **前端**：Vue.js + WebView (Tao/Wry)，提供原生级桌面体验，无需 Electron 的庞大体积。
  - **通信**：基于 SSE (Server-Sent Events) 的实时数据流。

- **🔌 强大的插件系统**
  - **进程隔离**：每个插件在独立的子进程中运行，互不干扰，崩溃不影响主程序。
  - **无缝集成**：自动将 Bot 连接信息（Host, Port, Token）注入插件环境变量。
  - **实时监控**：在界面中实时查看每个插件的标准输出 (stdout/stderr) 日志。
  - **热插拔**：支持动态启用/禁用插件，无需重启主程序。

- **🤖 Milky 协议支持**
  - 完美对接 Milky 协议服务端。
  - 支持 Token 鉴权。
  - 自动断线重连。

- **📊 可视化管理**
  - **仪表盘**：查看系统状态、Bot 连接状态。
  - **日志中心**：统一的日志查看器，支持按插件筛选和实时滚动。
  - **配置管理**：图形化配置 Bot 连接参数，无需手动修改 JSON 文件。

## 🛠️ 项目结构

```
yuyubot/
├── src/                # Rust 后端源码
│   ├── plus/           # 插件管理系统 (Manager, Plugin, Process)
│   ├── server/         # Web API & Bot 连接逻辑
│   ├── logger.rs       # 日志系统
│   └── main.rs         # 程序入口
├── res/                # 前端资源 (Vue.js 应用)
│   ├── pages/          # Vue 组件页面
│   ├── index.html      # 前端入口
│   └── ...
└── Cargo.toml          # 依赖配置
```

## 🚀 快速开始

### 前置要求

- [Rust](https://www.rust-lang.org/) (Stable)
- Windows 10/11 (目前主要支持 Windows)

### 运行开发环境

```bash
cargo run
```

### 构建发布版本

```bash
cargo build --release
```

构建完成后，可执行文件位于 `target/release/yuyubot.exe`。

## 🧩 插件开发指南

yuyubot 的插件是独立的**可执行程序**（可以是 Python 脚本、Node.js 程序或编译后的二进制文件）。

### 1. 插件结构

在 `app/` 目录下创建一个新文件夹（例如 `my-plugin`），结构如下：

```
app/
└── my-plugin/
    ├── app.json        # 插件描述文件
    ├── main.exe        # 插件入口 (或 main.py, index.js 等)
    └── ...             # 其他依赖文件
```

### 2. app.json 规范

```json
{
  "name": "示例插件",
  "version": "1.0.0",
  "description": "这是一个测试插件",
  "entry": "main.exe",  // 插件启动入口命令
  "author": "Your Name"
}
```

### 3. 环境变量注入

yuyubot 会在启动插件时自动注入以下环境变量，供插件连接 Bot 使用：

| 环境变量名 | 描述 |
|------------|------|
| `YUYU_HOST` | Bot API 主机地址 (例如 `127.0.0.1`) |
| `YUYU_API_PORT` | HTTP API 端口 |
| `YUYU_EVENT_PORT` | 事件流端口 |
| `YUYU_TOKEN` | 鉴权 Token (如果有) |
| `YUYU_DATA_DIR` | 插件专属数据目录 (例如 `data/my-plugin`) |

### 4. 日志输出

插件只需向 **标准输出 (stdout)** 打印内容，yuyubot 会自动捕获并在“日志”页面显示。

## 📄 许可证

本项目未指定特定的开源许可证。
