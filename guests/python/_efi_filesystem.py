"""Expose namespace permissions through standard Python filesystem methods."""
import errno
import operator
import os
import stat
import struct


def install(request):
    import posix
    original_stat = os.stat
    original_fstat = os.fstat
    original_mkdir = os.mkdir
    original_open = os.open
    original_chdir = os.chdir
    original_scandir = os.scandir

    def at_path(path, dir_fd):
        path = os.fspath(path)
        if os.path.isabs(path):
            return path, 3
        if dir_fd is None:
            cwd = b'/proc/self/cwd' if isinstance(path, bytes) else '/proc/self/cwd'
            return os.path.join(cwd, path), 3
        return path, operator.index(dir_fd)

    def metadata(operation, fd, flags=1, mode=0, path=None):
        if fd < 0 or fd >= 128:
            raise OSError(errno.EBADF, os.strerror(errno.EBADF))
        if path is not None:
            encoded = os.fsencode(path)
            if b'\0' in encoded:
                raise ValueError('embedded null byte')
            words = (fd, flags, mode) if operation == 12 else (fd, flags)
        else:
            encoded = b''
            words = (fd, mode) if operation == 14 else (fd,)
        error, value = request(operation, *words, payload=encoded)
        if error:
            raise OSError(error, os.strerror(error), path)
        return value

    def allocation(fd, flags=1, path=None):
        encoded = b'' if path is None else os.fsencode(path)
        error, reply = request(24, fd, flags, payload=encoded)
        if error:
            raise OSError(error, os.strerror(error), path)
        try:
            data = os.read(reply, 8)
            if len(data) != 8:
                raise OSError(errno.EIO, 'incomplete allocation response')
            return struct.unpack('<Q', data)[0]
        finally:
            os.close(reply)

    def filesystem_statistics(fd, path=None):
        if not 0 <= fd < 128:
            raise OSError(errno.EBADF, os.strerror(errno.EBADF))
        encoded = b'' if path is None else os.fsencode(path)
        if b'\0' in encoded:
            raise ValueError('embedded null byte')
        error, reply = request(26, fd, int(path is not None), payload=encoded)
        if error:
            raise OSError(error, os.strerror(error), path)
        try:
            data = os.read(reply, 96)
            if len(data) != 96:
                raise OSError(errno.EIO, 'incomplete filesystem statistics response')
            fields = struct.unpack('<12Q', data)
            return os.statvfs_result(fields[:8] + fields[9:11], {'f_fsid': fields[8]})
        finally:
            os.close(reply)

    def statvfs(path):
        if isinstance(path, int):
            return fstatvfs(path)
        resolved, fd = at_path(path, None)
        return filesystem_statistics(fd, resolved)

    def fstatvfs(fd):
        return filesystem_statistics(operator.index(fd))

    def replace_mode(value, mode, allocated):
        fields = list(value)
        fields[0] = stat.S_IFMT(value.st_mode) | mode
        fields[4] = fields[5] = 1
        extra = {name: getattr(value, name) for name in (
            'st_atime', 'st_mtime', 'st_ctime', 'st_atime_ns', 'st_mtime_ns', 'st_ctime_ns')}
        extra['st_blocks'] = (allocated + 511) // 512
        return os.stat_result(fields, extra)

    def stat_path(path, *, dir_fd=None, follow_symlinks=True):
        if isinstance(path, int):
            if dir_fd is not None or not follow_symlinks:
                raise ValueError('fd does not accept dir_fd or follow_symlinks')
            return fstat(path)
        value = original_stat(path, dir_fd=dir_fd, follow_symlinks=follow_symlinks)
        resolved, fd = at_path(path, dir_fd)
        mode = metadata(11, fd, int(bool(follow_symlinks)), path=resolved)
        return replace_mode(value, mode, allocation(fd, int(bool(follow_symlinks)), resolved))

    def lstat(path, *, dir_fd=None):
        return stat_path(path, dir_fd=dir_fd, follow_symlinks=False)

    def fstat(fd):
        fd = operator.index(fd)
        value = original_fstat(fd)
        return replace_mode(value, metadata(13, fd), allocation(fd))

    def checked_mode(mode):
        # POSIX ignores type/high bits; namespace modes contain only permissions.
        return operator.index(mode) & 0o7777

    def chmod(path, mode, *, dir_fd=None, follow_symlinks=True):
        mode = checked_mode(mode)
        if isinstance(path, int):
            if dir_fd is not None or not follow_symlinks:
                raise ValueError('fd does not accept dir_fd or follow_symlinks')
            return fchmod(path, mode)
        resolved, fd = at_path(path, dir_fd)
        metadata(12, fd, int(bool(follow_symlinks)), mode, resolved)

    def fchmod(fd, mode):
        metadata(14, operator.index(fd), mode=checked_mode(mode))

    def umask(mask):
        return request(15, operator.index(mask) & 0o777)[0]

    def create(function, path, mode, *args, **kwargs):
        # Preview 1 omits the POSIX mode argument. Apply it during creation,
        # including through dangling links, without changing existing files.
        _, previous = request(19, checked_mode(mode))
        try:
            return function(path, *args, **kwargs)
        finally:
            request(19, previous & 0xffffffff)

    def mkdir(path, mode=0o777, *, dir_fd=None):
        return create(original_mkdir, path, mode, checked_mode(mode), dir_fd=dir_fd)

    def open_path(path, flags, mode=0o777, *, dir_fd=None):
        flags = operator.index(flags)
        if flags & os.O_CREAT:
            return create(original_open, path, mode, flags, checked_mode(mode), dir_fd=dir_fd)
        return original_open(path, flags, checked_mode(mode), dir_fd=dir_fd)

    def access(path, mode, *, dir_fd=None, effective_ids=False, follow_symlinks=True):
        mode = operator.index(mode)
        if mode & ~(os.R_OK | os.W_OK | os.X_OK):
            raise ValueError('invalid access mode')
        try:
            value = stat_path(path, dir_fd=dir_fd, follow_symlinks=follow_symlinks)
        except OSError:
            return False
        return (value.st_mode >> 6) & mode == mode

    def chdir(path):
        path = os.fspath(path)
        if not path:
            raise OSError(errno.ENOENT, os.strerror(errno.ENOENT), path)
        base = b'/proc/self/cwd' if isinstance(path, bytes) else '/proc/self/cwd'
        encoded = os.fsencode(os.path.join(base, path))
        if b'\0' in encoded:
            raise ValueError('embedded null byte')
        error, _ = request(17, payload=encoded)
        if error:
            raise OSError(error, os.strerror(error), path)

    def getcwdb():
        fd = request(16)[0]
        try:
            chunks = []
            while chunk := os.read(fd, 4096):
                chunks.append(chunk)
            return b''.join(chunks)
        finally:
            os.close(fd)

    def getcwd():
        return os.fsdecode(getcwdb())

    class ScanAnchor:
        def __init__(self, fd):
            self.fd = -1
            self.fd = os.dup(fd)

        def __del__(self):
            if self.fd >= 0:
                os.close(self.fd)

    class Entry:
        def __init__(self, entry, base, anchor):
            self._entry = entry
            self._base = base
            self._anchor = anchor
            self._stats = {}

        @property
        def name(self):
            return self._entry.name

        @property
        def path(self):
            return self._entry.path

        def __fspath__(self):
            return self.path

        def inode(self):
            return self._entry.inode()

        def is_symlink(self):
            return self._entry.is_symlink()

        def is_dir(self, *, follow_symlinks=True):
            return self._entry.is_dir(follow_symlinks=follow_symlinks)

        def is_file(self, *, follow_symlinks=True):
            return self._entry.is_file(follow_symlinks=follow_symlinks)

        def stat(self, *, follow_symlinks=True):
            follow = bool(follow_symlinks)
            if follow not in self._stats:
                value = self._entry.stat(follow_symlinks=follow)
                if self._anchor is None:
                    fd = 3
                    path = os.path.join(self._base, self.name)
                else:
                    fd = self._anchor.fd
                    path = self.name
                mode = metadata(11, fd, int(follow), path=path)
                self._stats[follow] = replace_mode(value, mode, allocation(fd, int(follow), path))
            return self._stats[follow]

    class FdEntry:
        def __init__(self, name, anchor):
            self.name = self.path = name
            self.anchor = anchor
            self.cache = {}

        def inode(self):
            return self.stat(follow_symlinks=False).st_ino

        def stat(self, *, follow_symlinks=True):
            follow = bool(follow_symlinks)
            if follow not in self.cache:
                self.cache[follow] = original_stat(self.name, dir_fd=self.anchor.fd, follow_symlinks=follow)
            return self.cache[follow]

        def is_symlink(self):
            return stat.S_ISLNK(self.stat(follow_symlinks=False).st_mode)

        def is_dir(self, *, follow_symlinks=True):
            try:
                return stat.S_ISDIR(self.stat(follow_symlinks=follow_symlinks).st_mode)
            except FileNotFoundError:
                return False

        def is_file(self, *, follow_symlinks=True):
            try:
                return stat.S_ISREG(self.stat(follow_symlinks=follow_symlinks).st_mode)
            except FileNotFoundError:
                return False

    def fd_entries(anchor):
        error, fd = request(18, anchor.fd)
        if error:
            raise OSError(error, os.strerror(error))
        data = bytearray()
        try:
            while block := os.read(fd, 65536):
                data.extend(block)
        finally:
            os.close(fd)
        offset = 0
        result = []
        while offset < len(data):
            length = struct.unpack_from('<I', data, offset)[0]
            offset += 4
            name = os.fsdecode(bytes(data[offset:offset + length]))
            offset += length
            if name not in ('.', '..'):
                result.append(FdEntry(name, anchor))
        return iter(result)

    class Scanner:
        def __init__(self, path):
            if isinstance(path, int):
                self._anchor = ScanAnchor(path)
                self._base = None
                try:
                    self._iterator = fd_entries(self._anchor)
                except BaseException:
                    self._anchor = None
                    raise
            else:
                self._anchor = None
                self._base, _ = at_path(path, None)
                self._iterator = original_scandir(path)

        def __iter__(self):
            return self

        def __next__(self):
            try:
                entry = next(self._iterator)
            except StopIteration:
                self.close()
                raise
            return Entry(entry, self._base, self._anchor)

        def close(self):
            if self._base is not None:
                self._iterator.close()
            else:
                self._iterator = iter(())
            self._anchor = None

        def __enter__(self):
            return self

        def __exit__(self, *exception):
            self.close()

    def scandir(path='.'):
        return Scanner('.' if path is None else path)

    def remove(path, *, dir_fd=None):
        resolved, directory_fd = at_path(path, dir_fd)
        directory_fd = operator.index(directory_fd)
        if directory_fd < 0 or directory_fd >= 128:
            raise OSError(errno.EBADF, os.strerror(errno.EBADF), path)
        encoded = os.fsencode(resolved)
        if b'\0' in encoded:
            raise ValueError('embedded null byte')
        error, _ = request(20, directory_fd, payload=encoded)
        if error:
            raise OSError(error, os.strerror(error), path)

    def rmdir(path, *, dir_fd=None):
        resolved, directory_fd = at_path(path, dir_fd)
        directory_fd = operator.index(directory_fd)
        if directory_fd < 0 or directory_fd >= 128:
            raise OSError(errno.EBADF, os.strerror(errno.EBADF), path)
        encoded = os.fsencode(resolved)
        if b'\0' in encoded:
            raise ValueError('embedded null byte')
        error, _ = request(21, directory_fd, payload=encoded)
        if error:
            raise OSError(error, os.strerror(error), path)

    def link(src, dst, *, src_dir_fd=None, dst_dir_fd=None, follow_symlinks=True):
        source, source_fd = at_path(src, src_dir_fd)
        destination, destination_fd = at_path(dst, dst_dir_fd)
        if not (0 <= source_fd < 128 and 0 <= destination_fd < 128):
            raise OSError(errno.EBADF, os.strerror(errno.EBADF))
        source = os.fsencode(source)
        destination = os.fsencode(destination)
        if b'\0' in source or b'\0' in destination:
            raise ValueError('embedded null byte')
        error, _ = request(25, source_fd, destination_fd, int(bool(follow_symlinks)),
                           len(source), payload=source + destination)
        if error:
            raise OSError(error, os.strerror(error), src, None, dst)

    methods = {'stat': stat_path, 'lstat': lstat, 'fstat': fstat,
               'chmod': chmod, 'fchmod': fchmod, 'umask': umask,
               'mkdir': mkdir, 'open': open_path, 'access': access, 'chdir': chdir,
               'scandir': scandir, 'getcwd': getcwd, 'getcwdb': getcwdb}
    methods.update({'remove': remove, 'unlink': remove, 'rmdir': rmdir, 'link': link,
                    'statvfs': statvfs, 'fstatvfs': fstatvfs})
    for name, function in methods.items():
        previous = getattr(os, name, None)
        for capability in ('supports_fd', 'supports_dir_fd', 'supports_follow_symlinks', 'supports_effective_ids'):
            supported = getattr(os, capability)
            if previous in supported:
                supported.remove(previous)
                supported.add(function)
        setattr(os, name, function)
        setattr(posix, name, function)
    os.supports_fd.add(chmod)
    os.supports_dir_fd.add(chmod)
    os.supports_follow_symlinks.add(chmod)
    os.supports_effective_ids.add(access)
    os.supports_fd.add(scandir)
    os.supports_fd.add(statvfs)
    os.supports_dir_fd.add(link)
    os.supports_follow_symlinks.add(link)
    os.getuid = posix.getuid = lambda: 1
    os.geteuid = posix.geteuid = lambda: 1
    os.getgid = posix.getgid = lambda: 1
    os.getegid = posix.getegid = lambda: 1
    original_chdir('/proc/self/cwd')
