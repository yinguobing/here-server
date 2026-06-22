#!/usr/bin/env python3
"""
Location history receiver.
POST /location {"lat": float, "lon": float, "timestamp": int, "source": "harmonyos"}
Token: X-Location-Token header
"""

import json
import os
from datetime import datetime, timezone, timedelta
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path

# Config
TOKEN = os.environ.get("LOCATION_TOKEN", "change-me-to-a-secret-token")
DATA_FILE = Path("/tmp/location.json")
MAX_HOURS = 24  # keep last 24 hours


def load_data():
    if DATA_FILE.exists():
        try:
            with open(DATA_FILE) as f:
                return json.load(f)
        except (json.JSONDecodeError, IOError):
            return {"locations": []}
    return {"locations": []}


def save_data(data):
    DATA_FILE.write_text(json.dumps(data, ensure_ascii=False, indent=2))


def prune_old(data):
    cutoff = datetime.now(timezone.utc) - timedelta(hours=MAX_HOURS)
    cutoff_ts = int(cutoff.timestamp())
    data["locations"] = [
        loc for loc in data["locations"] if loc.get("timestamp", 0) > cutoff_ts
    ]
    return data


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"[{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}] {args[0]}")

    def do_POST(self):
        # Token check
        token = self.headers.get("X-Location-Token", "")
        if token != TOKEN:
            self.send_error(401, "Unauthorized")
            return

        if self.path != "/location":
            self.send_error(404, "Not Found")
            return

        if self.headers.get("Content-Type", "") != "application/json":
            self.send_error(400, "Content-Type must be application/json")
            return

        try:
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length).decode("utf-8")
            payload = json.loads(body)
        except (ValueError, IOError) as e:
            self.send_error(400, f"Bad request: {e}")
            return

        lat = payload.get("lat")
        lon = payload.get("lon")
        timestamp = payload.get("timestamp")
        source = payload.get("source", "unknown")

        if lat is None or lon is None or timestamp is None:
            self.send_error(400, "Missing lat, lon, or timestamp")
            return

        data = load_data()
        data["locations"].append({
            "lat": lat,
            "lon": lon,
            "timestamp": timestamp,
            "source": source,
            "received_at": datetime.now(timezone.utc).isoformat(),
        })
        data = prune_old(data)
        save_data(data)

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps({"ok": True, "count": len(data["locations"])}).encode())

    def do_GET(self):
        # Simple health check
        if self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"ok")
            return
        self.send_error(404, "Not Found")


if __name__ == "__main__":
    import sys
    port = int(os.environ.get("PORT", 9001))
    server = HTTPServer(("0.0.0.0", port), Handler)
    print(f"Location receiver running on port {port}")
    server.serve_forever()
