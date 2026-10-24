"""Consecutive real-interpreter session checks with independent saved state."""
SESSION_STEPS = [
    ('shell', '''export SESSION_VALUE=retained
session_fn() { printf 'function-%s\\n' "$1"; }
alias session_alias='echo alias-ok'
mkdir -p /tmp/session-old
cd /tmp/session-old
umask 077
exec 7> /session-fd-output
printf 'first-' >&7
printf '%s' "$$" > /session-shell-pid
false''', 'Exit code: 1'),
    ('python', '''import os, sys, subprocess
counter = 41
assert not os.get_inheritable(5)
for operation in (lambda: os.dup(5), lambda: os.dup2(5, 7),
                  lambda: os.dup2(0, 5), lambda: os.set_inheritable(5, True),
                  lambda: os.open('/proc/self/fd/5', os.O_RDWR)):
    try:
        operation()
    except OSError:
        pass
    else:
        raise AssertionError('Private session descriptor escaped')
os.environ['PY_SESSION_VALUE'] = 'retained'
os.mkdir('/tmp/python-session')
os.chdir('/tmp/python-session')
assert 'SESSION_VALUE' not in os.environ
with open('/session-python-pid', 'w') as out: out.write(str(os.getpid()))
print('PYTHON_SESSION_STARTED')''', 'PYTHON_SESSION_STARTED'),
    ('shell', '''test "$?" -eq 1 || exit 21
test "$SESSION_VALUE" = retained || exit 22
test "$(pwd)" = /tmp/session-old || exit 23
test "$(umask)" = 0077 || exit 24
printf second >&7
session_fn kept
session_alias
if read unexpected; then exit 25; fi
printf 'SHELL_SESSION_RETAINED\\n' ''', 'SHELL_SESSION_RETAINED'),
    ('python', '''counter += 1
assert counter == 42
assert os.getcwd() == '/tmp/python-session'
assert os.environ['PY_SESSION_VALUE'] == 'retained'
os.rename('/tmp/session-old', '/tmp/session-new')
assert sys.stdin.buffer.read() == b''
child = subprocess.run(['python', '-c', "import os; assert not os.path.exists('/proc/self/fd/5'); print('CHILD_SESSION_PRIVATE')"], capture_output=True)
assert (child.returncode, child.stdout, child.stderr) == (0, b'CHILD_SESSION_PRIVATE\\n', b''), child
print('PYTHON_SESSION_RETAINED')''', 'PYTHON_SESSION_RETAINED'),
    ('shell', '''test "$(pwd)" = /tmp/session-new || exit 31
test "$SESSION_VALUE" = retained || exit 32
session_fn moved
printf 'SHELL_ANCHORED_SESSION_OK\\n' ''', 'SHELL_ANCHORED_SESSION_OK'),
    ('python', "counter += 1\nraise ValueError('SESSION_EXPECTED_ERROR')", 'SESSION_EXPECTED_ERROR'),
    ('python', '''assert counter == 43
assert os.getcwd() == '/tmp/python-session'
assert os.environ['PY_SESSION_VALUE'] == 'retained'
print('PYTHON_EXCEPTION_STATE_RETAINED')''', 'PYTHON_EXCEPTION_STATE_RETAINED'),
    ('shell', 'exit 7', 'Session reset.'),
    ('shell', '''test "${SESSION_VALUE-unset}" = unset || exit 41
test "$(pwd)" = / || exit 42
test "$(umask)" = 0022 || exit 43
printf 'SHELL_EXIT_RESET_OK\\n'
export CLEAR_SESSION=pending
cd /tmp/session-new''', 'SHELL_EXIT_RESET_OK'),
    ('python', 'sys.exit(9)', 'Session reset.'),
    ('python', '''import os
assert 'counter' not in globals()
assert 'PY_SESSION_VALUE' not in os.environ
assert os.getcwd() == '/'
assert not {'4', '5'} & set(os.listdir('/proc/self/fd'))
with open('/sessions-result', 'wb') as out: out.write(b'SESSIONS_OK\\n')
print('SESSIONS_OK')
clear_token = 'pending'
os.environ['CLEAR_SESSION'] = 'pending'
os.chdir('/tmp/python-session')''', 'SESSIONS_OK'),
]

CLEAR_STEPS = [
    ('shell', 'test "${CLEAR_SESSION-unset}" = unset || exit 51; test "$(pwd)" = / || exit 52; export CANCEL_SESSION_TOKEN=pending; cd /tmp/session-new; echo SHELL_CLEAR_OK', 'SHELL_CLEAR_OK'),
    ('python', "assert 'clear_token' not in globals(); import os; assert 'CLEAR_SESSION' not in os.environ; assert os.getcwd() == '/'; survivor = 71; print('PYTHON_CLEAR_OK')", 'PYTHON_CLEAR_OK'),
    ('shell', 'return 5; echo RETURN_DID_NOT_STOP', 'Exit code: 5'),
    ('shell', 'test "$?" -eq 5 || exit 71; test "$CANCEL_SESSION_TOKEN" = pending || exit 72; echo SHELL_RETURN_OK', 'SHELL_RETURN_OK'),
    ('shell', 'if then', 'Session reset.'),
    ('shell', 'test "${CANCEL_SESSION_TOKEN-unset}" = unset || exit 73; test "$(pwd)" = / || exit 74; export CANCEL_SESSION_TOKEN=pending; cd /tmp/session-new; echo SHELL_SYNTAX_RESET_OK', 'SHELL_SYNTAX_RESET_OK'),
]


def verify_sessions(workspace):
    assert workspace.read('sessions-result') == b'SESSIONS_OK\n'
    assert workspace.read('session-fd-output') == b'first-second'
    assert workspace.read('session-shell-pid').isdigit()
    assert workspace.read('session-python-pid').isdigit()
    assert not any(path.startswith(('tmp/', 'proc/', 'dev/')) for path in workspace.entries)
