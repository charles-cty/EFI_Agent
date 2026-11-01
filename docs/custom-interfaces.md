# Project-defined interfaces and parameters

Source audit: 2026-11-01, implementation at commit `0ee83cf`.

This inventory covers guest application entry points, language adapters, custom
WASM imports, private protocols, installation formats, Agent commands,
configuration, and build entry points. It separates changes to upstream
application interfaces from interfaces of EFI Agent itself. Each source link
points to the implementation that defines the contract.

The audit used maintained source and vendor changes against the pristine imports
of BusyBox (`4e5840a`) and QuickJS (`83713b0`). Generated packages, dependency
caches, and historical verification fixtures are not separate interface
definitions. Individual internal Rust methods and test case parameters are
outside this inventory. No application behavior was changed or tested for this
audit.

## 1. Changes visible to application users

### WolfSSH: a project-written client command line

The shipped `wolfssh` command is the client in
[guests/wolfssh/main.c](../guests/wolfssh/main.c). It uses the WolfSSH library.
Its complete command-line contract was written for this project:

```text
wolfssh --host HOST [--port PORT] --user USER \
  --host-key-sha256 HEX --password-file FILE -- COMMAND
```

| Parameter | Project behavior |
| --- | --- |
| `--host HOST` | Required destination hostname or IPv4 address. |
| `--port PORT` | Numeric destination port; default `22`. |
| `--user USER` | Required remote username. |
| `--host-key-sha256 HEX` | Required SHA-256 digest of the raw SSH public-key blob, encoded as exactly 64 hexadecimal digits. This format differs from OpenSSH's `SHA256:` Base64 display. |
| `--password-file FILE` | Required password input file, at most 4096 bytes. Rejects NUL and removes one final LF or CRLF. |
| `-- COMMAND` | Exactly one nonempty argument that contains the remote command string. The shell must quote commands that contain spaces. |

The client makes one password-authentication attempt and opens an exec channel.
It sends stdin EOF. It has no interactive shell, PTY, password prompt, public-key
authentication, `known_hosts`, or first-connection confirmation. It returns the
remote exit status when it receives a value from 0 through 255; setup, transport,
or missing-status failures return 255. Invalid invocation returns 2.

All five named options and the command operand belong in this inventory,
including options that use familiar names. The mandatory hexadecimal pin and
password-file workflow are the largest changes to normal SSH client use.

### QuickJS: a replacement executable entry point

Sources: [main.c](../guests/quickjs/main.c),
[zx.js](../guests/quickjs/zx.js), and [zx port contract](../vendor/zx/PORT.md).

| Interface | Origin and current behavior |
| --- | --- |
| `quickjs -e SCRIPT` | The option spelling exists upstream. The project main accepts this as its only ordinary invocation. Script-file arguments, `--version`, and the upstream option set are absent. |
| `qjs` | A project-installed alias of the same module. It has the same restricted CLI. |
| `--efi-session` | Project-private mode for retained evaluation. Requires a host-created session channel. |
| Global `ash` command template | Project integration of selected zx behavior. The name was changed to `ash` at the user's request. Each command runs a fresh ash child. It does not use the retained shell-tool interpreter. |
| `ash(options)` and mutable `ash` properties | Accept `cwd`, `env`, `input`, `nothrow`, `quiet`, `verbose`, `prefix`, `postfix`, and `quote`. `env` overlays inherited values through shell exports. |
| Global `ProcessPromise` | Project-written WASI adapter. Supports Promise methods, `nothrow`, `quiet`, `verbose`, `text`, `json`, `lines`, `buffer`, `blob`, and `pipe`. |
| `pipe`, `pipe.stdout`, `pipe.stderr`, `pipe.stdall` | Supply completed captured text to a later command template. They do not connect live OS streams. |
| Global `ProcessOutput` and `quote` | Reused zx names and code, explicitly exposed by this port. `captureError` is an added output field. |
| `ProcessOutput.text(encoding)` | Project replacement accepts only `utf8` and `utf-8`. `buffer()` returns a `Uint8Array`; `blob()` requires a Blob implementation supplied by the caller. |
| `quit(status)` | Project global that exits the interpreter; retained direct input then returns to conversation mode. |
| `print`, `console.log`, `console.error` | Project C bindings. `print` and `console.log` use names also supplied by upstream QuickJS helpers; this port supplies its own implementation and adds `console.error`. |
| `__shell(command, input)` | Globally reachable private C bridge used by the adapter. Returns a Promise with captured output and exit information. |
| `__utf8(text)` | Globally reachable private UTF-8 conversion bridge used by the adapter. |
| `__write_stderr(text)` | Globally reachable private C bridge used by the adapter to report zx command diagnostics without adding a newline. |

