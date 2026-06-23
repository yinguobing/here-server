# 我在这里 (I Am Here)

鸿蒙 App「我在这里」的后端服务。接收设备上报的 GPS 定位数据，基于 SurrealDB 存储，支持多用户独立 Token 和数据隔离。

> App 下载与使用说明：[yinguobing.com/tools/footprint-ohos](https://yinguobing.com/tools/footprint-ohos/)

## 用户管理

首次使用需创建用户（获得独立 Token）：

```bash
# 创建用户
./manage add-user "你的名字"
# → 输出 ID、Name、Token

# 查看所有用户
./manage list-users

# 轮换 Token
./manage rotate-token <用户ID>

# 删除用户（含其所有数据）
./manage delete-user <用户ID>
```

## API

### POST /location

上报定位数据。

**Headers**

| Header | 说明 |
|---|---|
| `Content-Type` | `application/json`（必填） |
| `Authorization` | `Bearer <token>`，推荐使用 |
| `X-Location-Token` | 兼容旧版，直接传 Token 值 |

> 两个 Header 二选一即可。`Authorization` 优先。

**Body（JSON）**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `lat` | number | ✅ | 纬度（WGS84），范围 [-90, 90] |
| `lon` | number | ✅ | 经度（WGS84），范围 [-180, 180] |
| `timestamp` | number | ✅ | Unix 时间戳（秒） |
| `source` | string | ✅ | 固定值 `"harmonyos"` |
| `accuracy` | number | ❌ | 定位精度（米），不可用时为 -1 |
| `altitude` | number | ❌ | 海拔（米），不可用时为 -1 |
| `speed` | number | ❌ | 速度（m/s），不可用时为 -1 |

请求示例：

```json
{
  "lat": 23.190664,
  "lon": 113.470556,
  "timestamp": 1776854363,
  "source": "harmonyos",
  "accuracy": 10.5,
  "altitude": 42.0,
  "speed": 0.0
}
```

**响应**

| 状态码 | 含义 |
|---|---|
| 200 | 上报成功 |
| 400 | 请求格式错误（缺少必填字段 / Content-Type 不对） |
| 401 | Token 无效 |
| 404 | 路径不存在 |

成功响应 Body：

```json
{"ok": true, "count": 42}
```

> `count` 为当前存储的定位记录总数。

### GET /location

查询定位记录，供下游智能体读取。需鉴权。

**Query 参数**

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `limit` | number | `50` | 返回最近 N 条记录 |

请求示例：

```bash
curl -H "Authorization: Bearer <token>" \
  "http://localhost:9001/location?limit=10"
```

响应：按时间升序的最近 N 条记录：

```json
[
  {
    "lat": 23.19,
    "lon": 113.47,
    "timestamp": 1782140344,
    "source": "harmonyos",
    "accuracy": 10.5,
    "altitude": 42.0,
    "speed": 0.0,
    "received_at": "2026-06-22T14:59:04.121+00:00"
  }
]
```

### GET /health

健康检查，返回 `ok`。

## 配置

| 环境变量 | 默认值 | 说明 |
|---|---|---|
| `PORT` | `9001` | 监听端口 |
| `DATA_DIR` | `/var/lib/i-am-here` | 数据库持久化目录 |
| `MAX_HOURS` | `24` | 定位记录保留时长 |
| `LOCATION_TOKEN` | — | 向后兼容：设置后自动创建 admin 用户 |

## 数据存储

SurrealDB 嵌入式数据库，通过 `DATA_DIR` 指定持久化目录（默认 `/var/lib/i-am-here`）。自动清理超过 `MAX_HOURS`（默认 24 小时）的旧记录。

## 部署

### 方式一：deb 包安装（推荐）

从 [Releases](https://github.com/yinguobing/i-am-here/releases) 下载 deb 包：

```bash
sudo dpkg -i i-am-here_0.1.0-1_amd64.deb
```

安装后自动创建 `/etc/i-am-here/env` 并启动服务。**启动后创建用户：**

```bash
# 获得用户 Token
manage add-user "你的名字"

# 将输出的 Token 填入 App 设置页
```

### 服务管理

```bash
systemctl status i-am-here   # 查看状态
systemctl restart i-am-here  # 重启（修改配置后）
journalctl -u i-am-here -f   # 查看日志
manage list-users            # 查看所有用户
manage add-user "name"       # 新增用户
```

### 方式二：从源码编译

```bash
# 1. 编译（输出两个二进制：i-am-here、manage）
cargo build --release

# 2. 启动服务
export DATA_DIR=/var/lib/i-am-here
export PORT=9001
./target/release/i-am-here &

# 3. 创建用户
./target/release/manage add-user "你的名字"
```

### 打包 deb

```bash
cargo install cargo-deb
cargo deb
# 输出：target/debian/i-am-here_*.deb
```

## Nginx 反向代理（可选）

生产环境建议前置 Nginx：

```nginx
server {
    listen 443 ssl;
    server_name your-domain.com;

    location / {
        proxy_pass http://127.0.0.1:9001;
    }
}
```
