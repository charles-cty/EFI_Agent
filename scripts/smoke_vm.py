# /// script
# dependencies = ["pyte>=0.8.2,<0.9", "cryptography>=44", "dissect.fat>=3,<4", "asyncssh>=2.21"]
# ///
"""Verify direct firmware DNS/HTTPS/SSE/tool execution with QEMU and a local provider.

No relay or API credentials are needed. DNS uses an unprivileged local TCP port.
"""
import argparse
from contextlib import ExitStack
from datetime import datetime, timedelta, timezone
import json
import re
import shutil
from pathlib import Path
import socket
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time
from textwrap import dedent
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from wasm_time_fixture import TIME_CHECK
from wasm_basic_applets_fixture import BASIC_APPLETS_CHECK
from wasm_ls_fixture import LS_CHECK
from wasm_mutation_applets_fixture import MUTATION_APPLETS_CHECK
from test_busybox_mutations import verify_mutation_applets
from wasm_date_fixture import DATE_CHECK
from test_busybox_date import verify_date
from wasm_text_applets_fixture import TEXT_APPLETS_CHECK
from test_busybox_text import verify_text_applets
from wasm_base64_fixture import BASE64_CHECK
from test_busybox_base64 import verify_base64
from wasm_sha256_fixture import SHA256_CHECK
from test_busybox_sha256 import verify_sha256
from wasm_realpath_fixture import REALPATH_CHECK
from test_busybox_realpath import verify_realpath
from wasm_stat_fixture import STAT_CHECK
from wasm_program_paths_fixture import PROGRAM_PATHS_CHECK
from wasm_copy_move_fixture import COPY_MOVE_CHECK
from test_busybox_copy_move import verify_copy_move
from wasm_xargs_fixture import XARGS_CHECK
from test_busybox_xargs import verify_xargs
from wasm_compression_fixture import COMPRESSION_CHECK
from test_busybox_compression import verify_compression
from wasm_unzip_fixture import UNZIP_CHECK
from test_busybox_unzip import verify_unzip
from wasm_du_fixture import DU_CHECK
from test_busybox_du import verify_du
from wasm_ln_fixture import LN_CHECK
from test_busybox_ln import verify_ln
from wasm_df_fixture import DF_CHECK
from test_busybox_df import verify_df
from test_busybox_stat import verify_stat
from wasm_tar_fixture import TAR_CHECK
from test_busybox_tar import verify_tar
from wasm_temporary_fixture import TEMPORARY_CHECK, verify_temporary
from wasm_mount_paths_fixture import MOUNT_PATHS_CHECK, verify_mount_paths
from wasm_character_devices_fixture import CHARACTER_DEVICES_CHECK, verify_character_devices
from wasm_process_files_fixture import PROCESS_FILES_CHECK, verify_process_files
from wasm_cwd_fixture import CWD_CHECK, verify_cwd
from wasm_c_port_cwd_fixture import c_port_cwd_check, verify_c_port_cwd
from wasm_executable_freshness_fixture import (
    EXECUTABLE_FRESHNESS_CHECK, executable_image, verify_executable_freshness,
)
from wasm_installation_fixture import INSTALLATION_CHECK, verify_installation
from wasm_sessions_fixture import SESSION_STEPS, CLEAR_STEPS, verify_sessions

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import NameOID
import pyte
from dissect.fat import FATFS
from dissect.util.stream import RangeStream
from wasm_namespace_fixture import FatWorkspace


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
    code_steps = [
        ("shell", {"command": "printf '%s\\n' 'a b' '中' > code-shell.txt\necho shell-out; echo shell-err >&2"}),
        ("shell", {"command": "export TOOL_SESSION_VALUE=retained; echo session-ash"}),
        ("shell", {"command": "test \"$TOOL_SESSION_VALUE\" = retained || exit 9; echo retained-ash"}),
        ("python", {"code": "import subprocess\nfrom pathlib import Path\nr = subprocess.run(\"printf '%s' 'python child 中'\", shell=True, capture_output=True, text=True, check=True)\nPath('code-python.txt').write_text(r.stdout)\nprint(r.stdout)"}),
        ("shell", {"command": "echo failure-out; echo failure-err >&2; exit 7"}),
        ("python", {"code": "raise ValueError('code-error-419')"}),
        ("python", {"code": "import sys\nsys.stdout.write('A' * 65536)\nsys.stderr.write('B' * 65537)"}),
        ("shell", {"command": "echo forbidden\u0000"}),
        ("python", {"code": "print('retained-interpreter', 'subprocess' in globals())"}),
    ]
    code_expected = ["shell-out", "session-ash", "retained-ash", "python child 中", "Exit code: 7", "code-error-419", "[Output truncated at 64 KiB]", "Tool error: Code must contain no NUL", "retained-interpreter True"]
    filesystem_tools = False

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
        tools = request["tools"]
        names = {tool["name"] if responses else tool["function"]["name"] for tool in tools}
        assert names == {"read", "write", "edit", "shell", "python"}, names
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
        code_previous = previous and (previous["call_id"] if responses else previous["id"]).startswith("code-")
        if previous and not code_previous:
            step = int((previous["call_id"] if responses else previous["id"]).split("-")[1])
            expected = ["SEED_731", "File edited", "File saved", "Tool error: old_text has multiple matches; file was not changed"][step - 1]
            assert result == expected, (step, result)
        else:
            step = 0
        call = None
        text = "DIRECT_MODEL_VERIFIED"
        if prompt == "Exercise code tools" or code_previous:
            step = int((previous["call_id"] if responses else previous["id"]).split("-")[1]) if previous else 0
            if previous:
                expected = Provider.code_expected[step - 1]
                assert expected in result, (step, result)
                if expected == "[Output truncated at 64 KiB]":
                    assert result.count("[Output truncated at 64 KiB]") == 1
                    assert "A" * 65536 in result and "B" * 65536 in result
                if expected == "Tool error: Code must contain no NUL":
                    assert "forbidden\n" not in result
            if step < len(Provider.code_steps):
                name, arguments = Provider.code_steps[step]
                call = {"id": f"code-{step+1}", "type": "function", "function": {"name": name, "arguments": json.dumps(arguments)}, "extra_content": {"signature": "SIGNED_419"}}
                text = ""
            else:
                text = "CODE_TOOLS_VERIFIED"
        elif prompt == "Cancel Python code tool":
            code = "import subprocess\np = subprocess.Popen(['sleep', '100'])\np.wait()"
            call = {"id": "cancel-code", "type": "function", "function": {"name": "python", "arguments": json.dumps({"code": code})}, "extra_content": {"signature": "SIGNED_419"}}
            text = ""
        elif prompt == "Cancel code tool":
            command = "echo started > code-cancel-started.txt; (sleep 30) & wait" if Provider.filesystem_tools else "echo started > code-cancel-started.txt; (while :; do :; done) & wait"
            call = {"id": "cancel-code", "type": "function", "function": {"name": "shell", "arguments": json.dumps({"command": command})}, "extra_content": {"signature": "SIGNED_419"}}
            text = ""
        elif prompt == "Exercise the file tools" or previous:
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