The adapter supports up to 31 concurrent capture tasks. Command input and each
captured stream are bounded to 512 KiB. The adapter rejects `sync`, `kill`, and
`timeout`. File module loading, upstream `std`/`os`, Node APIs, and timers are
absent. The upstream zx dollar name is private inside the bootstrap closure;
there is no public `$` global.

### Python: standard names with project-installed replacements

Sources: [sitecustomize.py](../guests/python/sitecustomize.py),
[_efi_process.py](../guests/python/_efi_process.py), and
[_efi_filesystem.py](../guests/python/_efi_filesystem.py).

CPython is the unchanged prebuilt interpreter. Startup imports private adapter
modules and changes the following standard interfaces in memory:

| Surface | Replaced names or behavior |
| --- | --- |
| `os` and `posix`: descriptors | `pipe`, `dup`, `dup2`, `get_inheritable`, `set_inheritable`. |
| `os` and `posix`: processes | `getpid`, `getppid`, `waitpid`, `system`, `kill`, `waitstatus_to_exitcode`, `WIFSTOPPED`, `WSTOPSIG`, and `WNOHANG`. `kill` explicitly reports unsupported operation. `waitpid` requires a positive child PID and options 0 or `WNOHANG`. |
| `subprocess`: execution | Replaces private `_fork_exec`, enables `_can_fork_exec`, disables `_USE_POSIX_SPAWN`, and replaces private wait helpers. Public `Popen` operations reach the custom runtime spawn implementation. |
| `subprocess`: timeout rejection | Wraps `run`, `call`, `check_output`, `Popen.wait`, and `Popen.communicate`; non-`None` timeouts fail explicitly. |
| `subprocess`: unsupported arguments | Rejects sessions, process groups, credentials, extra groups, and pre-exec callbacks. A separate executable with a distinct custom `argv[0]` is unsupported. Child cwd, umask, environment, descriptor mapping, and `pass_fds` use project adapters. |
| `os` and `posix`: metadata | `stat`, `lstat`, `fstat`, `chmod`, `fchmod`, `umask`, `statvfs`, `fstatvfs`. |
| `os` and `posix`: paths and files | `mkdir`, `open`, `access`, `chdir`, `getcwd`, `getcwdb`, `scandir`, `remove`, `unlink`, `rmdir`, `link`. |
| Capability sets | Updates `os.supports_fd`, `supports_dir_fd`, `supports_follow_symlinks`, and `supports_effective_ids` to refer to adapter functions. |
| Identity calls | `os`/`posix` `getuid`, `geteuid`, `getgid`, and `getegid` return the fixed guest identity 1. |
| Directory entry types | `scandir` returns project iterator/entry proxies with standard methods. Entry objects are not instances of built-in `os.DirEntry`. |
| Private cwd route | Initializes libc cwd to `/proc/self/cwd`; adapters retain guest cwd identity and preserve original path components. |

The adapters use the private `.efi-process` endpoint described below. They do
not add a public `efi` Python API. The former Python `-e` entry point and
`efi.shell` interface were removed and are not current interfaces.

