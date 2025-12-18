# YuyuBot

YuyuBot 是一个为 Milky 协议设计的桌面端 Bot 管理框架。通过图形界面管理 Bot 连接和插件运行，让你专注于插件开发。

## 截图

![插件管理](screenshots/plugins.png)

## 功能概览

### Bot 连接管理

- 配置 Milky 协议服务端的连接参数（Host、API 端口、事件端口、Token）
- 实时显示连接状态
- 断线自动重连
- 支持记住连接配置，下次启动自动连接

### 插件管理

- 图形化管理插件的启动和停止
- 插件进程隔离，单个插件崩溃不影响其他插件和主程序
- 实时查看每个插件的输出日志
- 支持插件的导入（yuyu.7z）和导出
- 记住已启用的插件，下次启动自动运行

### 日志系统

- 统一的日志查看界面
- 支持查看框架日志和插件日志
- 实时日志推送
- 支持清空日志

### 数据管理

- 每个插件拥有独立的数据目录
- 一键打开数据目录

### 插件菜单

- 插件可自行开启 Web 服务器作为配置界面
- 插件通过 API 上报菜单地址后，插件管理界面会显示“菜单”按钮，点击即可打开插件菜单

### 本地转发

- 为 Milky 协议提供本地转发代理
- 多个插件共享同一连接，减少重复请求
- 节约网络流量，降低服务端压力

### 优雅退出

- 停止插件（或主程序退出）时，会优先向插件进程发送 `Ctrl+C`（相当于 `SIGINT`）
- 给予插件最多 5 秒做清理与保存数据，期间会继续读取并显示插件的退出前输出
- 若 5 秒后仍未退出，则强制结束插件进程

### 插件间通信（计划中）

- 通过 Web 服务器实现插件之间的相互通信
- 支持插件间数据交换和协作

---

## 插件开发指南

YuyuBot 的插件是**独立的可执行程序**，可以用任何语言编写：Python、Node.js、Go、Rust、甚至是批处理脚本。

### 创建插件

在程序目录下的 `app/` 文件夹中创建一个新目录，目录名即为插件 ID：

```
app/
└── my-plugin/          # 插件ID: my-plugin
    ├── app.json        # 必需：插件描述文件
    ├── main.py         # 入口程序（示例）
    └── ...             # 其他文件
```

### app.json 配置

```json
{
  "name": "我的插件",
  "version": "1.0.0",
  "description": "这是一个示例插件",
  "entry": "python main.py",
  "author": "Your Name"
}
```

| 字段 | 必需 | 说明 |
|------|------|------|
| name | 是 | 插件显示名称 |
| version | 是 | 版本号 |
| description | 是 | 插件描述 |
| entry | 是 | 启动命令，支持带参数 |
| author | 否 | 作者 |

entry 示例：
- `main.exe` - 直接运行可执行文件
- `python main.py` - 使用系统 Python 运行
- `node index.js` - 使用 Node.js 运行

### 环境变量

YuyuBot 启动插件时会自动注入以下环境变量：

用于连接 Milky（Bot 侧）：

| 环境变量 | 说明 | 示例值 |
|----------|------|--------|
| `MILKY_HOST` | Bot 服务主机地址 | `127.0.0.1` |
| `MILKY_API_PORT` | HTTP API 端口 | `3010` |
| `MILKY_EVENT_PORT` | 事件流端口 | `3011` |
| `MILKY_TOKEN` | 鉴权 Token（如果配置了） | `your-token` |
| `YUYU_DATA_DIR` | 插件专属数据目录的绝对路径 | `C:\...\data\my-plugin` |

用于访问主程序内置插件 API（插件 → 主程序）：

| 环境变量 | 说明 | 示例值 |
|----------|------|--------|
| `YUYU_HOST` | 主程序 API 主机（固定） | `localhost` |
| `YUYU_PORT` | 主程序绑定的随机端口号 | `54321` |
| `YUYU_TOKEN` | 插件访问主程序 API 的鉴权 Token（每次启动动态生成） | `...` |

### 日志输出

插件只需向**标准输出 (stdout)** 打印内容，YuyuBot 会自动捕获并在界面中显示。无需额外配置。

```python
# Python 示例
print("插件已启动")
print(f"连接到 {os.environ['MILKY_HOST']}:{os.environ['MILKY_API_PORT']}")
```

### 数据存储

使用 `YUYU_DATA_DIR` 环境变量获取插件专属的数据目录路径，用于存储配置、缓存等持久化数据：

```python
import os
import json

data_dir = os.environ['YUYU_DATA_DIR']
config_path = os.path.join(data_dir, 'config.json')

# 读取配置
if os.path.exists(config_path):
    with open(config_path, 'r') as f:
        config = json.load(f)
```

