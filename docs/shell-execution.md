# UEFI Shell integration

Native packaging boots EDK II UEFI Shell 2.2, release 26H1. The startup script
connects installed drivers and starts `EFI\AGENT\AGENT.EFI` using the Shell's
`homefilesystem` mapping. It selects the common 80x25 console mode so display
and serial console sinks agree on the cursor bounds. Esc at the Shell countdown skips the script. Agent
can also be started directly. `/caps` reports whether the Shell protocol is
installed. A model Shell command tool is not implemented in this stage.

The Shell source is pinned by the shallow `vendor/uefi-shell` submodule. The
build output is supplied through `EFI_AGENT_SHELL` and is not stored in Git.
Packaging includes the submodule license and source record on each boot volume.
The VM serial entry point remains Agent; its Shell binary is a separate tool.

## Command tool design constraints

UEFI Shell 2.2 provides `EFI_SHELL_PROTOCOL.Execute`. `uefi-raw` 0.17 exposes
this interface. It takes an image handle, a UTF-16 command line, an optional
environment, and a command status output. The interface returns an EFI status;
the command status is a separate result. Both must be recorded. The protocol
must be opened without exclusive access, since the parent Shell owns it.

Before enabling model-driven commands:

- Define permissions and the commands or device access they permit. Shell
  commands can change files outside the configured workspace, access other
  volumes, load images, modify firmware variables, or reset the machine.
- Set and restore the Shell working directory using the mapping of Agent's
  boot volume and configured workspace. Do not assume `fs0:` or the parent
  Shell's current directory. Test multiple FAT volumes and paths with spaces.
- Capture stdout and stderr with bounded storage. Handle the Shell's UTF-16
  output, malformed text, empty output, and truncation. Redirection changes
  command parsing; quoting and metacharacters must have explicit semantics.
- Keep command status separate from output and transport failure. Test a
  successful command, a command failure, and an unavailable executable.
- Restore the console, directory, and environment after every returning
  command. Redraw the interactive interface after command output.
- Specify cancellation limits. Execute is synchronous; it offers no process
  isolation or general kill operation. The Shell execution-break event alone
  cannot guarantee that a command returns. A synchronous command can also
  prevent the interface from polling Escape. Do not promise a hard timeout.
- Verify the complete model tool loop under actual QEMU. A detected Shell
  protocol alone is not proof of correct command execution or output capture.

The current implementation keeps the parent Shell available and probes its
protocol. It does not call Execute or expose a command tool to the model.