`-S` and `-I` are upstream CPython options. `-S` disables site startup and these
adapters; `-I` retains them. `PYTHON_BASIC_REPL` is an upstream CPython variable.
The project adds a startup default of `PYTHON_BASIC_REPL=1` through Agent
configuration. It does not invent that variable or monkey-patch pyrepl.

### BusyBox and ash

Sources: [ash entry point](../vendor/busybox/shell/ash.c),
[session loop](../guests/busybox/session.inc),
[task codec](../guests/busybox/tasks.inc),
[identity support](../guests/busybox/support.c), and
[applet scope](busybox-applets.md).

| Interface | Project behavior |
| --- | --- |
| `ash --efi-session` | Private retained-interpreter mode; accepts no further CLI arguments. Reads source and writes completion through `/.efi-session`. |
| `ash --efi-task SIZE FD` | Private child-task entry point. Restores serialized shell state and syntax nodes from a descriptor instead of using `fork`. Serialized data is limited to 512 KiB and syntax depth 256. |
| Identity and account lookup | Fixed UID/GID 1 and the account/group name `agent`; no supplementary identities or host account database. This affects standard applet output such as owner names. |
| Standard `kill`/`pkill` behavior | Supports positive-PID checks with signal 0 and default termination for INT/KILL/TERM. Children are adopted. User handlers, process groups, and other signals are unavailable. This is a restricted standard interface, not new option syntax. |
| Terminal control | Project termios/ioctl adapters expose the controlling terminal during direct input. Ctrl+Q cancels a raw-terminal command. This key has project-specific cancellation meaning. |

The shipped 90 BusyBox names use selected upstream applets, with `bash` entering
the same ash shell as `sh`. The source comparison
found the two private ash options above, with no additional project-defined
applet CLI option names. Bash compatibility, startup-banner suppression, and
selected applet features are upstream configuration choices. They are not new
shell parameters. Port limitations and changed implementations remain separate
from option-name additions.

### curl, jq, and wolfSSL

The curl and jq builds use their upstream executable entry points. The audit
found no project-defined runtime command-line options for either program.
Standard curl trust options such as `--cacert` and `-k` belong to curl. jq filter,
module, and regex options belong to jq.

Their project changes sit in build settings, libc cwd/filesystem wrappers, and
network adapters. wolfSSL is linked as a library and has no shipped command-line
entry point. Sources: [guest driver](../scripts/build_guests.py),
[curl build](../scripts/build_curl.sh), [jq build](../scripts/build_jq.sh),
[POSIX network adapter](../guests/posix/network.c), and
[wolfSSL settings](../guests/wolfssl/user_settings.h).

## 2. Custom runtime and guest-library interfaces

### WASM import namespace `efi_process` (19 functions)

Contract: [efi_process.h](../guests/include/efi_process.h).
Registration: [abi.rs](../crates/wasm/src/abi.rs) and
[terminal.rs](../crates/wasm/src/terminal.rs).

| Import names | Purpose |
| --- | --- |
| `getpid`, `getppid` | Dispatcher process identity. |
| `program_exists` | Exact registry-name query. |
| `spawn` | Spawn with inherited startup environment and selected standard descriptors. |
| `spawn_fds` | Spawn with explicit stdin/stdout/stderr descriptors. |
| `spawn_env_fds` | Spawn with an explicit replacement environment and standard descriptors. |
| `spawn_mapped_fds` | Spawn with an explicit environment and child-target/parent-source descriptor map. |
| `waitpid`, `waitpid_nohang` | Wait or poll for one child. Status is a direct exit code, not a POSIX packed wait status. |
| `cancel_child` | Cancel a direct child's subtree; retain the child as waitable with status 130 and reap descendants. |
| `kill` | Check or terminate one positive PID with the restricted signal set. |
| `pipe`, `input_snapshot` | Create a 64 KiB pipe or a consuming read-only input snapshot of at most 512 KiB. |
| `dup2`, `dupfd` | Duplicate descriptors. |
| `get_fd_flags`, `set_fd_flags` | Descriptor-local close-on-exec bit. |
| `terminal_get`, `terminal_set` | Get `{mode, columns, rows}` or set the three-bit terminal mode. |