### 连接 Bot

使用注入的环境变量构建 API 地址：

```python
import os
import requests

host = os.environ['MILKY_HOST']
api_port = os.environ['MILKY_API_PORT']
token = os.environ.get('MILKY_TOKEN', '')

# API: POST /api/:api
api_url = f"http://{host}:{api_port}/api/send_group_message"

headers = {'Content-Type': 'application/json'}
if token:
    headers['Authorization'] = f'Bearer {token}'

payload = {
    "group_id": 123456789,
    "message": [
        {"type": "text", "data": {"text": "Hello, world!"}}
    ]
}

response = requests.post(api_url, json=payload, headers=headers, timeout=10)
print(response.json())
```

上面示例使用的是 Milky 的连接信息，对应环境变量为 `MILKY_HOST`、`MILKY_API_PORT`、`MILKY_EVENT_PORT`、`MILKY_TOKEN`。

`YUYU_HOST/YUYU_PORT/YUYU_TOKEN` 是主程序提供给插件调用内置 API（例如插件菜单）的信息，不要与 Milky 连接变量混用。

### 监听事件

连接事件流端口获取实时事件：

```python
import os
import json
import urllib3
import sseclient

host = os.environ['MILKY_HOST']
event_port = os.environ['MILKY_EVENT_PORT']
token = os.environ.get('MILKY_TOKEN', '')

event_url = f"http://{host}:{event_port}/event"

headers = (
    {'Accept': 'text/event-stream', 'Authorization': f'Bearer {token}'}
    if token
    else {'Accept': 'text/event-stream'}
)

response = urllib3.PoolManager().request(
    'GET',
    event_url,
    preload_content=False,
    headers=headers,
)

client = sseclient.SSEClient(response)
for event in client.events():
    print(json.loads(event.data))
```

### 完整示例

一个简单的复读机插件：

```python
#!/usr/bin/env python3
import os
import json
import requests
import urllib3
import sseclient

# 从环境变量获取配置
HOST = os.environ['MILKY_HOST']
API_PORT = os.environ['MILKY_API_PORT']
EVENT_PORT = os.environ['MILKY_EVENT_PORT']
TOKEN = os.environ.get('MILKY_TOKEN', '')

API_URL = f"http://{HOST}:{API_PORT}/api"
EVENT_URL = f"http://{HOST}:{EVENT_PORT}/event"

def get_headers():
    headers = {}
    if TOKEN:
        headers['Authorization'] = f'Bearer {TOKEN}'
    return headers

def get_plain_text(segments):
    parts = []
    for seg in segments:
        if seg.get('type') == 'text':
            parts.append(seg.get('data', {}).get('text', ''))
    return ''.join(parts)

def send_group_message(group_id, text):
    payload = {
        "group_id": group_id,
        "message": [
            {"type": "text", "data": {"text": text}}
        ]
    }
    requests.post(
        f"{API_URL}/send_group_message",
        json=payload,
        headers=get_headers(),
        timeout=10,
    )

def main():
    print("复读机插件已启动")

    headers = (
        {'Accept': 'text/event-stream', 'Authorization': f'Bearer {TOKEN}'}
        if TOKEN
        else {'Accept': 'text/event-stream'}
    )
    response = urllib3.PoolManager().request(
        'GET',
        EVENT_URL,
        preload_content=False,
        headers=headers,
    )

    client = sseclient.SSEClient(response)
    for event in client.events():
        try:
            evt = json.loads(event.data)
        except json.JSONDecodeError:
            continue

        if evt.get('event_type') != 'message_receive':
            continue

        msg = evt.get('data', {})
        if msg.get('message_scene') != 'group':
            continue

        group_id = msg.get('peer_id')
        text = get_plain_text(msg.get('segments', []))

        if not text.startswith('/echo '):
            continue

        content = text[len('/echo '):]
        send_group_message(group_id, content)
        print(f"复读: {content}")

if __name__ == '__main__':
    main()
```

### 插件菜单 API

插件可以启动自己的 Web 服务作为配置界面，然后向主程序上报菜单入口。主程序收到上报后，会在插件管理页显示“菜单”按钮。

接口：

- `POST http://{YUYU_HOST}:{YUYU_PORT}/set_webui_port`
- Header：`Authorization: Bearer {YUYU_TOKEN}`
- Body（JSON）：

```json
{
  "webui": "/",
  "port": 12345
}
```

---

## 构建

```bash
# 开发运行
cargo run

# 发布构建
cargo build --release
```

## 系统要求

- Windows 10/11

- WebView2
