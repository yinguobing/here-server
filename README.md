# 我在这里 (I Am Here)

鸿蒙APP后端服务。接收鸿蒙设备上报的 GPS 定位数据，供智能体读取。

## API

### POST /location

接收定位数据。

**Headers:**
- `Content-Type: application/json`
- `X-Location-Token: <your-token>`

**Body:**
```json
{
  "lat": 23.19,
  "lon": 113.47,
  "timestamp": 1776854363,
  "source": "harmonyos"
}
```

**Response:**
```json
{"ok": true, "count": 42}
```

### GET /health

健康检查，返回 `ok`。

## 部署

```bash
python3 location_receiver.py
```

默认端口 9001，可通过 `PORT` 环境变量修改。