These imports are project APIs, not WASI Preview 1. Process operations return
negative POSIX errno values on error. Spawn arguments and environments use
NUL-separated UTF-8 and share a 64 KiB budget. Descriptor indices are below 128;
descriptor 3 is the workspace preopen and cannot be a mapped target. Terminal
mode bits select canonical input, echo, and LF-to-CRLF output conversion.

### WASM import namespace `efi_net` (6 functions)

Contract: [efi_net.h](../guests/include/efi_net.h).
Implementation: [abi.rs](../crates/wasm/src/abi.rs),
[network.rs](../crates/wasm/src/network.rs), and
[socket.rs](../crates/wasm/src/socket.rs).

| Import | Purpose |
| --- | --- |
| `tcp_open` | Create a TCP descriptor. `EFI_NET_CLOEXEC=0x10000` is a project flag. |
| `tcp_connect_fd` | Start or finish connecting an existing descriptor. |
| `tcp_error` | Read and clear shared connection-error state. |
| `tcp_connect` | Combined create/connect operation. |
| `tcp_close` | Drain accepted output and close cooperatively. Ordinary WASI fd close remains abortive for sockets. |
| `resolve_ipv4` | Resolve one IPv4 address through the configured TCP DNS provider. |

These are custom IPv4 client APIs. Connected descriptors use standard WASI
read/write/poll. The project POSIX shim exposes familiar `socket`, `connect`,
`getsockopt`, `send`, `recv`, `getaddrinfo`, and `freeaddrinfo` names above them.
The shim implements a subset; it does not introduce another application CLI.

### Private process/filesystem RPC: `.efi-process`

Sources: [process_control.rs](../crates/wasm/src/process_control.rs),
[filesystem_abi.rs](../crates/wasm/src/filesystem_abi.rs),
[Python caller](../guests/python/_efi_process.py), and
[C metadata caller](../guests/busybox/metadata.c).

An interpreter can open this private process-local endpoint using standard WASI
file operations. It cannot inherit the endpoint into a child. Each write is one
request: a little-endian operation word, argument words, and optional payload.
The basic reply is two signed 32-bit words; some operations return a descriptor
for additional data. Process errors use negative POSIX errno; filesystem replies
carry their own WASI error convention.

| Opcode | Operation |
| --- | --- |
| 1 | Create pipe. |
| 2, 3 | Duplicate to a free descriptor / duplicate to an exact target. |
| 4, 5 | Read / set close-on-exec flags. |
| 6, 7 | Get PID / parent PID. |
| 8 | Query exact program-registry name. |
| 9 | Spawn with arguments, environment, and descriptor mappings. |
| 10 | Wait or poll for a child. |
| 11, 12 | Read / set path permission mode. |
| 13, 14 | Read / set descriptor permission mode. |
| 15 | Set umask and return its previous value. |
| 16, 17 | Get cwd through a reply descriptor / change cwd. |
| 18 | Snapshot directory-entry names from an anchored directory descriptor into a reply stream. |
| 19 | Set or clear a temporary creation-mode override and return the old value. |
| 20, 21 | Unlink file / remove directory. |
| 22 | Spawn with cwd. |
| 23 | Spawn with optional cwd and umask. |
| 24 | Query allocated bytes through a reply descriptor. |
| 25 | Create a hard link. |
| 26 | Query filesystem statistics through a reply descriptor. |
| 27 | Query mount records through a reply descriptor. |

### Private session channel: `/.efi-session`

Sources: [session.rs](../crates/wasm/src/session.rs),
[ash loop](../guests/busybox/session.inc), and
[QuickJS loop](../guests/quickjs/main.c).

