"""Run a fresh guest ash process and capture its output."""

from _efi_shell import run as _run


class CompletedCommand:
    def __init__(self, status, stdout_bytes, stderr_bytes):
        self.status = status
        self.stdout_bytes = stdout_bytes
        self.stderr_bytes = stderr_bytes

    @property
    def stdout(self):
        return self.stdout_bytes.decode("utf-8", "replace")

    @property
    def stderr(self):
        return self.stderr_bytes.decode("utf-8", "replace")


class ShellError(RuntimeError):
    def __init__(self, result):
        super().__init__(f"Shell exited with status {result.status}")
        self.result = result


def quote(value):
    """Quote one value as one ash word, including empty strings."""
    text = str(value)
    if "\0" in text:
        raise ValueError("NUL in shell argument")
    return "'" + text.replace("'", "'\\''") + "'"


def shell(command, *, check=True):
    """Run shell text, or quote each word in a list or tuple."""
    if isinstance(command, (list, tuple)):
        command = " ".join(quote(word) for word in command)
    elif not isinstance(command, str):
        raise TypeError("Expected shell text or a list of words")
    result = CompletedCommand(*_run(command))
    if check and result.status != 0:
        raise ShellError(result)
    return result
