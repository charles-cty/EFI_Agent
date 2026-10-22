"""Install the EFI Agent process backend before application imports."""
import os
import sys

try:
    from _efi_process import install
    install()
except BaseException:
    import traceback
    traceback.print_exc()
    os._exit(1)