The host creates the channel only for a retained session. Requests contain
three little-endian 32-bit words: sequence, source byte count, and a 0/1
completion-display flag, followed by source bytes. Source is limited to 60 KiB
and cannot contain NUL. Replies contain the sequence and signed completion
status. The channel is private and cannot cross spawn. Source and completion
do not use ordinary stdin/stdout/stderr.

### Project C helper APIs

| Header / implementation | Added helpers |
| --- | --- |
| [efi_process.h](../guests/include/efi_process.h), [process.c](../guests/lib/process.c) | `efi_process_pack_argv`, `efi_process_spawn_argv`, `efi_process_spawn_argv_fds`, `efi_process_spawn_argv_env_fds`, `efi_process_spawn_argv_env_mapped`. |
| [efi_shell.h](../guests/include/efi_shell.h), [shell.c](../guests/lib/shell.c) | `efi_shell_start`, `efi_shell_start_input`, `efi_shell_step`, `efi_shell_done`; task and bounded-output structures. |
| [efi_filesystem.h](../guests/include/efi_filesystem.h), [cwd.c](../guests/lib/cwd.c) | `efi_filesystem_fstatat` and libc path wrappers that preserve original components and cwd identity. |
| [efi_timezone.h](../guests/include/efi_timezone.h), [timezone.c](../guests/lib/timezone.c) | `efi_timezone_map`, `efi_timezone_unmap`, `efi_timezone_setting` for the vendored timezone rules engine. |
| [efi_sjlj.h](../guests/sjlj/include/efi_sjlj.h), [runtime.c](../guests/sjlj/runtime.c) | `efi_sjlj_run`, replacement setjmp/longjmp machinery, and stack-pointer helpers. Uses Binaryen Asyncify's `start_unwind`, `stop_unwind`, `start_rewind`, and `stop_rewind` transformation interface. |
| [efi_wasi.h](../guests/include/efi_wasi.h) | Project C convenience wrappers for standard `wasi_snapshot_preview1` imports. Their WASM import names are standard WASI, not another custom namespace. |

## 3. EFI Agent's own public interfaces

### Launcher commands

Sources: [main.rs](../crates/launcher/src/main.rs),
[vm.rs](../crates/launcher/src/vm.rs), [boot.rs](../crates/launcher/src/boot.rs),
[pack.rs](../crates/launcher/src/pack.rs), and
[installation.rs](../crates/launcher/src/installation.rs).

| Command | Parameters |
| --- | --- |
| `efi-agent vm` | `QEMU OVMF_CODE OVMF_VARS EFI_OR_ESP_OR_IMAGE WORKSPACE [--memory-mib MiB]`; RAM defaults to 256 MiB. |
| `efi-agent pack` | `ESP_DIRECTORY NEW_DISK_IMAGE`. |
| `efi-agent package` | `APPLICATION_EFI OUTPUT_DIRECTORY [PREPARED_DIRECTORY] [--vm-only]`. |
| `efi-agent catalog` | `PREPARED_DIRECTORY`; generate and validate the guest catalog. |
| `efi-agent install` | `PREPARED_DIRECTORY WORKSPACE [--guest]`. The `--guest` mode writes a guest staging tree instead of a host workspace export. It is accepted by the implementation but omitted from top-level help. |

### Conversation commands and direct terminal behavior

Sources: [main.rs](../crates/uefi/src/main.rs),
[wasm.rs](../crates/uefi/src/wasm.rs), and
[event_loop.rs](../crates/uefi/src/event_loop.rs).

| Command | Contract |
| --- | --- |
| `/help`, `/caps`, `/status` | Command help, detected capabilities, and actual runtime/API/history status. |
| `/enter [shell\|js]` | Open direct interpreter input; default is shell. |
| `/effort [value]` | Read or change the provider reasoning effort. |
| `/compact` | Summarize completed older exchanges with tools disabled; preserve interpreter state and exact protected call groups. |
| `/clear` | Clear conversation and reset both retained interpreters. |
| `/quit`, `/exit` | Exit EFI Agent. |
| `/wasm load PATH` | Infer a registry name from the filename, removing a case-insensitive `.wasm` suffix. |
| `/wasm load NAME PATH` | Register a module with an explicit name. |
| `/wasm start NAME [ARGS...]` | Start a background root. |
| `/wasm run NAME [ARGS...]` | Run a root with live status and output. |
| `/wasm ps`, `/wasm kill PID`, `/wasm reap PID` | List processes, cancel a process tree, or reap a completed process. `/wasm kill` uses Agent cancellation, separate from guest POSIX signal behavior. |

