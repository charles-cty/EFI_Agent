"""Adapt standard Python process interfaces through WASI descriptor operations."""
import errno
import os
import struct

_channel = os.open('.efi-process', os.O_RDWR)
_read = os.read
_write = os.write


def _request(operation, *words, payload=b''):
    packet = struct.pack('<' + 'I' * (1 + len(words)), operation, *words) + payload
    if _write(_channel, packet) != len(packet):
        raise RuntimeError('Incomplete process control request')
    response = _read(_channel, 8)
    if len(response) != 8:
        raise RuntimeError('Incomplete process control response')
    result, value = struct.unpack('<ii', response)
    if result < 0:
        errors = {2: errno.ENOENT, 7: errno.E2BIG, 8: errno.ENOEXEC,
                  9: errno.EBADF, 10: errno.ECHILD, 11: errno.EAGAIN,
                  12: errno.ENOMEM, 14: errno.EFAULT, 22: errno.EINVAL, 24: errno.EMFILE}
        error = errors[-result]
        raise OSError(error, os.strerror(error))
    return result, value


def _unsupported(*args, **kwargs):
    raise OSError(errno.ENOTSUP, 'Process operation is unavailable')


def _pipe():
    return _request(1)


def _dup(fd):
    result = _request(2, fd, 0)[0]
    _set_inheritable(result, False)
    return result


def _dup2(fd, target, inheritable=True):
    result = _request(3, fd, target)[0]
    if fd != target or not inheritable:
        _set_inheritable(result, inheritable)
    return result


def _get_inheritable(fd):
    return not _request(4, fd)[0]


def _set_inheritable(fd, inheritable):
    _request(5, fd, int(not inheritable))


def _waitpid(pid, options):
    if pid <= 0 or options not in (0, 1):
        _unsupported()
    child, code = _request(10, pid, options)
    return child, (code & 255) << 8 if child else 0


def _waitstatus_to_exitcode(status):
    if status & 255 == 127:
        raise ValueError('Stopped process has no exit code')
    return -(status & 127) if status & 127 else (status >> 8) & 255


def _registry_name(name):
    name = os.fsdecode(name)
    for prefix in ('/usr/bin/', '/bin/', './'):
        if name.startswith(prefix):
            name = name[len(prefix):]
            break
    return 'ash' if name == 'sh' else name


def _pack(values):
    result = bytearray()
    for value in values:
        encoded = os.fsencode(value)
        if b'\0' in encoded:
            raise ValueError('embedded null byte')
        result += encoded + b'\0'
        if len(result) > 65536:
            raise OSError(errno.E2BIG, 'Process arguments exceed 64 KiB')
    return result


def _fork_exec(args, executables, close_fds, keep, cwd, env,
               p2cread, p2cwrite, c2pread, c2pwrite, errread, errwrite,
               errpipe_read, errpipe_write, restore_signals, start_new_session,
               process_group, gid, gids, uid, umask, preexec_fn):
    if (cwd is not None or start_new_session or process_group != -1 or
            gid is not None or gids is not None or uid is not None or
            umask != -1 or preexec_fn is not None):
        _unsupported()
    argv = list(args)
    if not argv:
        raise ValueError('args must name an executable')
    _pack(argv)
    program = None
    for candidate in executables:
        name = _registry_name(candidate)
        encoded = os.fsencode(name)
        if not encoded or b'\0' in encoded:
            raise ValueError('Invalid executable name')
        if _request(8, payload=encoded)[0]:
            program = name
            break
    if program is None:
        raise FileNotFoundError(errno.ENOENT, 'Executable is not registered', argv[0])
    if _registry_name(argv[0]) != program:
        _unsupported()
    argv[0] = program
    arguments = _pack(argv)
    environment = _pack(env if env is not None else
                        [os.fsencode(key) + b'=' + os.fsencode(value) for key, value in os.environ.items()])
    if len(arguments) + len(environment) > 65536:
        raise OSError(errno.E2BIG, 'Arguments and environment exceed 64 KiB')
    if any(fd < 0 or fd == 3 or fd >= 128 or fd == _channel for fd in keep):
        raise OSError(errno.EBADF, 'Invalid inherited descriptor')
    replacements = (p2cread, c2pwrite, errwrite)
    mappings = []
    for target in range(128):
        if target in (3, _channel, errpipe_write):
            continue
        source = replacements[target] if target < 3 and replacements[target] >= 0 else target
        explicit = (target < 3 and replacements[target] >= 0) or target in keep
        try:
            inheritable = _get_inheritable(source)
        except OSError as error:
            if error.errno == errno.EBADF and not explicit:
                continue
            raise
        if explicit or inheritable and (target < 3 or not close_fds):
            mappings.extend((target, source))
    return _request(9, len(arguments), len(environment), len(mappings) // 2,
                    payload=struct.pack('<' + 'I' * len(mappings), *mappings) + arguments + environment)[0]


def _system(command):
    import subprocess
    if isinstance(command, bytes):
        command = os.fsdecode(command)
    return subprocess.call(command, shell=True, close_fds=False) << 8


def install():
    import posix
    import subprocess
    from _efi_filesystem import install as install_filesystem
    install_filesystem(_request)
    methods = {
        'pipe': _pipe, 'dup': _dup, 'dup2': _dup2,
        'get_inheritable': _get_inheritable, 'set_inheritable': _set_inheritable,
        'getpid': lambda: _request(6)[0], 'getppid': lambda: _request(7)[0],
        'waitpid': _waitpid, 'system': _system, 'kill': _unsupported,
        'waitstatus_to_exitcode': _waitstatus_to_exitcode,
        'WIFSTOPPED': lambda status: status & 255 == 127,
        'WSTOPSIG': lambda status: (status >> 8) & 255,
        'WNOHANG': 1,
    }
    for name, value in methods.items():
        setattr(os, name, value)
        setattr(posix, name, value)
    subprocess._can_fork_exec = True
    subprocess._USE_POSIX_SPAWN = False
    subprocess._fork_exec = _fork_exec
    for name in ('waitpid', 'waitstatus_to_exitcode', 'WIFSTOPPED', 'WSTOPSIG', 'WNOHANG'):
        setattr(subprocess._del_safe, name, getattr(os, name))
    # Keep upstream argument, pipe, capture, status and text handling.
    # Reject deadlines before any high-level helper can start a child.
    def reject_timeout(function):
        def call(*args, timeout=None, **kwargs):
            if timeout is not None:
                _unsupported()
            return function(*args, **kwargs)
        return call
    for name in ('run', 'call', 'check_output'):
        setattr(subprocess, name, reject_timeout(getattr(subprocess, name)))
    wait = subprocess.Popen.wait
    communicate = subprocess.Popen.communicate

    def wait_without_timeout(self, timeout=None):
        if timeout is not None:
            _unsupported()
        return wait(self)

    def communicate_without_timeout(self, input=None, timeout=None):
        if timeout is not None:
            _unsupported()
        return communicate(self, input=input)

    subprocess.Popen.wait = wait_without_timeout
    subprocess.Popen.communicate = communicate_without_timeout
