# /// script
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Boot a real VM and check serial rendering, guest TCP4, and provider RPC.

Run with uv. The provider is a local deterministic HTTP server; no key is needed.
This exercises guest transport, not the launcher's interactive terminal relay.
"""

import argparse
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import pyte


FILE_MARKER = "HOST_FILE_731_VERIFIED"
MODEL_MARKER = "MODEL_RESPONSE_419_VERIFIED"
AGENT_MARKER = "AGENT_TOOLS_853_VERIFIED"
ANSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")


class Provider(BaseHTTPRequestHandler):
    received = False
    api_format = "chat_completions"
    expected_effort = "medium"
    agent_steps = 0
    waiting = threading.Event()
    release = threading.Event()
    finish_stream = threading.Event()

    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        request = json.loads(body)
        responses = Provider.api_format == "responses"
        format_valid = True
        if responses:
            format_valid = (
                request["reasoning"]["summary"] == "auto"
                and request["store"] is False
                and "reasoning.encrypted_content" in request["include"]
            )
            messages = []
            pending_reasoning = None
            for item in request["input"]:
                if item.get("type") == "reasoning":
                    assert item["encrypted_content"] == "OPAQUE_REASONING_731"
                    assert item["signature"] is None
                    assert item["extensions"] == {"nested": [None, "中", 419]}
                    pending_reasoning = item
                elif item.get("type") == "function_call":
                    messages.append({"role": "assistant", "content": None,
                                     "reasoning_content": "reason 中 retained",
                                     "tool_calls": [{"id": item["call_id"], "type": "function",
                                                     "function": {"name": item["name"], "arguments": item["arguments"]}}]})
                    assert pending_reasoning is not None
                    pending_reasoning = None
                elif item.get("type") == "function_call_output":
                    messages.append({"role": "tool", "content": item["output"], "tool_call_id": item["call_id"]})
                elif item.get("type") == "message":
                    messages.append({"role": item["role"], "content": "".join(part["text"] for part in item["content"])})
                    pending_reasoning = None
                else:
                    messages.append(item)
            request = dict(request, messages=messages, reasoning_effort=request["reasoning"]["effort"],
                           stream_options={"include_usage": True},
                           tools=[{"function": tool} for tool in request["tools"]])
        valid = (
            self.path == ("/v1/responses" if responses else "/v1/chat/completions")
            and format_valid
            and self.headers["Authorization"] == "Bearer smoke-key"
            and request["model"] == "smoke-model"
            and request["stream"] is True
            and request["reasoning_effort"] == Provider.expected_effort
            and request["stream_options"] == {"include_usage": True}
            and {tool["function"]["name"] for tool in request["tools"]} == {"read", "write", "edit"}
        )
        messages = request["messages"]
        if messages[-1]["content"] == "Quota error":
            response = json.dumps({"error": {"message": "Insufficient quota for this request"}}).encode()
            self.send_response(402)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)
            return
        message = {"role": "assistant", "content": MODEL_MARKER}
        if messages[-1] == {"role": "user", "content": "Delay until cancelled"}:
            Provider.waiting.set()
            Provider.release.wait(timeout=10)
            message = {"role": "assistant", "content": "STALE_RESPONSE_MUST_NOT_APPEAR"}
        elif messages[-1]["content"] in ("After idle", "After reconnect"):
            message = {"role": "assistant", "content": "RECONNECTED_VERIFIED" if messages[-1]["content"] == "After reconnect" else "IDLE_CONNECTION_VERIFIED"}
        elif messages[-1] == {"role": "user", "content": "Exercise truncated tools"}:
            message = {"role": "assistant", "content": None, "tool_calls": [{"id": "truncated", "type": "function", "function": {"name": "write", "arguments": json.dumps({"path": "must-not-exist.txt", "content": "BAD"})}}]}
        elif messages[-1] == {"role": "user", "content": "Say the model marker"}:
            # UI slash commands must not pollute model history.
            valid = valid and len(messages) == 3 and messages[0]["role"] == "system"
            valid = valid and messages[1] == {"role": "user", "content": "Delay until cancelled"}
            Provider.received = valid
        else:
            steps = [
                ("read", {"path": "needle.txt"}),
                ("edit", {"path": "needle.txt", "old_text": FILE_MARKER, "new_text": "edited 中 853"}),
                ("write", {"path": "created.txt", "content": "created 419\n"}),
                # Ambiguous overlapping replacement must fail without changing the file.
                ("edit", {"path": "ambiguous.txt", "old_text": "aa", "new_text": "X"}),
            ]
            step = Provider.agent_steps
            if step == 0:
                valid = valid and messages[-1] == {"role": "user", "content": "Exercise the file tools"}
            else:
                expected = [FILE_MARKER, "File edited", "File saved", "Tool error: old_text has multiple matches; file was not changed"][step - 1]
                valid = valid and messages[-1] == {"role": "tool", "content": expected, "tool_call_id": f"call-{step}"}
                valid = valid and messages[-2]["tool_calls"][0]["id"] == f"call-{step}"
                valid = valid and messages[-2]["reasoning_content"] == "reason 中 retained"
                if not responses:
                    previous = messages[-2]
                    valid = valid and previous["tool_calls"][0]["extra_content"] == {"provider": {"thought_signature": "SIGNED_STATE_419"}}
                    valid = valid and previous["tool_calls"][0]["function"]["signature"] is None
                    valid = valid and previous["reasoning_details"] == [{"index": 0, "type": "reasoning.encrypted", "text": None, "summary": None, "data": "OPAQUE_419"}]
            if step < len(steps):
                name, arguments = steps[step]
                message = {"role": "assistant", "content": None, "tool_calls": [{"id": f"call-{step+1}", "type": "function", "function": {"name": name, "arguments": json.dumps(arguments)}}]}
            else:
                message = {"role": "assistant", "content": AGENT_MARKER}
            if valid:
                Provider.agent_steps += 1
        self.send_response(200 if valid else 400)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.end_headers()
        def event(delta, finish=None):
            if responses:
                if "content" in delta:
                    value = {"type": "response.output_text.delta", "delta": delta["content"]}
                elif "reasoning_content" in delta:
                    value = {"type": "response.reasoning_summary_text.delta", "delta": delta["reasoning_content"]}
                else:
                    value = {"type": "response.function_call_arguments.delta", "delta": ""}
            else:
                value = {"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
            data = json.dumps(value, ensure_ascii=False)
            wire = ("data: " + data + "\r\n\r\n").encode()
            # Split UTF-8 and event delimiters across writes.
            for offset in range(0, len(wire), 7):
                self.wfile.write(wire[offset:offset + 7])
            self.wfile.flush()
        try:
            event({"role": "assistant", "reasoning_content": "reason 中 "})
            event({"reasoning_content": "retained"})
            if message.get("tool_calls"):
                call = message["tool_calls"][0]
                event({"reasoning_details": [{"index": 0, "type": "reasoning.encrypted", "text": None, "summary": None, "data": "OPAQUE_419"}],
                       "tool_calls": [{"index": 0, "id": call["id"], "type": "function", "extra_content": {"provider": {"thought_signature": "SIGNED_STATE_419"}},
                                       "function": {"name": call["function"]["name"], "arguments": "", "signature": None}}]})
                arguments = call["function"]["arguments"]
                for offset in range(0, len(arguments), 3):
                    event({"tool_calls": [{"index": 0, "function": {"arguments": arguments[offset:offset + 3]}}]})
                if messages[-1]["content"] == "Exercise truncated tools":
                    return
                event({}, "tool_calls")
            else:
                if message["content"] == MODEL_MARKER:
                    event({"content": "STREAM_PREFIX_中_VISIBLE "})
                    if not Provider.finish_stream.wait(timeout=10):
                        raise AssertionError("Guest did not display text before stream completion")
                content = message["content"]
                for offset in range(0, len(content), 5):
                    event({"content": content[offset:offset + 5]})
                event({}, "stop")
            usage = {"prompt_tokens": 731, "completion_tokens": 419, "total_tokens": 1150,
                     "prompt_tokens_details": {"cached_tokens": 73},
                     "completion_tokens_details": {"reasoning_tokens": 19}}
            if responses:
                output = [{"type": "reasoning", "id": "rs_731", "encrypted_content": "OPAQUE_REASONING_731",
                           "summary": [{"type": "summary_text", "text": "SUMMARY_419_VISIBLE"}],
                           "signature": None, "extensions": {"nested": [None, "中", 419]}}]
                self.wfile.write(("data: " + json.dumps({"type": "response.output_item.done", "output_index": 0, "item": output[0]}) + "\n\n").encode())
                for call in message.get("tool_calls", []):
                    output.append({"type": "function_call", "id": "fc_" + call["id"], "call_id": call["id"],
                                   "name": call["function"]["name"], "arguments": call["function"]["arguments"]})
                if message.get("content"):
                    output.append({"type": "message", "id": "msg_731", "role": "assistant", "status": "completed",
                                   "content": [{"type": "output_text", "text": message["content"], "annotations": []}]})
                stats = {"input_tokens": 731, "output_tokens": 419, "total_tokens": 1150,
                         "input_tokens_details": {"cached_tokens": 73}, "output_tokens_details": {"reasoning_tokens": 19}}
                # Tool rounds omit the reasoning item from final output. The
                # next HTTP request must replay the stream item's exact state.
                final_output = output[1:] if message.get("tool_calls") else output
                value = {"type": "response.completed", "response": {"status": "completed", "output": final_output, "usage": stats}}
                self.wfile.write(("data: " + json.dumps(value) + "\n\n").encode())
            else:
                self.wfile.write(("data: " + json.dumps({"choices": [], "usage": usage}) + "\n\n").encode())
                self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            if messages[-1]["content"] != "Delay until cancelled":
                raise

    def log_message(self, *_args):
        pass


def unused_port():
    # The standalone serve command takes an address. Close this probe before
    # starting it, then verify the child is listening before booting the guest.
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def wait_for_service(process, port):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("HostBridge exited before listening")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError("HostBridge did not listen")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", required=True)
    parser.add_argument("--code", type=Path, required=True)
    parser.add_argument("--vars", type=Path, required=True)
    parser.add_argument("--esp", type=Path, required=True)
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--accel", choices=["kvm", "whpx"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--api-format", choices=["chat_completions", "responses"], default="chat_completions")
    parser.add_argument("--idle-seconds", type=int, default=0)
    args = parser.parse_args()
    Provider.api_format = args.api_format
    args.output.mkdir(parents=True, exist_ok=True)

    provider = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    thread = threading.Thread(target=provider.serve_forever, daemon=True)
    thread.start()
    children = []
    temporary = tempfile.TemporaryDirectory(prefix="efi-agent-smoke-")
    try:
        workspace = Path(temporary.name)
        (workspace / "needle.txt").write_text(FILE_MARKER, encoding="utf-8")
        (workspace / "ambiguous.txt").write_text("aaa", encoding="utf-8")
        env = os.environ.copy()
        env.update({
            "EFI_AGENT_API_BASE": f"http://127.0.0.1:{provider.server_port}/v1",
            "EFI_AGENT_API_KEY": "smoke-key",
            "EFI_AGENT_MODEL": "smoke-model",
            "EFI_AGENT_API_FORMAT": args.api_format,
        })
        rpc_port = unused_port()
        bridge = subprocess.Popen(
            [str(args.launcher.resolve()), "serve", str(workspace), f"127.0.0.1:{rpc_port}"],
            cwd=workspace, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        )
        children.append(("bridge", bridge))
        wait_for_service(bridge, rpc_port)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(15)
            console_port = listener.getsockname()[1]
            command = [
                args.qemu, "-machine", f"q35,accel={args.accel}", "-m", "128",
                "-display", "none", "-serial", "none", "-monitor", "none", "-no-reboot",
                "-drive", f"if=pflash,format=raw,readonly=on,file={args.code.resolve()}",
                "-drive", f"if=pflash,format=raw,snapshot=on,file={args.vars.resolve()}",
                    "-drive", f"if=none,id=esp,format=raw,readonly=on,file={('fat:ro:' if args.esp.is_dir() else '')}{args.esp.resolve()}",
                "-device", "virtio-blk-pci,drive=esp", "-device", "virtio-serial-pci",
                "-chardev", f"socket,id=terminal,host=127.0.0.1,port={console_port}",
                "-device", "virtconsole,chardev=terminal",
                "-chardev", f"socket,id=bridge,host=127.0.0.1,port={rpc_port},reconnect-ms=1000",
                "-netdev", "user,id=network,guestfwd=tcp:10.0.2.100:7420-chardev:bridge",
                "-device", "virtio-net-pci,netdev=network",
            ]
            qemu = subprocess.Popen(command, cwd=workspace, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            children.append(("qemu", qemu))
            connection, _ = listener.accept()
            with connection:
                connection.settimeout(0.5)
                output = bytearray()
                screen = pyte.Screen(100, 30)
                terminal_stream = pyte.ByteStream(screen)
                state = "boot"
                deadline = time.monotonic() + 75
                while time.monotonic() < deadline:
                    try:
                        data = connection.recv(65536)
                        if not data:
                            raise RuntimeError("Guest terminal disconnected")
                        output.extend(data)
                        terminal_stream.feed(data)
                    except TimeoutError:
                        pass
                    try:
                        visible = "\n".join(screen.display)
                    except IndexError:
                        # pyte cannot render an intermediate wide-cell update.
                        # Consume the remaining terminal bytes before assertions.
                        continue
                    if state == "boot" and "What would you like to build?" in visible:
                        print("PASS serial TUI render", flush=True)
                        connection.sendall(b"/caps\r")
                        state = "caps"
                    elif state == "caps" and "Cryptographic RNG:" in visible and "TCP4 interfaces:" in visible:
                        connection.sendall(b"/capabilities\r")
                        state = "removed-caps"
                    elif state == "removed-caps" and "Unknown command. Use /help." in visible:
                        connection.sendall(b"/effort future-budget\r")
                        state = "effort"
                    elif state == "effort" and "Reasoning effort: future-budget" in visible:
                        connection.sendall(b"\x1b[8;50;110t/status\r")
                        screen.resize(lines=50, columns=110)
                        state = "initial-status"
                    elif state == "initial-status" and "Last cached input / input: unavailable" in visible:
                        assert "Reasoning effort: future-budget" in visible
                        assert "API attempts: 0; usage reports: 0" in visible
                        assert "Current history tokens / model context window: unavailable" in visible
                        print("PASS /caps rename, open effort values, and truthful initial /status", flush=True)
                        connection.sendall(b"/effort medium\r")
                        state = "reset-effort"
                    elif state == "reset-effort" and "Reasoning effort: medium." in visible:
                        connection.sendall(b"\x1b[8;29;103t/read needle.txt\r")
                        screen.resize(lines=29, columns=103)
                        state = "commands"
                    elif state == "commands" and "Unknown command. Use /help." in visible:
                        if Provider.agent_steps:
                            raise AssertionError("Removed slash command reached the model")
                        print("PASS file slash command refused", flush=True)
                        connection.sendall(b"/clear\rDelay until cancelled\r")
                        state = "waiting"
                    elif state == "waiting" and Provider.waiting.is_set():
                        connection.sendall(b"\x1b[8;27;97t\x1b")
                        screen.resize(lines=27, columns=97)
                        cancelled_at = time.monotonic()
                        state = "cancelling"
                    elif state == "cancelling":
                        if "Request cancelled" in visible and "Ready" in screen.display[-1]:
                            if time.monotonic() - cancelled_at > 3:
                                raise AssertionError("Guest cancellation was not responsive")
                            print("PASS Esc cancellation and resize during model wait", flush=True)
                            connection.sendall(b"Say the model marker\r")
                            state = "model"
                        elif time.monotonic() - cancelled_at > 3:
                            raise AssertionError("Guest did not cancel the delayed model request")
                    elif state == "model" and "STALE_RESPONSE_MUST_NOT_APPEAR" in visible:
                        raise AssertionError("Cancelled response leaked into the new request")
                    elif state == "model" and "STREAM_PREFIX_中_VISIBLE" in visible and not Provider.finish_stream.is_set():
                        print("PASS Unicode stream text displayed before completion", flush=True)
                        connection.sendall("\x1b[200~DRAFT 中\nsecondX\x1b[201~\x7f".encode())
                        state = "editing_stream"
                    elif state == "editing_stream" and "DRAFT 中" in visible and "second▏" in visible:
                        if Provider.finish_stream.is_set() or MODEL_MARKER in visible:
                            raise AssertionError("Draft editing only appeared after model completion")
                        print("PASS multiline draft paste and Backspace render before model completion", flush=True)
                        connection.sendall(b"\x1b[A\x1b[HZ\x1b[B\x1b[FY")
                        state = "moving_draft"
                    elif state == "moving_draft" and "ZDRAFT 中" in visible and "secondY▏" in visible:
                        if Provider.finish_stream.is_set() or MODEL_MARKER in visible:
                            raise AssertionError("Arrow editing only appeared after model completion")
                        print("PASS Up/Down move the draft cursor during model output", flush=True)
                        # Remove the draft without submitting or cancelling.
                        connection.sendall(b"\x7f" * len("ZDRAFT 中\nsecondY"))
                        state = "clearing_draft"
                    elif state == "clearing_draft" and "DRAFT 中" not in visible and "second▏" not in visible:
                        Provider.finish_stream.set()
                        state = "model"
                    elif state == "model" and MODEL_MARKER in visible:
                        if not Provider.received:
                            raise AssertionError("Provider request format was incorrect")
                        if time.monotonic() - cancelled_at > 3:
                            raise AssertionError("New request waited for the cancelled provider response")
                        Provider.release.set()
                        print(f"PASS {args.api_format} request and guest response", flush=True)
                        connection.sendall(b"Quota error\r")
                        state = "quota"
                    elif state == "quota" and "Provider HTTP 402" in visible and "Insufficient quota" in visible:
                        print("PASS actual provider quota error reported", flush=True)
                        connection.sendall(b"/clear\rExercise truncated tools\r")
                        state = "truncated"
                    elif state == "truncated" and "Provider stream ended before [DONE]" in visible:
                        if (workspace / "must-not-exist.txt").exists():
                            raise AssertionError("Truncated tool call was executed")
                        print("PASS truncated streamed tool call did not write a file", flush=True)
                        Provider.expected_effort = "future-budget"
                        connection.sendall(b"/effort future-budget\r/clear\rExercise the file tools\r")
                        state = "agent"
                    elif state == "agent" and "STALE_RESPONSE_MUST_NOT_APPEAR" in visible:
                        raise AssertionError("Cancelled stream leaked into the tool turn")
                    elif state == "agent" and AGENT_MARKER in visible:
                        if Provider.agent_steps != 5:
                            raise AssertionError("Agent did not complete all correlated tool rounds")
                        if (workspace / "needle.txt").read_text(encoding="utf-8") != "edited 中 853":
                            raise AssertionError("Agent edit did not change the actual file")
                        if (workspace / "created.txt").read_text(encoding="utf-8") != "created 419\n":
                            raise AssertionError("Agent write did not create the actual file")
                        if (workspace / "ambiguous.txt").read_text(encoding="utf-8") != "aaa":
                            raise AssertionError("Ambiguous edit changed the file")
                        print("PASS guest-driven read/edit/write loop and failed edit recovery", flush=True)
                        connection.sendall(b"\x1b[8;50;110t")
                        screen.resize(lines=50, columns=110)
                        state = "details"
                    elif (state == "details" and "Tool result: edit (failed)" in visible
                          and "Ready" in screen.display[-1]):
                        label = "Reasoning summary" if args.api_format == "responses" else "Reasoning (provider text)"
                        hidden = "SUMMARY_419_VISIBLE" if args.api_format == "responses" else "reason 中 retained"
                        assert "[+] " + label in visible
                        assert hidden not in visible
                        assert "multiple matches" not in visible
                        rows = [i for i, line in enumerate(screen.display) if "[+] " + label in line]
                        connection.sendall(f"\x1b[<0;5;{rows[-1]+1}M\x1b[<0;5;{rows[-1]+1}m".encode())
                        state = "reason-open"
                    elif state == "reason-open" and hidden in visible and "[-] " + label in visible:
                        row = next(i for i, line in enumerate(screen.display) if "[-] " + label in line)
                        connection.sendall(f"\x1b[<0;5;{row+1}M".encode())
                        state = "reason-closed"
                    elif state == "reason-closed" and hidden not in visible and "[-] " + label not in visible:
                        row = next(i for i, line in enumerate(screen.display) if "[+] Tool result: edit (failed)" in line)
                        connection.sendall(f"\x1b[<0;5;{row+1}M".encode())
                        state = "tool-open"
                    elif state == "tool-open" and "multiple matches" in visible and "[-] Tool result: edit (failed)" in visible:
                        row = next(i for i, line in enumerate(screen.display) if "[-] Tool result: edit (failed)" in line)
                        connection.sendall(f"\x1b[<0;5;{row+1}M".encode())
                        state = "tool-closed"
                    elif state == "tool-closed" and "multiple matches" not in visible and "[-] Tool result: edit (failed)" not in visible:
                        print("PASS reasoning and tool results collapsed by default, click expand/collapse", flush=True)
                        connection.sendall(b"\x1b[8;50;110t/status\r")
                        screen.resize(lines=50, columns=110)
                        state = "usage-status"
                    elif state == "usage-status" and "Last cached input / input: 9.99%" in visible:
                        assert "Input tokens: 731;" in visible
                        assert "Output (includes reasoning) tokens: 419;" in visible
                        assert "Total tokens: 1150;" in visible
                        assert "Cached input tokens: 73;" in visible
                        assert "Reasoning tokens: 19;" in visible
                        assert "Reasoning effort: future-budget" in visible
                        assert "API attempts: 9; usage reports: 7" in visible
                        assert "reported subtotal: 5117 (7 requests)" in visible
                        assert "reported subtotal: 2933 (7 requests)" in visible
                        assert "reported subtotal: 8050 (7 requests)" in visible
                        print("PASS real SSE usage, reasoning tokens, and cache ratio in /status", flush=True)
                        connection.sendall(b"/clear\r/status\r")
                        state = "cleared-status"
                    elif (state == "cleared-status" and "History: 1 messages," in visible
                          and "Last cached input / input: 9.99%" in visible):
                        assert "API attempts: 9; usage reports: 7" in visible
                        assert "Reasoning effort: future-budget" in visible
                        print("PASS effort forwarded to API and /clear preserves usage totals", flush=True)
                        if args.idle_seconds:
                            idle_until = time.monotonic() + args.idle_seconds
                            deadline = idle_until + 30
                            state = "idle"
                        else:
                            state = "done"
                            break
                    elif state == "idle" and time.monotonic() >= idle_until:
                        connection.sendall(b"After idle\r")
                        state = "after-idle"
                    elif state == "after-idle" and "IDLE_CONNECTION_VERIFIED" in visible:
                        print(f"PASS model response after {args.idle_seconds}s idle", flush=True)
                        bridge.terminate()
                        bridge.communicate(timeout=10)
                        bridge = subprocess.Popen(
                            [str(args.launcher.resolve()), "serve", str(workspace), f"127.0.0.1:{rpc_port}"],
                            cwd=workspace, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                        )
                        children.append(("bridge-restarted", bridge))
                        wait_for_service(bridge, rpc_port)
                        idle_until = time.monotonic() + 40
                        deadline = idle_until + 45
                        state = "reconnecting"
                    elif state == "reconnecting" and time.monotonic() >= idle_until:
                        connection.sendall(b"/clear\rAfter reconnect\r")
                        state = "after-reconnect"
                    elif state == "after-reconnect" and "RECONNECTED_VERIFIED" in visible:
                        print("PASS heartbeat recovery after HostBridge restart", flush=True)
                        state = "done"
                        break
                    if qemu.poll() is not None:
                        raise RuntimeError("QEMU exited during the test")
                (args.output / "serial.bin").write_bytes(output)
                (args.output / "transcript.txt").write_bytes(ANSI.sub(b"", output))
                (args.output / "screen.txt").write_text("\n".join(screen.display), encoding="utf-8")
                if state != "done":
                    raise TimeoutError(f"VM smoke test stopped in state {state}")
    finally:
        Provider.release.set()
        Provider.finish_stream.set()
        for name, child in reversed(children):
            if child.poll() is None:
                child.terminate()
            try:
                _, stderr = child.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                _, stderr = child.communicate()
            (args.output / f"{name}.log").write_bytes(stderr)
        provider.shutdown()
        provider.server_close()
        thread.join(timeout=2)
        # Windows holds child working directories open until the children exit.
        temporary.cleanup()


if __name__ == "__main__":
    main()