`/wasm` arguments use whitespace splitting; shell quoting and empty argument
syntax are absent. Direct terminal text is interpreter input. Enter evaluates
source or sends stdin; Ctrl+J inserts a newline; Ctrl+D supplies EOF; Esc/Ctrl+C
cancels cooked execution or clears idle input. Raw-terminal Ctrl+Q cancels.
Cancellation resets the affected interpreter. Terminal input/output stays
outside model history. The launcher also supplies selection/copy/paste handling
and a rapid second Ctrl+C exit gesture.

### Model tools

Source: [agent.rs](../crates/core/src/agent.rs).

Byte budgets, complete tool return contracts and configuration fields are in
[context-budgets.md](context-budgets.md). `limits` in firmware configuration and
JSON `EFI_AGENT_LIMITS` at launch select the same validated fields. `/status`
reports task/summary requests separately and does not convert bytes into tokens.

| Tool | Arguments |
| --- | --- |
| `read` | `path`. |
| `write` | `path`, `content`; UTF-8, create/overwrite, at most 512 KiB. |
| `edit` | `path`, `old_text`, `new_text`; require exactly one matching occurrence. |
| `shell` | `command`; evaluate in the retained ash interpreter. |
| `js` | `code`; evaluate in the retained QuickJS context. |
| `reset_js` | No arguments; release the JS interpreter and its children. |

Tool schemas reject extra properties. Shell and JS tool capture is limited to
64 KiB per stdout/stderr stream with explicit truncation. This is separate from
the 512 KiB command-capture limit inside the JS adapter. Tool stdin is finite
and empty. Direct terminal input has a different I/O contract.

## 4. Configuration and private storage formats

### Firmware configuration and host environment

Sources: [config.rs](../crates/core/src/config.rs) and
[boot.rs](../crates/launcher/src/boot.rs).
The project configuration file is `EFI/AGENT/CONFIG.JSON`.

| JSON field | Host configuration variable | Default / meaning |
| --- | --- | --- |
| `api_base` | `EFI_AGENT_API_BASE` | Required provider URL. |
| `api_key` | `EFI_AGENT_API_KEY` | Required provider credential. |
| `model` | `EFI_AGENT_MODEL` | Required model name. |
| `api_format` | `EFI_AGENT_API_FORMAT` | `chat_completions`; also accepts `responses`. |
| `reasoning_effort` | `EFI_AGENT_REASONING_EFFORT` | `medium`; provider validates the requested effort. |
| `dns_address` | `EFI_AGENT_DNS_ADDRESS` | `[1,1,1,1]` in JSON; dotted IPv4 on the host. |
| `dns_port` | `EFI_AGENT_DNS_PORT` | `53`. |
| `ipv4.address` | `EFI_AGENT_IPV4_ADDRESS` | Optional static address; JSON uses four octets. |
| `ipv4.subnet_mask` | `EFI_AGENT_IPV4_NETMASK` | Static netmask; set all three host IPv4 variables together. |
| `ipv4.gateway` | `EFI_AGENT_IPV4_GATEWAY` | Static gateway. Omitted IPv4 configuration selects DHCP. |
| `workspace` | No host override | Boot-volume path; default `\work`. |
| `ca_certificate` | `EFI_AGENT_CA_CERTIFICATE` | Optional DER certificate path. The host variable names an input file; the launcher copies it to `EFI/AGENT/CA.DER`. |
| `environment` | `EFI_AGENT_ENVIRONMENT` | JSON string-to-string map. Overrides built-in guest defaults, including `PYTHON_BASIC_REPL=1`. |
| No JSON field | `EFI_AGENT_SHELL` | Select a custom UEFI Shell binary for native packaging. |

