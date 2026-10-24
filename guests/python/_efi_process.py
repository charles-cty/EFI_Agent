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
                  12: errno.ENOMEM, 13: errno.EACCES, 20: errno.ENOTDIR,
                  40: errno.ELOOP, 14: errno.EFAULT, 22: errno.EINVAL, 24: errno.EMFILE,
                  36: errno.ENAMETOOLONG, 44: errno.ENOENT,
                  54: errno.ENOTDIR, 58: errno.ENOTSUP, 29: errno.EIO,
                  76: errno.ENOTCAPABLE}
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
    unsupported = {
        'start_new_session': start_new_session,
        'process_group': process_group != -1,
        'gid': gid is not None,
        'gids': gids is not None,
        'uid': uid is not None,
        'preexec_fn': preexec_fn is not None,
    }
    rejected = [name for name, active in unsupported.items() if active]
    if rejected:
        raise OSError(errno.ENOTSUP, 'Unsupported process options: ' + ', '.join(rejected))
    if umask < -1:
        raise ValueError('umask must be -1 or a non-negative mode')
    argv = list(args)
    if not argv:
        raise ValueError('args must name an executable')
    _pack(argv)
    encoded = os.fsencode(argv[0])
    if not encoded or b'\0' in encoded:
        raise ValueError('Invalid executable name')
    # Upstream Popen supplies PATH candidates. The runtime resolves the name
    # against the explicit child environment and cwd in one operation.
    if not any(candidate == encoded or
               b'/' not in encoded and candidate.endswith(b'/' + encoded)
               for candidate in executables):
        _unsupported()
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
    if cwd is None and umask < 0:
        return _request(9, len(arguments), len(environment), len(mappings) // 2,
                        payload=struct.pack('<' + 'I' * len(mappings), *mappings) +
                        arguments + environment)[0]
    if cwd is None:
        cwd = b''
    else:
        if not os.fspath(cwd):
            raise OSError(errno.ENOENT, os.strerror(errno.ENOENT), cwd)
        cwd = os.fsencode(os.path.join('/proc/self/cwd', os.fsdecode(cwd)))
    if b'\0' in cwd:
        raise ValueError('embedded null byte')
    payload = struct.pack('<' + 'I' * len(mappings), *mappings) + arguments + environment + cwd
    if umask >= 0:
        return _request(23, len(arguments), len(environment), len(mappings) // 2,
                        len(cwd), umask, payload=payload)[0]
    return _request(22, len(arguments), len(environment), len(mappings) // 2,
                    len(cwd), payload=payload)[0]


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
