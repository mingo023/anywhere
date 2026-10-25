"""A push relay for scripts/mobile-e2e.sh: takes /v1/send as the real relay does, checks its signature, and logs it.

usage: fake-push-relay.py <port> <log>
A handle here is "<name>.<secret>", so the relay can check X-Anywhere-Signature without sealing anything. Each send is
one JSON line in <log>: {"signed": bool, "body": <the send>}. Every send answers 200.
"""

import base64
import hashlib
import hmac
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


def signed(handle, ts, sig, body):
    secret = handle.rsplit(".", 1)[-1]
    key = base64.urlsafe_b64decode(secret + "=" * (-len(secret) % 4))
    want = hmac.new(key, ts.encode() + b"." + body, hashlib.sha256).digest()
    return hmac.compare_digest(base64.urlsafe_b64encode(want).rstrip(b"=").decode(), sig)


class Relay(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        send = json.loads(body)
        ok = self.path == "/v1/send" and signed(
            send["handle"], self.headers.get("X-Anywhere-Timestamp", ""), self.headers.get("X-Anywhere-Signature", ""), body
        )
        with open(sys.argv[2], "a") as log:
            log.write(json.dumps({"signed": ok, "body": send}) + "\n")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"apnsId":"fake-apns"}')

    def log_message(self, *args):
        pass


HTTPServer(("127.0.0.1", int(sys.argv[1])), Relay).serve_forever()