The config rejects unknown fields. Guest environment names cannot be empty or
contain `=`/NUL; values cannot contain NUL. Startup environment is bounded to
64 KiB. An empty map retains built-in defaults; set `PYTHON_BASIC_REPL` to an
empty string to disable that default. New roots receive resolved defaults;
children inherit current parent values unless supplied a replacement
environment. The host process environment is not copied wholesale.

The run-VM scripts set static QEMU user-network addresses and DNS themselves.
These are project launcher defaults, separate from guest application options.

### Installation, workspace, and boot formats

| Interface / path | Project-defined contract and source |
| --- | --- |
| `programs.json` | Version 1 manifest: `version`, `modules`; each module has `path`, `commands`, `libraries`. Limits: 64 KiB, 128 modules, 512 command names. [Schema](../crates/filesystem/src/installation.rs). |
| Catalog command aliases | 90 BusyBox names, `python`, `python3`, `curl`, `quickjs`, `qjs`, `wolfssh`, `jq`: 97 aliases across six modules. [Catalog generator](../crates/launcher/src/installation.rs). |
| Automatic discovery | Top-level `.wasm` filenames can provide inferred command names; `qjs` and `python3` aliases are added for `quickjs` and `python` when absent. Virtual `/bin` exposes installed commands. [Runtime discovery](../crates/wasm/src/installation.rs). |
| `.efi-agent-fs/` | Private filesystem journal: `format`, `objects`, and alternating `state0`/`state1` records preserve metadata and inode/link state on FAT. [Journal](../crates/filesystem/src/namespace/journal.rs). |
| `.efi-agent-workspace.json` | Host export metadata: `format`, `version`, `entries`; cached entries contain observed host data, guest attributes, and optional link data. [Host workspace](../crates/launcher/src/workspace.rs). |
| `.efi-process`, `.efi-session` | Reserved runtime endpoint names; not ordinary persistent user files. |
| `/bin`, `/dev`, `/proc`, `/tmp` | Project virtual mounts with standard-looking paths. `/proc/self/cwd` also acts as the libc cwd route. [Filesystem](../crates/wasm/src/filesystem.rs), [process records](../crates/wasm/src/filesystem/process.rs). |
| `EFI/AGENT/VM.TXT` | Private `serial\n` marker selects VM serial-console behavior. [Boot packaging](../crates/launcher/src/boot.rs). |
| `EFI/AGENT/AGENT.EFI`, `EFI/BOOT/startup.nsh` | Project boot layout. Native startup uses the upstream UEFI Shell command language. [Boot packaging](../crates/launcher/src/boot.rs). |

### Launcher/firmware serial protocol

Sources: [serial.rs](../crates/core/src/serial.rs),
[selection.rs](../crates/launcher/src/selection.rs), and
[input.rs](../crates/core/src/input.rs).

| Interface | Contract |
| --- | --- |
| `ESC ] 777 ; efi-agent ; ready BEL` | Private firmware-readiness notification. |
| `ESC ] 778 ; efi-agent ; view ; ... BEL` | Private selection-view JSON frame. Contains terminal mode, dimensions, transcript/input revisions and coordinates, offsets, and cursor data. |
| Input escape sequences | Uses terminal conventions for keyboard, resize, bracketed paste, and mouse events. The project maps them to its own UI actions; their conventional escape spellings are not new WASM APIs. |

## 5. Build and verification interfaces

These interfaces configure project tooling. They do not extend guest commands.

