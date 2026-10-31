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
    ('js', '''let counter = 41;
const fixed = 731;
var mutable = 'retained';
function add(value) { return value + fixed; }
let closure = () => counter;
let shellResult = await ash`test -z "$SESSION_VALUE"`;
await ash`printf '%s' js-started > /session-js-started`;
print('JS_SESSION_STARTED');''', 'JS_SESSION_STARTED'),
    ('shell', '''test "$?" -eq 1 || exit 21
test "$SESSION_VALUE" = retained || exit 22
test "$(pwd)" = /tmp/session-old || exit 23
test "$(umask)" = 0077 || exit 24
printf second >&7
session_fn kept
session_alias
if read unexpected; then exit 25; fi
printf 'SHELL_SESSION_RETAINED\\n' ''', 'SHELL_SESSION_RETAINED'),
    ('js', '''counter += 1;
if (counter !== 42 || closure() !== 42 || add(1) !== 732 || mutable !== 'retained') throw Error('Lost globals');
await ash`mv /tmp/session-old /tmp/session-new`;
shellResult = await ash`if read value; then exit 1; fi; echo FINITE_STDIN`;
if (shellResult.stdout.trim() !== 'FINITE_STDIN') throw Error('Unexpected stdin');
print('JS_SESSION_RETAINED');''', 'JS_SESSION_RETAINED'),
    ('shell', '''test "$(pwd)" = /tmp/session-new || exit 31
test "$SESSION_VALUE" = retained || exit 32
session_fn moved
printf 'SHELL_ANCHORED_SESSION_OK\\n' ''', 'SHELL_ANCHORED_SESSION_OK'),
    ('js', "counter += 1; throw Error('SESSION_EXPECTED_ERROR');", 'SESSION_EXPECTED_ERROR'),
    ('js', '''if (counter !== 43 || closure() !== 43) throw Error('Lost exception state');
print('JS_EXCEPTION_STATE_RETAINED');''', 'JS_EXCEPTION_STATE_RETAINED'),
    ('shell', 'exit 7', 'Session reset.'),
    ('shell', '''test "${SESSION_VALUE-unset}" = unset || exit 41
test "$(pwd)" = / || exit 42
test "$(umask)" = 0022 || exit 43
printf 'SHELL_EXIT_RESET_OK\\n'
export CLEAR_SESSION=pending
cd /tmp/session-new''', 'SHELL_EXIT_RESET_OK'),
    ('js', 'quit(9);', 'Session reset.'),
    ('js', '''if (typeof counter !== 'undefined') throw Error('State survived quit');
await ash`printf 'SESSIONS_OK\\n' > /sessions-result`;
print('SESSIONS_OK');
let clear_token = 'pending';''', 'SESSIONS_OK'),
    ('js', "let clear_token = 'again';", 'SyntaxError'),
    ('js', "if (clear_token !== 'pending') throw Error('Declaration changed'); print('JS_DECLARATION_RETAINED');", 'JS_DECLARATION_RETAINED'),
    ('js', "await Promise.reject(Error('JS_EXPECTED_REJECTION'));", 'JS_EXPECTED_REJECTION'),
    ('js', "let handled = Promise.reject(Error('JS_HANDLED')); handled.catch(() => {}); print('JS_HANDLED_REJECTION_OK');", 'JS_HANDLED_REJECTION_OK'),
    ('js', "Promise.reject(Error('JS_UNHANDLED_REJECTION'));", 'JS_UNHANDLED_REJECTION'),
    ('js', "ash`sleep 100`; throw Error('JS_CANCEL_PENDING_ON_ERROR');", 'JS_CANCEL_PENDING_ON_ERROR'),
    ('js', '''shellResult = await ash`printf '%s' ${["a b", "'quoted'", '中', '$HOME']}`;
if (shellResult.stdout !== "a b'quoted'中$HOME") throw Error('Word quoting failed');
try { await ash`echo bad >&2; exit 7`; throw Error('Command succeeded'); }
catch (error) { if (error.exitCode !== 7 || error.stderr !== 'bad\\n') throw error; }
await Promise.all([ash`printf a`, ash`printf b`]);
print('JS_ASYNC_RECOVERED');''', 'JS_ASYNC_RECOVERED'),
    ('js', "if (typeof clear_token !== 'string') throw Error('Lost state'); console.error('JS_STDERR_OK');", 'JS_STDERR_OK'),
    ('js', "let lateResolve; await new Promise(resolve => { lateResolve = resolve; }); print('STALE_REQUEST_RESUMED');", 'Session reset.'),
    ('js', "if (typeof lateResolve !== 'undefined') throw Error('Old continuation survived'); let clear_token = 'pending'; print('JS_PENDING_RESET_OK');", 'JS_PENDING_RESET_OK'),
    ('js', "let rejectedResolve; Promise.reject(Error('JS_UNHANDLED_WHILE_PENDING')); await new Promise(resolve => { rejectedResolve = resolve; }); print('STALE_REQUEST_RESUMED');", 'Session reset.'),
    ('js', "if (typeof rejectedResolve !== 'undefined') throw Error('Rejected continuation survived'); let clear_token = 'pending'; print('JS_REJECTED_PENDING_RESET_OK');", 'JS_REJECTED_PENDING_RESET_OK'),
]

CLEAR_STEPS = [
    ('shell', 'test "${CLEAR_SESSION-unset}" = unset || exit 51; test "$(pwd)" = / || exit 52; export CANCEL_SESSION_TOKEN=pending; cd /tmp/session-new; echo SHELL_CLEAR_OK', 'SHELL_CLEAR_OK'),
    ('js', "if (typeof clear_token !== 'undefined') throw Error('Clear retained state'); let survivor = 71; print('JS_CLEAR_OK');", 'JS_CLEAR_OK'),
    ('shell', 'return 5; echo RETURN_DID_NOT_STOP', 'Exit code: 5'),
    ('shell', 'test "$?" -eq 5 || exit 71; test "$CANCEL_SESSION_TOKEN" = pending || exit 72; echo SHELL_RETURN_OK', 'SHELL_RETURN_OK'),
    ('shell', 'if then', 'Session reset.'),
    ('shell', 'test "${CANCEL_SESSION_TOKEN-unset}" = unset || exit 73; test "$(pwd)" = / || exit 74; export CANCEL_SESSION_TOKEN=pending; cd /tmp/session-new; echo SHELL_SYNTAX_RESET_OK', 'SHELL_SYNTAX_RESET_OK'),
]


def verify_sessions(workspace):
    assert workspace.read('sessions-result') == b'SESSIONS_OK\n'
    assert workspace.read('session-js-started') == b'js-started'
    assert workspace.read('session-fd-output') == b'first-second'
    assert workspace.read('session-shell-pid').isdigit()
    assert not any(path.startswith(('tmp/', 'proc/', 'dev/')) for path in workspace.entries)
