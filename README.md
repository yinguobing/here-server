# 我在这里 (I Am Here)

鸿蒙 App「我在这里」的定位数据接收后端。接收设备上报的 GPS 定位数据，存入本地 JSON 文件，供下游智能体或服务读取。

> App 下载与使用说明：[yinguobing.com/tools/footprint-ohos](https://yinguobing.com/tools/footprint-ohos/)

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
| `LOCATION_TOKEN` | `change-me-to-a-secret-token` | API 鉴权 Token，**部署时务必修改** |

## 数据存储

定位数据保存在 `/tmp/location.json`，自动清理 24 小时前的记录，结构如下：

```json
{
  "locations": [
    {
      "lat": 23.190664,
      "lon": 113.470556,
      "timestamp": 1776854363,
      "source": "harmonyos",
      "accuracy": 10.5,
      "altitude": 42.0,
      "speed": 0.0,
      "received_at": "2026-06-22T14:30:00.123456+00:00"
    }
  ]
}
```

## 部署

### 方式一：deb 包安装（推荐）

从 [Releases](https://github.com/yinguobing/i-am-here/releases) 下载 deb 包：

```bash
sudo dpkg -i i-am-here_0.1.0-1_amd64.deb
```

安装后自动创建 `/etc/i-am-here/env` 并启动服务。**立即修改 Token：**

```bash
sudo vim /etc/i-am-here/env
# 修改 LOCATION_TOKEN 为你的密钥
sudo systemctl restart i-am-here
```

### 服务管理

```bash
systemctl status i-am-here   # 查看状态
systemctl restart i-am-here  # 重启（修改配置后）
journalctl -u i-am-here -f   # 查看日志
```

### 方式二：从源码编译

```bash
# 1. 编译
cargo build --release

# 2. 启动
export LOCATION_TOKEN="your-secret-token"
export PORT=9001
./target/release/i-am-here
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
