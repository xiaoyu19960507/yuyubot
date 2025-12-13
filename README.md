# YuyuBot

YuyuBot 是一个为 Milky 协议设计的桌面端 Bot 管理框架。通过图形界面管理 Bot 连接和插件运行，让你专注于插件开发。

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
- 支持插件的导入（zip）和导出
- 记住已启用的插件，下次启动自动运行

### 日志系统

- 统一的日志查看界面
- 支持查看框架日志和插件日志
- 实时日志推送
- 支持清空日志

### 数据管理

- 每个插件拥有独立的数据目录
- 一键打开数据目录

### 本地转发（计划中）

- 为 Milky 协议提供本地转发代理
- 多个插件共享同一连接，减少重复请求
- 节约网络流量，降低服务端压力

### 插件菜单（计划中）

- 插件可自行开启 Web 服务器作为配置界面
- 在插件管理界面中直接访问插件菜单

### 优雅退出（计划中）

- 当插件开启了 Web 服务器时，程序退出前会向插件发送退出请求
- 给予插件清理资源和保存数据的机会

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

YuyuBot 启动插件时会自动注入以下环境变量，用于连接 Bot：

| 环境变量 | 说明 | 示例值 |
|----------|------|--------|
| `YUYU_HOST` | Bot 服务主机地址 | `127.0.0.1` |
| `YUYU_API_PORT` | HTTP API 端口 | `3010` |
| `YUYU_EVENT_PORT` | 事件流端口 | `3011` |
| `YUYU_TOKEN` | 鉴权 Token（如果配置了） | `your-token` |
| `YUYU_DATA_DIR` | 插件专属数据目录的绝对路径 | `C:\...\data\my-plugin` |

### 日志输出

插件只需向**标准输出 (stdout)** 打印内容，YuyuBot 会自动捕获并在界面中显示。无需额外配置。

```python
# Python 示例
print("插件已启动")
print(f"连接到 {os.environ['YUYU_HOST']}:{os.environ['YUYU_API_PORT']}")
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

> 注意，此示例代码由AI编写，而AI并不了解Milky，请谨慎查看

使用注入的环境变量构建 API 地址：

```python
import os
import requests

host = os.environ['YUYU_HOST']
api_port = os.environ['YUYU_API_PORT']
token = os.environ.get('YUYU_TOKEN', '')

# 构建 API URL
api_url = f"http://{host}:{api_port}/api"

# 发送请求
headers = {}
if token:
    headers['Authorization'] = f'Bearer {token}'

response = requests.post(f"{api_url}/send_message", json={
    "group_id": 123456,
    "message": "Hello!"
}, headers=headers)
```

### 监听事件

> 注意，此示例代码由AI编写，而AI并不了解Milky，请谨慎查看

连接事件流端口获取实时事件：

```python
import os
import requests

host = os.environ['YUYU_HOST']
event_port = os.environ['YUYU_EVENT_PORT']
token = os.environ.get('YUYU_TOKEN', '')

event_url = f"http://{host}:{event_port}/event"

headers = {'Accept': 'text/event-stream'}
if token:
    headers['Authorization'] = f'Bearer {token}'

# SSE 事件流
response = requests.get(event_url, headers=headers, stream=True)
for line in response.iter_lines():
    if line:
        line = line.decode('utf-8')
        if line.startswith('data: '):
            data = line[6:]
            print(f"收到事件: {data}")
```

### 完整示例

> 注意，此示例代码由AI编写，而AI并不了解Milky，请谨慎查看

一个简单的复读机插件：

```python
#!/usr/bin/env python3
import os
import json
import requests

# 从环境变量获取配置
HOST = os.environ['YUYU_HOST']
API_PORT = os.environ['YUYU_API_PORT']
EVENT_PORT = os.environ['YUYU_EVENT_PORT']
TOKEN = os.environ.get('YUYU_TOKEN', '')

API_URL = f"http://{HOST}:{API_PORT}/api"
EVENT_URL = f"http://{HOST}:{EVENT_PORT}/event"

def get_headers():
    headers = {}
    if TOKEN:
        headers['Authorization'] = f'Bearer {TOKEN}'
    return headers

def send_group_message(group_id, message):
    requests.post(f"{API_URL}/send_group_msg", json={
        "group_id": group_id,
        "message": message
    }, headers=get_headers())

def main():
    print("复读机插件已启动")
    
    headers = get_headers()
    headers['Accept'] = 'text/event-stream'
    
    response = requests.get(EVENT_URL, headers=headers, stream=True)
    
    for line in response.iter_lines():
        if not line:
            continue
        
        line = line.decode('utf-8')
        if not line.startswith('data: '):
            continue
        
        try:
            event = json.loads(line[6:])
            
            # 处理群消息
            if event.get('post_type') == 'message' and event.get('message_type') == 'group':
                group_id = event['group_id']
                message = event['raw_message']
                
                # 复读
                if message.startswith('/echo '):
                    content = message[6:]
                    send_group_message(group_id, content)
                    print(f"复读: {content}")
        
        except json.JSONDecodeError:
            pass

if __name__ == '__main__':
    main()
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
