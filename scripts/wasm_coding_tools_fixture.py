"""Firmware acceptance for persistent tasks and recoverable coding tools."""
import json
import re
import time

from wasm_context_budget_fixture import ContextBudgets


class CodingTools:
    def __init__(self, timeout_only=False):
        self.timeout_only = timeout_only
        self.index = 0
        self.task_id = None
        self.expected = None
        self.results = []
        self.tool_rounds = 0
        self.pending_call_id = None
        self.pending_name = None
        self.background_id = None
        self.recovering = False
        self.recovery_index = 0
        self.recovery_expected = None
        self.checking_clear = False
        self.wait_cancelled = False
        self.transient_failures = 0
        self.silent_requests = 0
        self.timeout_elapsed = None
        self.saved_logs = []
        self.payload = 'x' * 524_289 + 'UNIQUE'

    def respond(self, handler, request, responses):
        messages = request['input' if responses else 'messages']
        last = messages[-1]
        prompt = last.get('content')
        if prompt == 'Coding warmup':
            return ContextBudgets.reply(handler, responses, text='CODING_WARMUP_OK')
        if prompt == 'Coding timeout':
            self.silent_requests += 1
            time.sleep(30)
            return
        if prompt == 'Coding recovery':
            if self.timeout_only:
                return ContextBudgets.reply(handler, responses, text='CODING_RECOVERY_OK')
            assert self.wait_cancelled
            assert any('Wait cancelled. Task ' in (item.get('output', item.get('content')) or '')
                       for item in messages if item.get('type') == 'function_call_output' or item.get('role') == 'tool')
            self.recovering = True
            return self.recover(handler, last, responses)
        if prompt == 'Coding cancel wait':
            return self.call(handler, responses, 'task', {'action': 'poll', 'id': self.background_id, 'wait_ms': 10000})
        if prompt == 'Coding cleared':
            self.checking_clear = True
            return self.call(handler, responses, 'task', {'action': 'list'})
        if self.checking_clear:
            result = last.get('output', last.get('content'))
            assert result == '', result
            return ContextBudgets.reply(handler, responses, text='CODING_CLEAR_OK')
        if self.recovering:
            return self.recover(handler, last, responses)
        assert request['tool_choice'] != 'none', 'Unexpected compaction in bounded coding exercise'
        if last.get('type') == 'function_call_output' or last.get('role') == 'tool':
            result = last.get('output', last.get('content'))
            call_id = last.get('call_id', last.get('tool_call_id'))
            if self.index == 5 and self.transient_failures == 0:
                self.transient_failures += 1
                return ContextBudgets.error(handler, 503, 'temporary_unavailable', 'Retry this model request')
            assert call_id == self.pending_call_id, (call_id, self.index)
            if self.expected not in result and self.pending_name == 'task' and 'Status: Running' in result:
                task_id = int(re.search(r'Task id: (\d+)', result)[1])
                return self.call(handler, responses, 'task', {'action': 'poll', 'id': task_id, 'wait_ms': 1000})
            assert self.expected in result, (self.index, self.expected, result[:1000])
            self.results.append(result)
            print(f'PASS coding step {self.index}: {self.expected}', flush=True)
            if self.index in (1, 7):
                self.task_id = int(re.search(r'Task id: (\d+)', result)[1])
            if self.index == 15:
                self.background_id = int(re.search(r'Task id: (\d+)', result)[1])
            if self.index == 3:
                for stream in ('stdout', 'stderr'):
                    path = re.search(stream + r'_path: (\S+)', result)[1]
                    self.saved_logs.append((stream, path))
            if self.index == 10:
                page = json.loads(result)
                assert bytes.fromhex(page['content']) == self.payload.encode()[524_285:524_295]
                assert page['next_offset'] == 524_295
        steps = [
            ('task', {'action': 'start', 'command': 'printf TASK_READY; sleep 1000', 'wait_ms': 1000}, 'TASK_READY'),
            ('shell', {'command': 'export CODING_KEEP=731; printf once >> /coding-effect; echo SHELL_STATE_SET'}, 'SHELL_STATE_SET'),
            ('shell', {'command': 'printf HEAD; awk \'BEGIN{for(i=0;i<50000;i++)print "x"}\'; printf FINAL_BUILD_FAILURE >&2; printf TAIL'}, 'FINAL_BUILD_FAILURE'),
            ('js', {'code': 'let codingKeep = 853; print("JS_STATE_SET");'}, 'JS_STATE_SET'),
            ('task', {'action': 'poll', 'id': self.task_id}, 'Running'),
            ('task', {'action': 'cancel', 'id': self.task_id, 'wait_ms': 100}, 'Exited(130)'),
            ('task', {'action': 'start', 'terminal': True, 'command': 'test -t 0 && test -t 1 || exit 9; printf INPUT_READY; read answer; printf "%s" "$answer" > /coding-input; echo INPUT_SAVED', 'wait_ms': 1000}, 'INPUT_READY'),
            ('task', {'action': 'input', 'id': self.task_id, 'input': 'typed 中\n', 'eof': True, 'wait_ms': 1000}, 'INPUT_SAVED'),
            ('write', {'path': 'coding/deep/large', 'content': self.payload}, 'File saved'),
            ('read', {'path': 'coding/deep/large', 'offset': 524_285, 'limit': 10, 'hex': True}, 'hex'),
            ('write', {'path': 'coding/deep/large', 'content': 'TAIL', 'append': True}, 'Content appended'),
            ('edit', {'path': 'coding/deep/large', 'old_text': 'UNIQUE', 'new_text': 'PATCHED'}, 'File edited'),
            ('shell', {'command': 'test "$CODING_KEEP" = 731 && test "$(cat /coding-effect)" = once && echo SHELL_STATE_PRESERVED'}, 'SHELL_STATE_PRESERVED'),
            ('js', {'code': 'if (codingKeep !== 853) throw Error("Lost JS state"); print("JS_STATE_PRESERVED");'}, 'JS_STATE_PRESERVED'),
        ]
        steps += [('task', {'action': 'start', 'command': 'sleep 1000 & printf BACKGROUND_READY', 'wait_ms': 1000}, 'BACKGROUND_READY')]
        steps += [('shell', {'command': f'echo ROUND_{i}'}, f'ROUND_{i}') for i in range(15, 21)]
        if self.index == len(steps):
            return ContextBudgets.reply(handler, responses, text='CODING_TOOLS_OK')
        name, arguments, self.expected = steps[self.index]
        self.index += 1
        return self.call(handler, responses, name, arguments)

    def call(self, handler, responses, name, arguments):
        self.pending_call_id = f'coding-{self.tool_rounds}'
        self.pending_name = name
        self.tool_rounds += 1
        call = {'id': self.pending_call_id, 'type': 'function', 'function': {
            'name': name, 'arguments': json.dumps(arguments, ensure_ascii=False)}}
        return ContextBudgets.reply(handler, responses, calls=[call])

    def recover(self, handler, last, responses):
        if last.get('type') == 'function_call_output' or last.get('role') == 'tool':
            result = last.get('output', last.get('content'))
            assert last.get('call_id', last.get('tool_call_id')) == self.pending_call_id
            if self.recovery_expected not in result and 'Status: Running' in result:
                task_id = int(re.search(r'Task id: (\d+)', result)[1])
                return self.call(handler, responses, 'task', {'action': 'poll', 'id': task_id, 'wait_ms': 1000})
            assert self.recovery_expected in result, (self.recovery_index, result)
            print(f'PASS task lifecycle step {self.recovery_index}: {self.recovery_expected}', flush=True)
        steps = [
            ('task', {'action': 'poll', 'id': self.background_id}, 'Status: Running'),
            ('task', {'action': 'cancel', 'id': self.background_id, 'wait_ms': 1000}, 'Status: Exited(0)'),
            ('task', {'action': 'forget', 'id': self.background_id}, 'Task handle released'),
            ('shell', {'command': 'test "$CODING_KEEP" = 731 && echo TASK_CANCEL_PRESERVES_SHELL'}, 'TASK_CANCEL_PRESERVES_SHELL'),
            ('js', {'code': 'if (codingKeep !== 853) throw Error("Lost JS state"); print("TASK_CANCEL_PRESERVES_JS");'}, 'TASK_CANCEL_PRESERVES_JS'),
            ('task', {'action': 'start', 'command': 'sleep 1000'}, 'Status: Running'),
        ]
        if self.recovery_index == len(steps):
            return ContextBudgets.reply(handler, responses, text='CODING_RECOVERY_OK')
        name, arguments, self.recovery_expected = steps[self.recovery_index]
        self.recovery_index += 1
        return self.call(handler, responses, name, arguments)

    def exercise(self, send, wait, connection, timeout):
        if self.timeout_only:
            send('Coding warmup')
            wait('CODING_WARMUP_OK', 40, idle=True)
            started = time.monotonic()
            send('Coding timeout')
            wait('timed out', 25, idle=True)
            self.timeout_elapsed = time.monotonic() - started
            assert self.timeout_elapsed < 25
            assert self.silent_requests == 3, self.silent_requests
        else:
            send('Coding exercise')
            wait('CODING_TOOLS_OK', timeout, idle=True)
            assert self.index == 21 and len(self.results) == 21
            assert self.transient_failures == 1
            send('Coding cancel wait')
            wait('Running task', 40)
            connection.sendall(b'\x1b')
            wait('Request cancelled', 40, idle=True)
            self.wait_cancelled = True
        send('Coding recovery')
        wait('CODING_RECOVERY_OK', 40 if self.timeout_only else timeout, idle=True)
        if not self.timeout_only:
            send('/clear')
            wait('Conversation cleared', 40)
            send('Coding cleared')
            wait('CODING_CLEAR_OK', 40, idle=True)

    def verify(self, workspace):
        if self.timeout_only:
            return
        assert workspace.read('coding-effect') == b'once'
        assert workspace.read('coding-input') == 'typed 中'.encode()
        assert workspace.read('coding/deep/large') == b'x' * 524_289 + b'PATCHEDTAIL'
        for stream, path in self.saved_logs:
            expected = b'HEAD' + b'x\n' * 50_000 + b'TAIL' if stream == 'stdout' else b'FINAL_BUILD_FAILURE'
            assert workspace.read(path.lstrip('/')) == expected

    def report(self):
        return {'completed_steps': self.index, 'tool_rounds': self.tool_rounds, 'lifecycle_steps': self.recovery_index,
                'cleared_task_list': self.checking_clear, 'wait_cancelled': self.wait_cancelled,
                'transient_failures': self.transient_failures,
                'silent_requests': self.silent_requests, 'timeout_elapsed_seconds': self.timeout_elapsed,
                'full_logs': self.saved_logs}