| Entry point | Project parameters |
| --- | --- |
| [scripts/build.ps1](../scripts/build.ps1) | `-Profile release\|debug`, `-Guests`, `-WasiSdk`, `-Binaryen`, `-MsysBash`, `-Jobs`, `-VmOnly`. |
| [scripts/build.sh](../scripts/build.sh) | `--profile release\|debug`, `--vm-only`, `--guests`, `--wasi-sdk`, `--binaryen`, `--jobs`, `--help`. |
| [scripts/build_guests.py](../scripts/build_guests.py) | `--component all\|quickjs\|wolfssl\|curl\|wolfssh\|jq\|python\|shell`, `--wasi-sdk`, `--binaryen`, `--output`, `--source`, `--wolfssl`, `--archive`, `--msys-bash`, `--jobs`, `--reconfigure`. BusyBox is built as part of `all` or through its separate driver. |
| [scripts/build_busybox.py](../scripts/build_busybox.py) | `--wasi-sdk`, `--binaryen`, `--output`, `--build-fixtures`; PowerShell wrapper uses `-WasiSdk`, `-Binaryen`, `-Output`, `-BuildFixtures`. |
| Component PowerShell wrappers | `build_quickjs`: `-WasiSdk`, `-Source`, `-Output`; `build_wolfssl`: adds `-Jobs`; `build_wolfssh`: adds `-WolfSsl`; `build_curl`: `-WasiSdk`, `-WolfSsl`, `-Source`, `-Output`, `-MsysBash`, `-Jobs`, `-Reconfigure`; `build_jq`: the same except `-WolfSsl`. [Scripts](../scripts). |
| Fetch and preparation | [fetch_guest_source.py](../scripts/fetch_guest_source.py): component plus `--source`; `fetch_curl.ps1`, `fetch_wolfssl.ps1`, `fetch_wolfssh.ps1`: `-Source`; [prepare_cpython.ps1](../scripts/prepare_cpython.ps1): `-Archive`, `-Output`; [prepare_timezone.py](../scripts/prepare_timezone.py): `--output`. |
| Internal shell build drivers | [build_curl.sh](../scripts/build_curl.sh): positional source, SDK, project, build, jobs, wolfSSL, platform; [build_jq.sh](../scripts/build_jq.sh): source, SDK, build, jobs, platform. |
| [scripts/run-vm.ps1](../scripts/run-vm.ps1) | `-Launcher`, `-Qemu`, `-Code`, `-Vars`, `-Esp`, `-Workspace`. The Bash wrapper uses fixed project-relative defaults. |
| Toolchain variables | `WASI_SDK_PATH`, `BINARYEN_PATH`. `CARGO_TARGET_DIR` is an upstream Cargo variable reused by the build. |
| Fixture builders | `build_wasm_c_fixture.ps1`: `-WasiSdk`, `-Output`; `build_wasm_sjlj.ps1`: also `-Binaryen`, `-Optimization`; `check_busybox.ps1`: `-WasiSdk`, `-Output`. |
| Native integration runner | [run_guest.rs](../crates/wasm/examples/run_guest.rs): program directory, optional leading `--config PATH`, then guest arguments or private `--lifecycle`/`--sessions` driver mode. |
| Retained-session integration driver | [sessions.rs](../crates/wasm/examples/support/sessions.rs): operation/source-file pairs for `shell`, `js`, `repl-shell`, `repl-js`, `eof-shell`, `eof-js`, `cancel-shell`, `cancel-js`, `reset_js`, `clear`. |
| Test-only environment | `EFI_AGENT_ASH` selects a real BusyBox module for ignored host integrations. `EFI_AGENT_CLIPBOARD_TEST_READER` and `EFI_AGENT_CLIPBOARD_TEST_WAYLAND_READER` select test readers in launcher tests. |

The `test_*` and `smoke_*` entry points have fixture paths, peer addresses,
timeouts, and VM/tool selections for their individual checks. They are
verification controls, not additional production interfaces. The files under
`scripts/fixtures` are test programs. Compiler, linker, Binaryen, QEMU, Cargo,
configure, and upstream application options used by scripts retain their
upstream ownership.