def run(services):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--accel", default="kvm", choices=["kvm", "whpx", "tcg"])
    parser.add_argument("--cpu", default="max", help="Guest CPU; HTTPS requires RDRAND")
    parser.add_argument("--memory", type=int, default=256, help="Guest RAM in MiB")
    for name in ("code", "vars", "esp", "launcher", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--api-format", default="chat_completions", choices=["chat_completions", "responses"])
    parser.add_argument("--tls-failure", choices=["untrusted", "hostname", "expired", "random"])
    parser.add_argument("--http", action="store_true", help="Use plaintext HTTP to test local-only transport and clean TCP EOF")
    parser.add_argument("--driver", type=Path, help="Optional EFI boot-service driver fixture")
    parser.add_argument("--code-workspace", type=Path, help="Stage ash, Python and its library; exercise real model code tools")
    parser.add_argument("--c-port-cwd", action="store_true", help="Verify shared C cwd support and real curl, QuickJS and WolfSSH relative paths")
    parser.add_argument("--cwd-probe", type=Path, help="C cwd fixture binary; required with --c-port-cwd")
    parser.add_argument("--executable-freshness", action="store_true", help="Verify live and cached executable images across source mutation")
    parser.add_argument("--installation", action="store_true", help="Verify manifest-installed commands, libraries and licenses")
    parser.add_argument("--sessions", action="store_true", help="Verify consecutive live shell/Python tool state and clear")
    parser.add_argument("--code-timeout", type=float, default=480, help="Maximum seconds for the complete code-tool sequence")
    parser.add_argument("--code-cancel-only", action="store_true", help="Run only code cancellation/isolation checks; requires --code-workspace")
    parser.add_argument("--filesystem-tools", action="store_true", help="Verify nested file-tool writes, mkdir, large writes and finite Python stdin")
    parser.add_argument("--text-applets", action="store_true", help="Run upstream basic and text applet WHPX checks")
    parser.add_argument("--base64-applet", action="store_true", help="Run upstream basic and base64 applet WHPX checks")
    parser.add_argument("--sha256-applet", action="store_true", help="Run upstream basic and SHA-256 applet WHPX checks")
    parser.add_argument("--realpath-applet", action="store_true", help="Run upstream basic and realpath applet WHPX checks")
    parser.add_argument("--stat-applet", action="store_true", help="Run upstream basic and stat applet WHPX checks")
    parser.add_argument("--ls-applet", action="store_true", help="Run upstream basic and ls applet WHPX checks")
    parser.add_argument("--find-applet", action="store_true", help="Run upstream basic and find applet WHPX checks")
    parser.add_argument("--program-paths", action="store_true", help="Verify guest PATH and shebang dispatch through WHPX")
    parser.add_argument("--copy-move-applets", action="store_true", help="Verify upstream cp/mv and independently saved filesystem effects")
    parser.add_argument("--xargs-applet", action="store_true", help="Verify upstream xargs parsing, concurrent children and saved bytes")
    parser.add_argument("--compression-applets", action="store_true", help="Verify upstream gzip/gunzip/zcat and independently decode saved output")
    parser.add_argument("--unzip-applet", action="store_true", help="Verify upstream unzip integrity, containment and saved metadata")
    parser.add_argument("--du-applet", action="store_true", help="Verify upstream du and independently count saved FAT clusters")
    parser.add_argument("--ln-applet", action="store_true", help="Verify upstream ln and independently inspect saved hard-link groups")
    parser.add_argument("--df-applet", action="store_true", help="Verify upstream df with real mount statistics and independent FAT capacity")
    parser.add_argument("--tar-applet", action="store_true", help="Verify upstream tar streams, containment, links and saved archives")
    parser.add_argument("--temporary-mount", action="store_true", help="Verify shared temporary inodes, quotas, mutations and saved workspace exclusion")
    parser.add_argument("--mount-paths", action="store_true", help="Verify cross-mount links, parent traversal and directory bounds")
    parser.add_argument("--character-devices", action="store_true", help="Verify non-terminal null/zero/random devices and independent saved bytes")
    parser.add_argument("--process-files", action="store_true", help="Verify process metadata, descriptor links, pipe lifetime and saved bytes")
    parser.add_argument("--cwd", action="store_true", help="Verify retained cwd identity through rename/removal and standard Python/ash APIs")
    args = parser.parse_args()
    if args.code_cancel_only and not args.code_workspace:
        parser.error("--code-cancel-only requires --code-workspace")
    if args.filesystem_tools:
        if not args.code_workspace or args.code_cancel_only:
            parser.error("--filesystem-tools requires --code-workspace and excludes --code-cancel-only")
        Provider.filesystem_tools = True
        Provider.code_steps = [
            ("write", {"path": "scripts/deep/input.py", "content": "print('NESTED_SCRIPT_OK')\n"}),
            ("shell", {"command": "python scripts/deep/input.py"}),
            ("python", {"code": dedent("""
                import os
                os.makedirs('data/space dir')
                payload = bytes(range(256)) * 32768
                for size in (65535, 65536, 65537, 524287, 524288, 524289, 8388608):
                    path = 'data/space dir/write-' + str(size)
                    fd = os.open(path, os.O_WRONLY | os.O_CREAT)
                    assert os.write(fd, payload[:size]) == size
                    os.close(fd)
                    offset = 0
                    with open(path, 'rb') as source:
                        while block := source.read(65536):
                            assert block == payload[offset:offset + len(block)]
                            offset += len(block)
                    assert offset == size
                print('FILESYSTEM_WRITES_OK')
                """)}),
            ("shell", {"command": "python - <<'PY'\nimport os\nassert not os.isatty(0)\nprint('HEREDOC_SCRIPT_OK')\nPY\n"}),
            ("python", {"code": "import os, subprocess\nassert not any(os.isatty(fd) for fd in (0, 1, 2))\nfor source, expected in [(b'', b''), (b'print(731)\\n', b'731\\n')]:\n    r = subprocess.run(['python', '-'], input=source, capture_output=True)\n    assert (r.returncode, r.stdout, r.stderr) == (0, expected, b''), r\nprint('FINITE_STDIN_OK')"}),
            ("python", {"code": dedent("""
                import errno, os, stat, subprocess
                os.makedirs('mutation/empty')
                with open('mutation/original', 'wb') as f:
                    f.write(b'original bytes')
                with open('mutation/replaced', 'wb') as f:
                    f.write(b'old destination')
                old = open('mutation/replaced', 'rb')
                os.rename('mutation/original', 'mutation/replaced')
                assert old.read() == b'old destination'
                old.close()
                os.symlink('replaced', 'mutation/link')
                assert os.readlink('mutation/link') == 'replaced'
                assert open('mutation/link', 'rb').read() == b'original bytes'
                retained = open('mutation/replaced', 'rb')
                os.unlink('mutation/replaced')
                assert retained.read() == b'original bytes'
                retained.close()
                assert not os.path.exists('mutation/link') and os.path.lexists('mutation/link')
                os.rmdir('mutation/empty')
                os.rename('mutation', 'moved-mutation')
                with open('moved-mutation/result', 'w') as f:
                    f.write('MUTATION_SHARED_OK\\n')
                print('MUTATION_IDENTITY_OK')
                os.chmod('moved-mutation/result', 0o400)
                try:
                    open('moved-mutation/result', 'wb')
                except PermissionError as error:
                    assert error.errno == errno.EACCES
                else:
                    raise AssertionError('Read-only mode did not deny a new writer')
                os.chmod('moved-mutation/result', 0o640)
                fd = os.open('moved-mutation', os.O_RDONLY)
                with os.scandir(fd) as entries:
                    entry = next(item for item in entries if item.name == 'result')
                os.close(fd)
                assert stat.S_IMODE(entry.stat().st_mode) == 0o640
                os.chdir('moved-mutation')
                subprocess.run('chmod u=rw,g=r,o= result; sed -i.bak s/SHARED/SHARED/ result', shell=True, check=True)
                assert stat.S_IMODE(os.stat('result').st_mode) == 0o640
                assert stat.S_IMODE(os.stat('result.bak').st_mode) == 0o640
                assert os.umask(0o077) == 0o022
                os.symlink('new-mode-target', 'create-link')
                fd = os.open('create-link', os.O_WRONLY | os.O_CREAT, 0o400)
                os.close(fd)
                assert stat.S_IMODE(os.stat('new-mode-target').st_mode) == 0o400
                subprocess.run('test "$(umask)" = 0077; : > mask-file; mkdir mask-dir', shell=True, check=True)
                assert stat.S_IMODE(os.stat('mask-file').st_mode) == 0o600
                assert stat.S_IMODE(os.stat('mask-dir').st_mode) == 0o700
                os.umask(0o022)
                os.chdir('/')
                print('PERMISSIONS_CWD_OK')
                """)}),
            ("shell", {"command": "read -r result < moved-mutation/result; printf '%s\\n' \"$result\""}),
            ("python", {"code": dedent("""
                import subprocess
                payload = (bytes(range(256)) * 2050)[:524289]
                long_line = b'x' * 1100003
                for path, data in [('stream-binary', payload), ('stream-long', long_line),
                                   ('stream-lines', b'a\\0b\\nsecond\\nlast\\0tail')]:
                    with open(path, 'wb') as output:
                        output.write(data)
                cases = [
                    ('cat stream-binary', payload, 0),
                    ('cat -n stream-lines', b'     1\\ta\\0b\\n     2\\tsecond\\n     3\\tlast\\0tail', 0),
                    ('head -n -1 stream-lines', b'a\\0b\\nsecond\\n', 0),
                    ('tail -n 1 stream-long', long_line, 0),
                    ('tail -c 65537 stream-binary', payload[-65537:], 0),
                    ('cat stream-binary | tee stream-copy | wc -c', None, 0),
                    ('yes | head -c 196613', (b'y\\n' * 98307)[:196613], 0),
                    ('head moved-mutation', b'', 1),
                    ('printf data | tee >&-', b'', 1),
                ]
                for command, expected, status in cases:
                    result = subprocess.run(command, shell=True, capture_output=True)
                    assert result.returncode == status, (command, result.returncode, result.stderr)
                    if expected is None:
                        assert int(result.stdout) == len(payload), result.stdout
                    else:
                        assert result.stdout == expected, (command, len(result.stdout))
                    if command.startswith('yes |'):
                        assert result.stderr == b'yes: Broken pipe\\n', result.stderr
                    else:
                        assert bool(result.stderr) == bool(status), (command, result.stderr)
                assert open('stream-copy', 'rb').read() == payload
                print('UPSTREAM_STREAMS_OK')
                """)}),
            ("python", {"code": dedent("""
                import compileall, importlib.util, os, py_compile, subprocess, sys
                os.makedirs('cache-check')
                with open('cache-check/cached_module.py', 'w') as source:
                    source.write('value = 731\\n')
                sys.path.insert(0, 'cache-check')
                import cached_module
                cache = importlib.util.cache_from_source(os.path.abspath('cache-check/cached_module.py'))
                assert cached_module.value == 731 and os.path.isfile(cache)
                with open('cache-check/pyc-magic.bin', 'wb') as marker:
                    marker.write(importlib.util.MAGIC_NUMBER)
                with open(cache, 'rb') as bytecode:
                    assert bytecode.read(4) == importlib.util.MAGIC_NUMBER
                py_compile.compile('cache-check/cached_module.py', cfile='cache-check/explicit.pyc', doraise=True)
                assert compileall.compile_dir('cache-check', quiet=2)
                original = os.stat('cache-check/cached_module.py')
                with open('cache-check/cached_module.py', 'w') as source:
                    source.write('value = ???\\n')
                os.utime('cache-check/cached_module.py', ns=(original.st_atime_ns, original.st_mtime_ns))
                reused = subprocess.run(
                    ['python', '-c', "import sys;sys.path.insert(0,'cache-check');import cached_module;assert cached_module.value==731"],
                    capture_output=True)
                assert (reused.returncode, reused.stdout, reused.stderr) == (0, b'', b''), reused
                with open('cache-check/no_cache_module.py', 'w') as source:
                    source.write('value = 193\\n')
                no_cache = subprocess.run(
                    ['python', '-B', '-c', "import sys;sys.path.insert(0,'cache-check');import no_cache_module"],
                    capture_output=True)
                assert (no_cache.returncode, no_cache.stdout, no_cache.stderr) == (0, b'', b''), no_cache
                assert not os.path.exists(importlib.util.cache_from_source('cache-check/no_cache_module.py'))
                print('PYTHON_CACHE_OK')
                """)}),
        ]
        Provider.code_expected = ["File saved", "NESTED_SCRIPT_OK", "FILESYSTEM_WRITES_OK", "HEREDOC_SCRIPT_OK", "FINITE_STDIN_OK", "PERMISSIONS_CWD_OK", "MUTATION_SHARED_OK", "UPSTREAM_STREAMS_OK", "PYTHON_CACHE_OK"]
        Provider.code_steps.append(('python', {'code': TIME_CHECK}))
        Provider.code_expected.append('MONOTONIC_WAIT_OK')
        Provider.code_steps.append(('python', {'code': BASIC_APPLETS_CHECK}))
        Provider.code_expected.append('UPSTREAM_BASIC_APPLETS_OK')
        Provider.code_steps.append(('python', {'code': MUTATION_APPLETS_CHECK + DATE_CHECK}))
        Provider.code_expected.append('UPSTREAM_DATE_OK')
    applet_modes = sum((args.text_applets, args.base64_applet, args.sha256_applet,
                        args.realpath_applet, args.stat_applet, args.ls_applet,
                        args.find_applet, args.program_paths, args.copy_move_applets,
                        args.xargs_applet, args.compression_applets, args.unzip_applet,
                        args.du_applet, args.ln_applet, args.df_applet, args.tar_applet,
                        args.temporary_mount, args.mount_paths, args.character_devices, args.process_files, args.cwd,
                        args.c_port_cwd, args.executable_freshness, args.installation, args.sessions))
    if applet_modes:
        if not args.code_workspace or args.filesystem_tools or args.code_cancel_only:
            parser.error("Applet checks require --code-workspace and exclude other code modes")
        if applet_modes != 1:
            parser.error("Select one applet check mode")
        if args.c_port_cwd:
            if not args.cwd_probe:
                parser.error("--c-port-cwd requires --cwd-probe")
            from test_wolfssh import SshPeer
            ssh_peer = services.enter_context(SshPeer('0.0.0.0'))
        fixture = (INSTALLATION_CHECK if args.installation else
                   EXECUTABLE_FRESHNESS_CHECK if args.executable_freshness else
                   c_port_cwd_check('10.0.2.2', ssh_peer.port, ssh_peer.pin) if args.c_port_cwd else
                   CWD_CHECK if args.cwd else
                   PROCESS_FILES_CHECK if args.process_files else
                   CHARACTER_DEVICES_CHECK if args.character_devices else
                   MOUNT_PATHS_CHECK if args.mount_paths else
                   TEMPORARY_CHECK if args.temporary_mount else
                   TAR_CHECK if args.tar_applet else
                   DF_CHECK if args.df_applet else
                   LN_CHECK if args.ln_applet else
                   DU_CHECK if args.du_applet else
                   UNZIP_CHECK if args.unzip_applet else
                   COMPRESSION_CHECK if args.compression_applets else
                   XARGS_CHECK if args.xargs_applet else
                   COPY_MOVE_CHECK if args.copy_move_applets else
                   PROGRAM_PATHS_CHECK if args.program_paths else
                   None if args.find_applet else
                   LS_CHECK if args.ls_applet else
                   STAT_CHECK if args.stat_applet else
                   REALPATH_CHECK if args.realpath_applet else
                   SHA256_CHECK if args.sha256_applet else
                   BASE64_CHECK if args.base64_applet else TEXT_APPLETS_CHECK)
        marker = ('INSTALLATION_OK' if args.installation else
                  'EXECUTABLE_FRESHNESS_OK' if args.executable_freshness else
                  'C_PORT_CWD_OK' if args.c_port_cwd else
                  'CWD_ANCHORS_OK' if args.cwd else
                  'PROCESS_FILES_OK' if args.process_files else
                  'CHARACTER_DEVICES_OK' if args.character_devices else
                  'MOUNT_PATHS_OK' if args.mount_paths else
                  'TEMPORARY_MOUNT_OK' if args.temporary_mount else
                  'UPSTREAM_TAR_OK' if args.tar_applet else
                  'UPSTREAM_DF_OK' if args.df_applet else
                  'UPSTREAM_LN_OK' if args.ln_applet else
                  'UPSTREAM_DU_OK' if args.du_applet else
                  'UPSTREAM_UNZIP_OK' if args.unzip_applet else
                  'UPSTREAM_COMPRESSION_OK' if args.compression_applets else
                  'UPSTREAM_XARGS_OK' if args.xargs_applet else
                  'UPSTREAM_COPY_MOVE_OK' if args.copy_move_applets else
                  'PROGRAM_PATHS_OK' if args.program_paths else
                  'UPSTREAM_BASIC_APPLETS_OK' if args.find_applet else
                  'UPSTREAM_LS_OK' if args.ls_applet else
                  'UPSTREAM_STATFS_OK' if args.stat_applet else
                  'UPSTREAM_REALPATH_OK' if args.realpath_applet else
                  'UPSTREAM_SHA256_OK' if args.sha256_applet else
                  'UPSTREAM_BASE64_OK' if args.base64_applet else 'UPSTREAM_TEXT_APPLETS_OK')
        Provider.code_steps = [('python', {'code': BASIC_APPLETS_CHECK})]
        Provider.code_expected = ['UPSTREAM_BASIC_APPLETS_OK']
        if args.mount_paths:
            Provider.code_steps.extend([
                ('write', {'path': '/tmp/model-tools/deep/input.py', 'content': "print('SHARED_MOUNT_MODEL_ORIGINAL')\n"}),
                ('edit', {'path': '/tmp/model-tools/deep/input.py', 'old_text': 'ORIGINAL', 'new_text': 'EDITED'}),
                ('read', {'path': '/tmp/model-tools/deep/input.py'}),
                ('shell', {'command': 'python /tmp/model-tools/deep/input.py; cat /tmp/model-tools/deep/input.py > shared-mount-tool-source; cp ash.wasm /tmp/model-tools/ash-copy.wasm'}),
                ('python', {'code': "import os; assert os.path.isfile('/tmp/model-tools/deep/input.py'); print('SHARED_MOUNT_PYTHON_OK')"}),
            ])
            Provider.code_expected.extend([
                'File saved', 'File edited', "print('SHARED_MOUNT_MODEL_EDITED')",
                'SHARED_MOUNT_MODEL_EDITED', 'SHARED_MOUNT_PYTHON_OK',
            ])
        if fixture is not None:
            Provider.code_steps.append(('python', {'code': fixture}))
            Provider.code_expected.append(marker)
        if args.sessions:
            Provider.code_steps = [('python', {'code': BASIC_APPLETS_CHECK})] + [(language, {'code' if language == 'python' else 'command': source})
                                   for language, source, _ in SESSION_STEPS]
            Provider.code_expected = ['UPSTREAM_BASIC_APPLETS_OK'] + [expected for _, _, expected in SESSION_STEPS]
        Provider.filesystem_tools = False
    for index, (name, arguments) in enumerate(Provider.code_steps, 1):
        if name == 'python':
            compile(arguments['code'], f'guest-tool-step-{index}', 'exec')
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
        if args.executable_freshness:
            (tree / 'work/fresh_image.wasm').write_bytes(executable_image(7))
        if args.code_workspace:
            if args.installation:
                subprocess.run([str(args.launcher.resolve()), 'install', str(args.code_workspace.resolve()),
                                str((tree / 'work').resolve()), '--guest'], check=True, timeout=120)
            else:
                for name in ("ash.wasm", "python.wasm"):
                    shutil.copyfile(args.code_workspace / name, tree / "work" / name)
                shutil.copytree(args.code_workspace / "lib", tree / "work/lib")
            if args.c_port_cwd:
                for name in ('curl', 'quickjs', 'wolfssh'):
                    shutil.copyfile(args.code_workspace / (name + '.wasm'), tree / 'work' / (name + '.wasm'))
                shutil.copyfile(args.cwd_probe, tree / 'work/cwd_probe.wasm')
            (tree / "work/background-loop.sh").write_bytes(b"while :; do :; done\n")
        if args.program_paths:
            (tree / "work/bin").mkdir()
            (tree / "work/bin/workspace-entry").write_bytes(b'workspace command data')
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
        qemu = subprocess.Popen([args.qemu, "-machine", f"q35,accel={args.accel}", "-cpu", args.cpu, "-m", str(args.memory), "-display", "none", "-serial", "none", "-monitor", "none", "-no-reboot",
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
            if args.filesystem_tools:
                send("/caps")
                visible = wait("Monotonic clock:")
                assert "PI CPU timer 0" in visible or "UEFI Timestamp" in visible, visible
                source = next(line.strip() for line in visible.splitlines() if "Monotonic clock:" in line)
                print(f"PASS {source}", flush=True)
                send("/clear")
                wait("Conversation cleared")
            if args.driver:
                send("/caps")
                wait("Extra drivers started: 1")
                send("/clear")
                wait("Conversation cleared")
            if applet_modes:
                send("Exercise code tools")
                wait("CODE_TOOLS_VERIFIED", args.code_timeout)
                if args.sessions:
                    send('/clear')
                    wait('Conversation cleared')
                    Provider.code_steps = [(language, {'code' if language == 'python' else 'command': source})
                                           for language, source, _ in CLEAR_STEPS]
                    Provider.code_expected = [expected for _, _, expected in CLEAR_STEPS]
                    send('Exercise code tools')
                    wait('CODE_TOOLS_VERIFIED', args.code_timeout)
                    send('/wasm start ash background-loop.sh')
                    visible = wait('WASM: ash background-loop.sh')
                    background_pid = int(re.search(r'WASM: ash background-loop.sh.*PID (\d+)', visible).group(1))
                    send('Cancel code tool')
                    wait('Running shell')
                    time.sleep(1)
                    connection.sendall(b'\x1b')
                    wait('Request cancelled')
                    Provider.code_steps = [
                        ('shell', {'command': 'test "${CANCEL_SESSION_TOKEN-unset}" = unset || exit 61; test "$(pwd)" = / || exit 62; export SURVIVE_PYTHON_CANCEL=retained; echo SHELL_CANCEL_RESET_OK'}),
                        ('python', {'code': 'assert survivor == 71; print("PYTHON_SURVIVED_SHELL_CANCEL")'}),
                    ]
                    Provider.code_expected = ['SHELL_CANCEL_RESET_OK', 'PYTHON_SURVIVED_SHELL_CANCEL']
                    send('Exercise code tools')
                    wait('CODE_TOOLS_VERIFIED', args.code_timeout)
                    send('Cancel Python code tool')
                    wait('Running python')
                    time.sleep(1)
                    connection.sendall(b'\x1b')
                    wait('Request cancelled')
                    Provider.code_steps = [
                        ('python', {'code': 'assert "survivor" not in globals(); print("PYTHON_CANCEL_RESET_OK")'}),
                        ('shell', {'command': 'test "$SURVIVE_PYTHON_CANCEL" = retained || exit 63; echo SHELL_SURVIVED_PYTHON_CANCEL'}),
                    ]
                    Provider.code_expected = ['PYTHON_CANCEL_RESET_OK', 'SHELL_SURVIVED_PYTHON_CANCEL']
                    send('Exercise code tools')
                    wait('CODE_TOOLS_VERIFIED', args.code_timeout)
                    send('/clear')
                    wait('Conversation cleared')
                    send('/wasm ps')
                    visible = wait(f'{background_pid}: Ready')
                    records = re.findall(r'\b(\d+): (?:Ready|Waiting|Exited|Reading)', visible)
                    assert records == [str(background_pid)], visible
                    send(f'/wasm kill {background_pid}')
                    wait(f'Cancelled {background_pid}')
                    send(f'/wasm reap {background_pid}')
                    wait(f'Reaped {background_pid}')
                if args.mount_paths:
                    send('/wasm load mounted /tmp/model-tools/ash-copy.wasm')
                    wait('Loaded mounted', args.code_timeout)
                send("/exit")
                qemu.wait(timeout=15)
                assert qemu.returncode == 0
                with image.open("rb") as disk:
                    fs = FATFS(RangeStream(disk, 1048576, image.stat().st_size - 1048576))
                    workspace = FatWorkspace(fs)
                    assert workspace.read('basic-applets-result') == b'UPSTREAM_BASIC_APPLETS_OK\n'
                    assert workspace.read('od-octal-result') == b'0000000 101 000 377 102\n0000004\n'
                    assert workspace.read('od-character-result') == b'0000000   A  \\0 377   B\n0000004\n'
                    assert workspace.read('hexdump-result') == (
                        b'00000000  41 00 ff 42                                       |A..B|\n'
                        b'00000004\n'
                    )
                    hexdump_octal = workspace.read('hexdump-octal-result')
                    assert hexdump_octal == b'0000000 101 000 377 102' + b' ' * 48 + b'\n0000004\n'
                    assert 'find-tree/a.txt' not in workspace.entries
                    assert 'find-tree/skip/b.txt' not in workspace.entries
                    assert workspace.read('find-tree/empty') == b''
                    assert workspace.read('find-tree/skip/deep/c.bin') == b'gamma'
                    assert workspace.entries['find-tree/link']['target'] == 'a.txt'
                    if args.program_paths:
                        assert workspace.read('program-paths-result') == b'PROGRAM_PATHS_OK\n'
                        assert workspace.entries['path-one/pick']['mode'] == 0o600
                        assert workspace.read('path-two/pick') == b'#!/bin/ash\nprintf path-two\n'
                    if args.ls_applet:
                        assert workspace.read('ls-result') == (
                            b'alpha\ndangling\ndir-link\nlarge\nlink\nspace name\nsub\n\xe5\x90\x8d\n'
                        )
                        assert workspace.read('ls-tree/.hidden') == b'h'
                        assert workspace.entries['ls-tree/link']['target'] == 'alpha'
                        assert workspace.entries['ls-tree/dir-link']['target'] == 'sub'
                        assert workspace.entries['ls-tree/dangling']['target'] == 'absent'
                    if args.stat_applet:
                        clusters = (fs.total_sectors - fs.first_data_sector) // fs.bpb.BPB_SecPerClus
                        free = sum(fs.fat.get(index) == 0 for index in range(2, clusters + 2)) * fs.cluster_size
                        verify_stat(workspace, (clusters * fs.cluster_size, free, free))
                    elif args.copy_move_applets:
                        verify_copy_move(workspace)
                    elif args.xargs_applet:
                        verify_xargs(workspace)
                    elif args.compression_applets:
                        verify_compression(workspace)
                    elif args.unzip_applet:
                        verify_unzip(workspace)
                    elif args.character_devices:
                        verify_character_devices(workspace)
                    elif args.process_files:
                        verify_process_files(workspace)
                    elif args.cwd:
                        verify_cwd(workspace)
                    elif args.executable_freshness:
                        verify_executable_freshness(workspace)
                    elif args.installation:
                        verify_installation(workspace, args.code_workspace)
                    elif args.sessions:
                        verify_sessions(workspace)
                    elif args.c_port_cwd:
                        verify_c_port_cwd(workspace)
                        assert ssh_peer.errors == [] and ssh_peer.commands == ['nonzero']
                        assert ssh_peer.auth_attempts == [('guest', 'secret')]
                    elif args.mount_paths:
                        verify_mount_paths(workspace)
                        assert workspace.read('shared-mount-tool-source') == b"print('SHARED_MOUNT_MODEL_EDITED')\n"
                    elif args.temporary_mount:
                        verify_temporary(workspace)
                    elif args.tar_applet:
                        verify_tar(workspace)
                    elif args.du_applet:
                        def allocation(name):
                            entry = fs.get('/work/' + workspace.entries[name]['backing'])
                            return sum(length for _, length in entry.dataruns()) * fs.cluster_size
                        verify_du(workspace, allocation)
                    elif args.ln_applet:
                        verify_ln(workspace)
                    elif args.df_applet:
                        clusters = (fs.total_sectors - fs.first_data_sector) // fs.bpb.BPB_SecPerClus
                        free = sum(fs.fat.get(index) == 0 for index in range(2, clusters + 2)) * fs.cluster_size
                        verify_df(workspace, (clusters * fs.cluster_size, free, free))
                    elif args.realpath_applet:
                        verify_realpath(workspace)
                    elif args.sha256_applet:
                        verify_sha256(workspace)
                    elif args.base64_applet:
                        verify_base64(workspace)
                    elif args.text_applets:
                        verify_text_applets(workspace)
                print("PASS WHPX upstream applets and saved FAT bytes", flush=True)
                return
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
                if args.code_workspace:
                    send("/clear")
                    wait("Conversation cleared")
                    if args.code_cancel_only:
                        send("/wasm load ash.wasm")
                        wait("Loaded ash")
                    else:
                        send("Exercise code tools")
                        wait("CODE_TOOLS_VERIFIED", args.code_timeout)
                        send("/wasm ps")
                        wait("Ready  •")
                        print("PASS file tools, 8 MiB writes, finite stdin, mutations, permissions, inherited cwd/umask, upstream BusyBox applets and monotonic waits" if args.filesystem_tools else "PASS real model shell/Python tools, retained interpreters, source preservation, subprocess, failure and capture boundaries", flush=True)
                    send("/clear")
                    wait("Conversation cleared")
                    send("/wasm start ash background-loop.sh")
                    visible = wait("WASM: ash background-loop.sh")
                    background_pid = int(re.search(r"WASM: ash background-loop.sh.*PID (\d+)", visible).group(1))
                    send("Cancel code tool")
                    wait("Running shell")
                    time.sleep(1)
                    connection.sendall(b"\x1b")
                    wait("Request cancelled")
                    send("/clear")
                    wait("Conversation cleared")
                    send("/wasm ps")
                    visible = wait(f"{background_pid}: Ready")
                    records = re.findall(r"\b(\d+): (?:Ready|Waiting|Exited)", visible)
                    assert records == [str(background_pid)], visible
                    send(f"/wasm kill {background_pid}")
                    wait(f"Cancelled {background_pid}")
                    send(f"/wasm reap {background_pid}")
                    wait(f"Reaped {background_pid}")
                    print("PASS Esc cancels and reaps the tool tree while preserving an independent background root", flush=True)
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
                    workspace = FatWorkspace(fs)
                    for name, expected in [("needle.txt", "edited 中 853".encode()), ("created.txt", b"created 419\n"), ("ambiguous.txt", b"aaa")]:
                        assert workspace.read(name) == expected
                    if args.code_workspace:
                        if not args.code_cancel_only:
                            if args.filesystem_tools:
                                assert workspace.read('scripts/deep/input.py') == b"print('NESTED_SCRIPT_OK')\n"
                                for size in (65535, 65536, 65537, 524287, 524288, 524289, 8388608):
                                    actual = workspace.read(f'data/space dir/write-{size}')
                                    expected = (bytes(range(256)) * ((size + 255) // 256))[:size]
                                    assert actual == expected, size
                                assert workspace.read('moved-mutation/result') == b'MUTATION_SHARED_OK\n'
                                assert workspace.read('moved-mutation/result.bak') == b'MUTATION_SHARED_OK\n'
                                for name, mode in [('result', 0o640), ('result.bak', 0o640),
                                                   ('mask-file', 0o600), ('mask-dir', 0o700),
                                                   ('new-mode-target', 0o400)]:
                                    assert workspace.entries['moved-mutation/' + name]['mode'] == mode, name
                                assert workspace.entries['moved-mutation/link']['target'] == 'replaced'
                                stream_payload = (bytes(range(256)) * 2050)[:524289]
                                assert workspace.read('stream-copy') == stream_payload
                                assert workspace.read('stream-binary') == stream_payload
                                assert workspace.read('stream-long') == b'x' * 1100003
                                cached_bytecode = workspace.read('cache-check/__pycache__/cached_module.cpython-314.pyc')
                                pyc_magic = workspace.read('cache-check/pyc-magic.bin')
                                assert cached_bytecode[:4] == pyc_magic
                                assert workspace.read('cache-check/cached_module.py') == b'value = ???\n'
                                assert workspace.read('cache-check/explicit.pyc')[:4] == pyc_magic
                                assert 'cache-check/__pycache__/no_cache_module.cpython-314.pyc' not in workspace.entries
                                assert workspace.read('monotonic-result') == b'MONOTONIC_WAIT_OK\n'
                                assert workspace.read('basic-applets-result') == b'UPSTREAM_BASIC_APPLETS_OK\n'
                                verify_mutation_applets(workspace)
                                verify_date(workspace)
                                for removed in ('mutation', 'moved-mutation/empty', 'moved-mutation/original', 'moved-mutation/replaced'):
                                    assert removed not in workspace.entries, removed
                            else:
                                for name, expected in [("code-shell.txt", "a b\n中\n"), ("code-python.txt", "python child 中")]:
                                    assert workspace.read(name) == expected.encode()
                        assert workspace.read('code-cancel-started.txt') == b"started\n"
                    assert 'must-not-exist.txt' not in workspace.entries, 'Truncated tool call wrote a file'
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


def main():
    with ExitStack() as services:
        run(services)


if __name__ == "__main__":
    main()
