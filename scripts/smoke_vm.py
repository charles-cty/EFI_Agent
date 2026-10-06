# /// script
# dependencies = ["pyte>=0.8.2,<0.9", "cryptography>=44", "dissect.fat>=3,<4"]
# ///
"""Verify direct firmware DNS/HTTPS/SSE/tool execution with QEMU and a local provider.

No relay or API credentials are needed. DNS uses an unprivileged local TCP port.
"""
import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import socket
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import NameOID
import pyte
from dissect.fat import FATFS
from dissect.util.stream import RangeStream


class DNS(socketserver.BaseRequestHandler):
    queries = 0

    def handle(self):
        def exact(count):
            result = b""
            while len(result) < count:
                chunk = self.request.recv(count - len(result))
                if not chunk:
                    raise EOFError()
                result += chunk
            return result
        query = exact(int.from_bytes(exact(2), "big"))
        DNS.queries += 1
        response = query[:2] + b"\x81\x80\x00\x01\x00\x01\x00\x00\x00\x00" + query[12:]
        response += b"\xc0\x0c\x00\x01\x00\x01\x00\x00\x00\x3c\x00\x04\x0a\x00\x02\x02"
        self.request.sendall(len(response).to_bytes(2, "big") + response)


class DNSService(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class Provider(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    requests = []
    errors = []
    stream_started = threading.Event()
    stream_release = threading.Event()
    cancel_started = threading.Event()

    def log_message(self, *_):
        pass

    def do_POST(self):
        try:
            self.respond()
        except (BrokenPipeError, ConnectionResetError, ssl.SSLError):
            pass  # Expected when the firmware cancels or rejects a certificate.
        except Exception as error:
            Provider.errors.append(repr(error))
            self.send_error(500)

    def respond(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        Provider.requests.append(request)
        assert self.headers["Authorization"] == "Bearer smoke-key"
        assert request["model"] == "smoke-model" and request["stream"] is True
        responses = self.path == "/v1/responses"
        assert self.path in ("/v1/chat/completions", "/v1/responses")
        if responses:
            assert request["store"] is False
            assert request["include"] == ["reasoning.encrypted_content"]
            assert request["reasoning"] == {"effort": "medium", "summary": "auto"}
            messages = request["input"]
            last = messages[-1]
            prompt = last.get("content")
            if last.get("type") == "function_call_output":
                calls = [item for item in messages if item.get("type") == "function_call"]
                previous = calls[-1]
                result = last["output"]
                assert any(item.get("encrypted_content") == "OPAQUE_419" for item in messages)
            else:
                previous = None
        else:
            assert request["stream_options"] == {"include_usage": True}
            assert request["reasoning_effort"] == "medium"
            messages = request["messages"]
            last = messages[-1]
            prompt = last.get("content")
            previous = messages[-2]["tool_calls"][0] if last["role"] == "tool" else None
            if previous:
                result = last["content"]
                assert messages[-2]["reasoning_content"] == "reason 中 retained"
                assert previous["extra_content"] == {"signature": "SIGNED_419"}
        if prompt == "Quota error":
            body = b'{"error":{"message":"Insufficient quota"}}'
            self.send_response(402)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if prompt == "Close error":
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(b'{"error":{"message":"Close-delimited error"}}')
            self.close_connection = True
            return
        if prompt == "Cancel request":
            Provider.cancel_started.set()
            time.sleep(4)
        steps = [("read", {"path": "needle.txt"}),
                 ("edit", {"path": "needle.txt", "old_text": "SEED_731", "new_text": "edited 中 853"}),
                 ("write", {"path": "created.txt", "content": "created 419\n"}),
                 ("edit", {"path": "ambiguous.txt", "old_text": "aa", "new_text": "X"})]
        if previous:
            step = int((previous["call_id"] if responses else previous["id"]).split("-")[1])
            expected = ["SEED_731", "File edited", "File saved", "Tool error: old_text has multiple matches; file was not changed"][step - 1]
            assert result == expected, (step, result)
        else:
            step = 0
        call = None
        text = "DIRECT_MODEL_VERIFIED"
        if prompt == "Exercise the file tools" or previous:
            if step < len(steps):
                name, arguments = steps[step]
                call = {"id": f"call-{step+1}", "type": "function", "function": {"name": name, "arguments": json.dumps(arguments)}, "extra_content": {"signature": "SIGNED_419"}}
                text = ""
            else:
                text = "DIRECT_TOOLS_VERIFIED"
        elif prompt == "Truncated tools":
            call = {"id": "truncated", "type": "function", "function": {"name": "write", "arguments": '{"path":"must-not-exist.txt","content":"BAD"}'}}
            text = ""
        elif prompt == "Stream text":
            text = "STREAM_FIRST_731"
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()

        def event(value):
            data = ("data: " + (value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)) + "\n\n").encode()
            # Fragment UTF-8, HTTP chunks, JSON and function arguments differently.
            for offset in range(0, len(data), 7):
                chunk = data[offset:offset + 7]
                self.wfile.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
            self.wfile.flush()

        if responses:
            if text:
                event({"type": "response.output_text.delta", "delta": text})
            output = [{"type": "reasoning", "id": "rs_419", "summary": [], "encrypted_content": "OPAQUE_419"}]
            if call:
                output.append({"type": "function_call", "call_id": call["id"], **call["function"]})
            else:
                output.append({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]})
            finish = {"type": "response.completed", "response": {"status": "completed", "output": output, "usage": {"input_tokens": 120, "output_tokens": 20, "total_tokens": 140, "input_tokens_details": {"cached_tokens": 60}, "output_tokens_details": {"reasoning_tokens": 5}}}}
        else:
            delta = {"role": "assistant", "content": text, "reasoning_content": "reason 中 retained"}
            if call:
                delta["tool_calls"] = [{"index": 0, **call, "function": {"name": call["function"]["name"], "arguments": call["function"]["arguments"][:5]}}]
            event({"choices": [{"index": 0, "delta": delta, "finish_reason": None}]})
            if call:
                event({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": call["function"]["arguments"][5:]}}]}, "finish_reason": None}]})
            finish = {"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls" if call else "stop"}]}
        if prompt == "Stream text":
            Provider.stream_started.set()
            assert Provider.stream_release.wait(15)
        if prompt != "Truncated tools":
            event(finish)
            if not responses:
                event({"choices": [], "usage": {"prompt_tokens": 120, "completion_tokens": 20, "total_tokens": 140, "prompt_tokens_details": {"cached_tokens": 60}, "completion_tokens_details": {"reasoning_tokens": 5}}})
                event("[DONE]")
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()


def certificates(directory, expired=False):
    now = datetime.now(timezone.utc)
    key = ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "EFI test CA")])
    ca = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key()).serial_number(x509.random_serial_number())
          .not_valid_before(now - timedelta(days=2)).not_valid_after(now + timedelta(days=2))
          .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True).sign(key, hashes.SHA256()))
    server_key = ec.generate_private_key(ec.SECP256R1())
    server = (x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "provider.test")])).issuer_name(subject).public_key(server_key.public_key()).serial_number(x509.random_serial_number())
              .not_valid_before(now - timedelta(days=2)).not_valid_after(now - timedelta(days=1) if expired else now + timedelta(days=1))
              .add_extension(x509.SubjectAlternativeName([x509.DNSName("provider.test")]), critical=False)
              .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True).sign(key, hashes.SHA256()))
    (directory / "ca.der").write_bytes(ca.public_bytes(serialization.Encoding.DER))
    (directory / "cert.pem").write_bytes(server.public_bytes(serialization.Encoding.PEM))
    (directory / "key.pem").write_bytes(server_key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--accel", default="kvm", choices=["kvm", "whpx", "tcg"])
    parser.add_argument("--cpu", default="max", help="Guest CPU; HTTPS requires RDRAND")
    for name in ("code", "vars", "esp", "launcher", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--api-format", default="chat_completions", choices=["chat_completions", "responses"])
    parser.add_argument("--tls-failure", choices=["untrusted", "hostname", "expired", "random"])
    parser.add_argument("--http", action="store_true", help="Use plaintext HTTP to test local-only transport and clean TCP EOF")
    parser.add_argument("--driver", type=Path, help="Optional EFI boot-service driver fixture")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    Provider.requests.clear()
    Provider.errors.clear()
    Provider.stream_started.clear()
    Provider.stream_release.clear()
    with tempfile.TemporaryDirectory(prefix="efi-direct-") as temporary:
        temporary = Path(temporary)
        certificates(temporary, args.tls_failure == "expired")
        provider = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        provider.daemon_threads = True
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(temporary / "cert.pem", temporary / "key.pem")
        if not args.http:
            provider.socket = context.wrap_socket(provider.socket, server_side=True)
        dns = DNSService(("127.0.0.1", 0), DNS)
        threading.Thread(target=provider.serve_forever, daemon=True).start()
        threading.Thread(target=dns.serve_forever, daemon=True).start()
        tree = temporary / "esp"
        (tree / "EFI/BOOT").mkdir(parents=True)
        (tree / "EFI/AGENT").mkdir()
        (tree / "work").mkdir()
        (tree / "EFI/BOOT/BOOTX64.EFI").write_bytes((args.esp / "EFI/BOOT/BOOTX64.EFI").read_bytes())
        (tree / "EFI/AGENT/VM.TXT").write_text("serial\n")
        (tree / "EFI/AGENT/CA.DER").write_bytes((temporary / "ca.der").read_bytes())
        host = "wrong.test" if args.tls_failure == "hostname" else "provider.test"
        config = {"api_base": f"https://{host}:{provider.server_port}/v1", "api_key": "smoke-key", "model": "smoke-model", "api_format": args.api_format, "dns_address": [10, 0, 2, 2], "dns_port": dns.server_address[1], "workspace": "\\work"}
        if args.http:
            config["api_base"] = f"http://10.0.2.2:{provider.server_port}/v1"
        if args.tls_failure != "untrusted":
            config["ca_certificate"] = "\\EFI\\AGENT\\CA.DER"
        (tree / "EFI/AGENT/CONFIG.JSON").write_text(json.dumps(config))
        (tree / "work/needle.txt").write_text("SEED_731")
        (tree / "work/ambiguous.txt").write_text("aaa")
        if args.driver:
            (tree / "EFI/AGENT/DRIVERS").mkdir()
            (tree / "EFI/AGENT/DRIVERS/Probe.efi").write_bytes(args.driver.read_bytes())
            (tree / "EFI/AGENT/DRIVERS.JSON").write_text(json.dumps(["\\EFI\\AGENT\\DRIVERS\\Probe.efi"]), encoding="utf-8")
        image = temporary / "disk.img"
        subprocess.run([str(args.launcher.resolve()), "pack", str(tree), str(image)], check=True)
        console = socket.socket()
        console.bind(("127.0.0.1", 0))
        console.listen()
        console.settimeout(40)
        qemu = subprocess.Popen([args.qemu, "-machine", f"q35,accel={args.accel}", "-cpu", args.cpu, "-m", "256", "-display", "none", "-serial", "none", "-monitor", "none", "-no-reboot",
                                 "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
                                 "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
                                 "-drive", f"if=none,id=esp,format=raw,file={image}", "-device", "virtio-blk-pci,drive=esp", "-device", "virtio-serial-pci",
                                 "-chardev", f"socket,id=terminal,host=127.0.0.1,port={console.getsockname()[1]}", "-device", "virtconsole,chardev=terminal",
                                 "-netdev", "user,id=network", "-device", "virtio-net-pci,netdev=network"], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        wire = bytearray()
        screen = pyte.Screen(110, 40)
        stream = pyte.ByteStream(screen)
        connection = None
        try:
            connection, _ = console.accept()
            connection.settimeout(.1)

            def wait(marker, timeout=40):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    try:
                        data = connection.recv(65536)
                        if not data:
                            raise EOFError("Guest closed serial")
                        wire.extend(data)
                        stream.feed(data)
                        if b"\x1b]777;efi-agent;ready\x07" in data:
                            connection.sendall(b"\x1b[8;40;110t")
                    except socket.timeout:
                        pass
                    visible = "\n".join(screen.display)
                    if marker in visible:
                        return visible
                    if Provider.errors:
                        raise AssertionError(Provider.errors)
                raise TimeoutError(f"Missing {marker}:\n" + "\n".join(screen.display))

            def send(text):
                connection.sendall(text.encode() + b"\r")

            wait("What would you like to build?")
            if args.driver:
                send("/caps")
                wait("Extra drivers started: 1")
                send("/clear")
                wait("Conversation cleared")
            send("Stream text")
            if args.tls_failure:
                wait("TLS random generator: Unsupported" if args.tls_failure == "random" else "TLS:")
                assert not Provider.requests
                print(f"PASS firmware rejects TLS {args.tls_failure} before sending credentials", flush=True)
            else:
                wait("STREAM_FIRST_731")
                assert Provider.stream_started.is_set()
                connection.sendall("\x1b[200~draft 中\nsecond\x1b[201~".encode())
                wait("second")
                Provider.stream_release.set()
                wait("Ready  •")
                connection.sendall(b"\x01" + b"\x7f" * 50)  # Clear the draft with Home/backspace below.
                connection.sendall(b"\x1b[F" + b"\x7f" * 50)
                send("/clear")
                wait("Conversation cleared")
                send("Exercise the file tools")
                wait("DIRECT_TOOLS_VERIFIED", 70)
                send("/status")
                wait("50.00%")
                send("/clear")
                wait("Conversation cleared")
                send("Quota error")
                wait("Provider HTTP 402: Insufficient quota")
                send("/clear")
                wait("Conversation cleared")
                if args.http:
                    send("Close error")
                    wait("Provider HTTP 400: Close-delimited error")
                    send("/clear")
                    wait("Conversation cleared")
                send("Truncated tools")
                wait("stream ended")
                send("/clear")
                wait("Conversation cleared")
                send("Cancel request")
                deadline = time.monotonic() + 20
                while not Provider.cancel_started.is_set() and time.monotonic() < deadline:
                    time.sleep(.02)
                assert Provider.cancel_started.is_set()
                connection.sendall(b"\x1b")
                wait("Request cancelled")
                send("Next request")
                wait("DIRECT_MODEL_VERIFIED")
                assert args.http or DNS.queries > 0
                transport = "HTTP with clean TCP EOF" if args.http else "DNS and verified HTTPS"
                print(f"PASS direct {transport}, {args.api_format} streaming, draft edits, tools, usage, truncation, cancellation and next request", flush=True)
            send("/exit")
            qemu.wait(timeout=15)
            assert qemu.returncode == 0
            if not args.tls_failure:
                with image.open("rb") as disk:
                    fs = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    for name, expected in [("needle.txt", "edited 中 853".encode()), ("created.txt", b"created 419\n"), ("ambiguous.txt", b"aaa")]:
                        assert fs.get(f"/work/{name}").open().read() == expected
                    try:
                        fs.get("/work/must-not-exist.txt")
                    except FileNotFoundError:
                        pass
                    else:
                        raise AssertionError("Truncated tool call wrote a file")
                    if args.driver:
                        assert fs.get("/work/driver-started.txt").open().read() == b"1"
                print("PASS actual FAT file bytes and no truncated tool execution", flush=True)
        finally:
            Provider.stream_release.set()
            if qemu.poll() is None:
                qemu.terminate()
            _, errors = qemu.communicate(timeout=15)
            (args.output / "qemu.log").write_bytes(errors)
            (args.output / "serial.bin").write_bytes(wire)
            (args.output / "screen.txt").write_text("\n".join(screen.display), encoding="utf-8")
            if connection:
                connection.close()
            console.close()
            provider.shutdown()
            provider.server_close()
            dns.shutdown()
            dns.server_close()


if __name__ == "__main__":
    main()
