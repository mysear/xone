from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class DevHandler(SimpleHTTPRequestHandler):
    def send_head(self):
        # Replace responses cached before the development server sent no-store.
        del self.headers["If-Modified-Since"]
        return super().send_head()

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


if __name__ == "__main__":
    handler = partial(DevHandler, directory=str(Path(__file__).resolve().parent))
    with ThreadingHTTPServer(("127.0.0.1", 5173), handler) as server:
        print("Serving XONE at http://127.0.0.1:5173", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
