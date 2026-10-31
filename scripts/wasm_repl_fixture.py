"""Exercise local terminal I/O, history isolation and shared interpreter state."""
import json
import re


def exercise_repl(send, wait, connection, provider, timeout):
    def command(source, expected, *, exact=False, idle=True):
        send(source)
        def complete(visible):
            footer = visible.splitlines()[-1].strip()
            ready = not idle or footer.startswith(('Ready', 'Returned to conversation'))
            return ready and (not exact or expected in [line.strip() for line in visible.splitlines()])
        return wait(expected, timeout, predicate=complete)

    def enter(language):
        command('/enter' if language == 'shell' else '/enter js', '$ ▏' if language == 'shell' else 'js> ▏')

    provider.code_steps = [
        ('shell', {'command': 'export REPL_KEEP=model; echo REPL_MODEL_SHELL_SEEDED'}),
        ('js', {'code': "let directValue = 41; print('REPL_MODEL_JS_SEEDED');"}),
    ]
    provider.code_expected = ['REPL_MODEL_SHELL_SEEDED', 'REPL_MODEL_JS_SEEDED']
    provider.code_verified = 'REPL_MODEL_SEEDED'
    command('Exercise code tools', 'REPL_MODEL_SEEDED')
    request_count = len(provider.requests)

    enter('shell')
    command("test \"$REPL_KEEP\" = model || exit 80; mkdir -p /tmp/repl; cd /tmp/repl; printf '%s' \"$$\" > /repl-shell-pid; printf '%s%s' REPL_ SHELL_READY", 'REPL_SHELL_READY')
    command('/bin/pwd', '/tmp/repl', exact=True)
    command('if then', 'syntax error:')
    command('test "$REPL_KEEP" = model || exit 86; printf "%s%s\\n" REPL_ SYNTAX_RECOVERED', 'REPL_SYNTAX_RECOVERED')
    visible = command("printf A; printf B >&2; printf C", 'ABC$ ▏')
    assert not any(label in visible for label in ('Standard output:', 'Standard error:', 'Exit code:', '  execution'))
    command("printf '%s%s\\n' TERMINAL_PRIVATE_ SOURCE_OUTPUT", 'TERMINAL_PRIVATE_SOURCE_OUTPUT')
    send("read terminalLine; printf '%s' \"$terminalLine\" > /repl-stdin; printf '%s%s\\n' REPL_ STDIN_DONE")
    wait('REPL_ STDIN_DONE', timeout)
    command('typed input 中', 'REPL_STDIN_DONE')
    send('cat > /repl-cat-input')
    wait('cat > /repl-cat-input', timeout)
    command('first input', 'first input', idle=False)
    connection.sendall(b'\x04')
    wait('$ ▏', timeout)
    command("printf '%s%s' REPL_ HISTORY", 'REPL_HISTORY')
    connection.sendall(b'\x1b[A')
    wait("$ printf '%s%s' REPL_ HISTORY", timeout)
    connection.sendall(b'\x03')
    wait('^C', timeout)
    command("cat <<'END' > /repl-multiline\nline one\n中\nEND\nprintf '%s%s' REPL_ MULTILINE", 'REPL_MULTILINE')
    for source in ['/leave', '/enter js', '/help', '/clear', '/quit', '/exit']:
        command(source, source.split()[0] + ': not found')
    command('printf "%s%s" REPL_ STILL_SHELL', 'REPL_STILL_SHELL')
    command('exit 7', 'Interpreter ended. Returned to conversation.')
    enter('js')
    command("/hello/.test('hello')", 'true', exact=True)
    command('directValue + 689', '730', exact=True)
    command('/clear', 'SyntaxError')
    command('directValue + 690', '731', exact=True)
    command('quit(9)', 'Interpreter ended. Returned to conversation.')
    assert len(provider.requests) == request_count, provider.requests
    print('PASS firmware direct terminal I/O, stdin/EOF, history and syntax recovery without API requests', flush=True)

    provider.code_steps = [
        ('shell', {'command': 'test "${REPL_KEEP-unset}" = unset && test "$(pwd)" = / || exit 81; if read unexpected; then exit 85; fi; export HANDOFF_FROM_MODEL=kept; printf manual-model > /repl-result; echo REPL_MODEL_SHELL'}),
        ('js', {'code': "if (typeof directValue !== 'undefined') throw Error('quit retained globals'); let modelValue = 731; print('REPL_MODEL_JS');"}),
    ]
    provider.code_expected = ['REPL_MODEL_SHELL', 'REPL_MODEL_JS']
    provider.code_verified = 'REPL_MODEL_SHARED'
    command('Exercise code tools', 'REPL_MODEL_SHARED')
    first_request = json.dumps(provider.requests[request_count])
    assert 'Manual shell execution by the user' not in first_request
    assert 'Manual js execution by the user' not in first_request
    for private in ('TERMINAL_PRIVATE_', 'TERMINAL_PRIVATE_SOURCE_OUTPUT', 'typed input 中', 'directValue + 689', '730', 'REPL_SHELL_READY'):
        assert private not in first_request, (private, first_request)
    request_count = len(provider.requests)
    print('PASS firmware model context excludes manual source/stdin/output; native exit releases interpreter state', flush=True)

    enter('js')
    command('modelValue + 2', '733', exact=True)
    command("(await $`printf '%s%s' REPL_ AWAITED`).stdout", 'REPL_AWAITED')
    command("throw Error('REPL_' + 'EXPECTED_ERROR')", 'REPL_EXPECTED_ERROR')
    command('modelValue + 4', '735', exact=True)
    connection.sendall(b'unsubmitted draft\x03')
    wait('^C', timeout)
    command('modelValue + 5', '736', exact=True)
    command('quit()', 'Interpreter ended. Returned to conversation.')
    enter('shell')
    command('test "$HANDOFF_FROM_MODEL" = kept || exit 82; printf "%s%s" -direct -again >> /repl-result; exit 7', 'Interpreter ended. Returned to conversation.')

    visible = command('/wasm start ash background-loop.sh', 'WASM: ash background-loop.sh')
    background = int(re.search(r'WASM: ash background-loop.sh.*PID (\d+)', visible)[1])
    enter('js')
    send("print('REPL_' + 'JS_CANCEL_READY'); await $`sleep 100`;")
    wait('REPL_JS_CANCEL_READY', timeout)
    connection.sendall(b'\x03')
    wait('Interpreter reset after cancellation', timeout, idle=True)
    command('quit()', 'Interpreter ended. Returned to conversation.')
    command('/clear', 'Conversation cleared', idle=False)
    command('/wasm ps', f'{background}: Ready')
    enter('js')
    connection.sendall(b'\x04')
    wait('Interpreter ended. Returned to conversation.', timeout, idle=True)
    enter('shell')
    command("trap 'printf EOF_TRAP > /repl-eof-trap' EXIT", '$ ▏')
    connection.sendall(b'\x04')
    wait('Interpreter ended. Returned to conversation.', timeout, idle=True)
    command(f'/wasm kill {background}', f'Cancelled {background}')
    command(f'/wasm reap {background}', f'Reaped {background}')
    assert len(provider.requests) == request_count, provider.requests
    assert not provider.errors, provider.errors


def verify_repl(workspace):
    assert workspace.read('repl-result') == b'manual-model-direct-again'
    assert workspace.read('repl-shell-pid').isdigit()
    assert workspace.read('repl-multiline') == 'line one\n中\n'.encode()
    assert workspace.read('repl-stdin') == 'typed input 中'.encode()
    assert workspace.read('repl-cat-input') == b'first input\n'
    assert workspace.read('repl-eof-trap') == b'EOF_TRAP'
