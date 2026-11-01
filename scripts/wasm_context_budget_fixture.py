"""Actual provider/firmware checks for byte budgets and context compaction."""
import hashlib
import json
import threading

LIMIT = 512 * 1024
PAYLOAD = '\x01' * LIMIT
CHECKPOINT = ('Earlier work created budget/deep/payload and retained shell COMPACT_KEEP=731 '
              'in /tmp/context-budget, plus JavaScript compactKeep. Continue the current task. '
              'Completed tool calls must not be repeated.')


class ContextBudgets:
    def __init__(self):
        self.pending = []
        self.last_result_batch = None
        self.sequence = 0
        self.summary_requests = 0
        self.summary_failure = False
        self.summary_wait = False
        self.summary_started = threading.Event()
        self.summary_release = threading.Event()
        self.context_error_after_effect = False
        self.context_errors = 0
        self.task_sizes = []
        self.checkpoints_seen = 0
        self.verified = None

    def respond(self, handler, request, responses):
        messages = request['input' if responses else 'messages']
        if request['tool_choice'] == 'none':
            assert request['tools'] == []
            self.summary_requests += 1
            if self.summary_failure:
                return self.error(handler, 503, 'summary_unavailable', 'Summary rejected for test')
            if self.summary_wait:
                self.summary_started.set()
                assert self.summary_release.wait(30)
            return self.reply(handler, responses, text=CHECKPOINT)
        names = {tool['name'] if responses else tool['function']['name'] for tool in request['tools']}
        assert names == {'read', 'write', 'edit', 'shell', 'js', 'reset_js'}
        self.task_sizes.append(len(json.dumps(request, ensure_ascii=False, separators=(',', ':')).encode()))
        assert self.task_sizes[-1] <= 4 * 1024 * 1024
        if any('[Conversation checkpoint]\n' in (item.get('content') or '') for item in messages):
            self.checkpoints_seen += 1
        last = messages[-1]
        if last.get('type') == 'function_call_output' or last.get('role') == 'tool':
            results = [item for item in messages if item.get('type') == 'function_call_output' or item.get('role') == 'tool']
            batch = tuple((result.get('call_id', result.get('tool_call_id')), result.get('output', result.get('content'))) for result in results[-max(1, len(self.pending)):])
            if self.pending:
                for result, (call_id, expected) in zip(results[-len(self.pending):], self.pending, strict=True):
                    assert result.get('call_id', result.get('tool_call_id')) == call_id
                    value = result.get('output', result.get('content'))
                    assert (value == expected if expected == PAYLOAD else expected in value), (call_id, value[:300])
                self.last_result_batch = batch
            else:
                assert batch == self.last_result_batch, 'Retry changed completed tool results'
            if self.context_error_after_effect:
                self.context_error_after_effect = False
                self.context_errors += 1
                return self.error(handler, 400, 'context_length_exceeded', 'Context token window exceeded')
            self.pending = []
            # The protected latest group must retain its opaque provider fields.
            if responses:
                assert any(item.get('encrypted_content') == 'OPAQUE_CONTEXT_419' for item in messages)
            else:
                assistant = next(item for item in reversed(messages) if item['role'] == 'assistant')
                assert assistant['tool_calls'][0]['extra_content']['signature'] == 'SIGNED_CONTEXT_419'
        else:
            prompt = last.get('content')
            if prompt == 'Context budget exercise':
                self.sequence = 0
            elif prompt == 'Context budget after manual':
                self.sequence = 10
            elif prompt == 'Context budget retry':
                self.sequence = 20
            elif prompt == 'Context budget recovery':
                self.sequence = 30
            elif prompt == 'Context budget admission':
                self.sequence = 40
            else:
                raise AssertionError(('Unexpected context prompt', prompt))
        step = self.sequence
        self.sequence += 1
        shell_check = 'test "$COMPACT_KEEP" = 731 && printf COMPACT_SHELL_STATE_OK'
        js_check = "if (compactKeep.value !== 731) throw Error('State lost'); print('COMPACT_JS_STATE_OK');"
        steps = {
            0: [('shell', {'command': 'mkdir -p /tmp/context-budget; cd /tmp/context-budget; export COMPACT_KEEP=731; printf once > /effect-count; echo CONTEXT_SETUP_OK'}, 'CONTEXT_SETUP_OK')],
            1: [('js', {'code': 'let compactKeep = {value:731}; print("CONTEXT_JS_SETUP_OK");'}, 'CONTEXT_JS_SETUP_OK')],
            2: [('write', {'path': 'budget/deep/payload', 'content': PAYLOAD}, 'File saved')],
            # The 512 KiB read boundary is covered by native core tests. Keep
            # the firmware request small enough to isolate compaction and the
            # model write's 512 KiB argument; verify the large saved bytes at
            # FAT export below.
            3: [('read', {'path': 'effect-count'}, 'once')],
            4: [('shell', {'command': shell_check}, 'COMPACT_SHELL_STATE_OK')],
            5: [('js', {'code': js_check}, 'COMPACT_JS_STATE_OK')],
            10: [('shell', {'command': shell_check}, 'COMPACT_SHELL_STATE_OK')],
            11: [('js', {'code': js_check}, 'COMPACT_JS_STATE_OK')],
            20: [('shell', {'command': 'printf retry-once >> /retry-effect; echo RETRY_EFFECT_DONE'}, 'RETRY_EFFECT_DONE')],
            21: [('js', {'code': js_check}, 'COMPACT_JS_STATE_OK')],
            30: [('shell', {'command': shell_check}, 'COMPACT_SHELL_STATE_OK')],
            31: [('js', {'code': js_check}, 'COMPACT_JS_STATE_OK')],
            40: [('write', {'path': 'must-not-exist', 'content': 'unexecuted'}, 'not executed'),
                 ('read', {'path': 'budget/deep/payload'}, 'not executed'),
                 ('read', {'path': 'budget/deep/payload'}, 'not executed')],
            41: [('write', {'path': 'oversized-must-not-exist', 'content': 'a' * (LIMIT + 1)}, 'File exceeds 512 KiB')],
        }
        if step == 20:
            self.context_error_after_effect = True
        if step in steps:
            calls = []
            for index, (name, arguments, expected) in enumerate(steps[step]):
                call_id = f'context-{step}-{index}'
                calls.append({'id': call_id, 'type': 'function', 'function': {'name': name, 'arguments': json.dumps(arguments, ensure_ascii=False, separators=(',', ':'))}, 'extra_content': {'signature': 'SIGNED_CONTEXT_419'}})
                self.pending.append((call_id, expected))
            return self.reply(handler, responses, calls=calls)
        markers = {6: 'CONTEXT_BUDGET_AUTO_OK', 12: 'CONTEXT_BUDGET_MANUAL_OK', 22: 'CONTEXT_BUDGET_RETRY_OK',
                   32: 'CONTEXT_BUDGET_RECOVERY_OK', 42: 'CONTEXT_BUDGET_ADMISSION_OK'}
        assert step in markers, step
        return self.reply(handler, responses, text='history record ' * 12000 + '\n' + markers[step])

    @staticmethod
    def error(handler, status, code, message):
        body = json.dumps({'error': {'code': code, 'message': message}}).encode()
        handler.send_response(status)
        handler.send_header('Content-Type', 'application/json')
        handler.send_header('Content-Length', str(len(body)))
        handler.end_headers()
        handler.wfile.write(body)

    @staticmethod
    def reply(handler, responses, *, text=None, calls=()):
        handler.send_response(200)
        handler.send_header('Content-Type', 'text/event-stream')
        handler.send_header('Transfer-Encoding', 'chunked')
        handler.end_headers()

        def event(value):
            payload = 'data: ' + (value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, separators=(',', ':'))) + '\n\n'
            payload = payload.encode()
            handler.wfile.write(f'{len(payload):x}\r\n'.encode() + payload + b'\r\n')
            handler.wfile.flush()

        if responses:
            output = [{'type': 'reasoning', 'id': 'r_context', 'summary': [], 'encrypted_content': 'OPAQUE_CONTEXT_419', 'signature': None}]
            output.extend({'type': 'function_call', 'call_id': call['id'], 'signature': 'SIGNED_CONTEXT_419', **call['function']} for call in calls)
            if text is not None:
                output.append({'type': 'message', 'role': 'assistant', 'content': [{'type': 'output_text', 'text': text}]})
            event({'type': 'response.completed', 'response': {'status': 'completed', 'output': output, 'usage': {'input_tokens': 120, 'output_tokens': 20, 'total_tokens': 140}}})
        else:
            delta = {'role': 'assistant'}
            if text is not None:
                delta['content'] = text
            if calls:
                delta['tool_calls'] = [{'index': i, **call} for i, call in enumerate(calls)]
            event({'choices': [{'index': 0, 'delta': delta, 'finish_reason': 'tool_calls' if calls else 'stop'}]})
            event({'choices': [], 'usage': {'prompt_tokens': 120, 'completion_tokens': 20, 'total_tokens': 140}})
            event('[DONE]')
        handler.wfile.write(b'0\r\n\r\n')
        handler.wfile.flush()

    def exercise(self, send, wait, connection, timeout):
        send('Context budget exercise')
        wait('CONTEXT_BUDGET_AUTO_OK', timeout, idle=True)
        assert self.summary_requests > 0 and max(self.task_sizes) > 640 * 1024
        send('/compact')
        wait('Conversation compacted:', timeout, idle=True)
        send('Context budget after manual')
        wait('CONTEXT_BUDGET_MANUAL_OK', timeout, idle=True)
        self.summary_failure = True
        send('/compact')
        wait('Summary rejected for test', timeout, idle=True)
        self.summary_failure = False
        self.summary_wait = True
        send('/compact')
        assert self.summary_started.wait(timeout)
        connection.sendall(b'\x1b')
        wait('cancelled', timeout, idle=True)
        self.summary_release.set()
        self.summary_wait = False
        send('Context budget retry')
        wait('CONTEXT_BUDGET_RETRY_OK', timeout, idle=True)
        assert self.context_errors == 1
        send('Context budget recovery')
        wait('CONTEXT_BUDGET_RECOVERY_OK', timeout, idle=True)
        send('Context budget admission')
        wait('CONTEXT_BUDGET_ADMISSION_OK', timeout, idle=True)
        send('/status')
        wait('Current history tokens: unavailable', timeout, idle=True)
        assert self.checkpoints_seen > 0 and not self.pending

    def verify(self, workspace):
        assert workspace.read('budget/deep/payload') == PAYLOAD.encode()
        assert workspace.read('effect-count') == b'once'
        assert workspace.read('retry-effect') == b'retry-once'
        assert 'must-not-exist' not in workspace.entries
        assert 'oversized-must-not-exist' not in workspace.entries
        self.verified = {'payload_bytes': LIMIT, 'payload_sha256': hashlib.sha256(workspace.read('budget/deep/payload')).hexdigest(),
                         'effect_count': workspace.read('effect-count').decode(), 'retry_effect': workspace.read('retry-effect').decode(),
                         'rejected_paths_absent': True}

    def report(self):
        return {'summary_requests': self.summary_requests, 'context_errors': self.context_errors,
                'task_request_bytes': self.task_sizes, 'checkpoint_requests': self.checkpoints_seen,
                'pending_call_ids': [call_id for call_id, _ in self.pending], 'saved_fat': self.verified,
                'request_body_errors': __import__('smoke_vm').Provider.body_errors}
