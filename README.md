# EFI Agent

A Rust coding agent that runs as an x86-64 UEFI application with a Ratatui
interface. It uses `no_std` and `alloc`, and does not load an operating system.
The firmware application calls the model API directly over TCP4 and HTTPS,
parses streamed responses, and executes file tools on its boot volume.

See [UEFI safety](docs/uefi-safety.md) for UEFI safety rules.

## Daily builds with Cargo

Use Rust 1.93 or later. On Linux, run these commands from a Linux-local checkout
(use a directory under `~/` when working in WSL2):

```bash
rustup target add x86_64-unknown-uefi
cargo build -p efi-agent
cargo build -p efi-agent-uefi --target x86_64-unknown-uefi
```

On Windows, use PowerShell 7.4 or later and the same commands with `rustup.exe`
and `cargo.exe`. Add `--release` to both builds for Release mode. A plain
`cargo build` builds the host launcher and shared core, not the UEFI target.

The Debug launcher is `target/debug/efi-agent` (`efi-agent.exe` on Windows).
The UEFI application is
`target/x86_64-unknown-uefi/debug/efi-agent-uefi.efi`. Release builds use
`release` in both paths. The launcher accepts this `.efi` file directly and
prepares a private VM boot disk; daily builds do not need a packaging script.

## API setup

The UEFI application supports Chat Completions and Responses APIs. The API base
is the provider root, for example `https://provider.example/v1`; the application
appends `/chat/completions` or `/responses`. No host model service is started.

For VM runs and bare-metal packaging, set these values before launching or building:

```bash
export EFI_AGENT_API_BASE='https://provider.example/v1'
export EFI_AGENT_API_KEY='your-key'
export EFI_AGENT_MODEL='your-model'
```

On Windows:

```powershell
$env:EFI_AGENT_API_BASE = 'https://provider.example/v1'
$env:EFI_AGENT_API_KEY = 'your-key'
$env:EFI_AGENT_MODEL = 'your-model'
```

Optional variables:

| Variable | Default | Purpose |
|---|---|---|
| `EFI_AGENT_API_FORMAT` | `chat_completions` | Set `responses` to use Responses API |
| `EFI_AGENT_REASONING_EFFORT` | `medium` | Provider reasoning effort |
| `EFI_AGENT_DNS_ADDRESS` | `1.1.1.1` | IPv4 DNS server; DNS queries use TCP |
| `EFI_AGENT_DNS_PORT` | `53` | DNS server port |
| `EFI_AGENT_CA_CERTIFICATE` | unset | Host path to an additional private CA certificate in DER format |

Both build scripts use these same environment variables. You do not need to
create a config file. They generate the firmware configuration automatically.

API credentials are available to the firmware:
VM launches write them into a private temporary boot disk; native packages
contain them in `EFI/AGENT/CONFIG.JSON`. Protect the boot media and generated
images. Credentials are sent to the provider only after TLS verification.
Use HTTPS for real provider access; HTTP is available for explicit local tests.

TLS verifies the certificate chain, hostname, and expiry against bundled public
CA roots. It requires a working UEFI RNG protocol and a correct firmware clock.
An unspecified firmware timezone is treated as UTC. For a private CA, set
`EFI_AGENT_CA_CERTIFICATE` to its host DER file path. VM launch and native
packaging copy that file into `EFI/AGENT/CA.DER` and set the firmware path
automatically. Verification cannot be disabled.

## VM launch

Install QEMU and matching OVMF CODE and VARS images containing VirtioSerialDxe,
VirtioNetDxe, and VirtioRngDxe. Linux requires KVM access; Windows requires WHPX.
The launcher uses the current terminal, without a graphical QEMU window.

On Ubuntu or Debian:

```bash
sudo apt update
sudo apt install build-essential pkg-config qemu-system-x86 ovmf
mkdir -p artifacts/firmware
ovmf_dir=$(dpkg-query -L ovmf | awk '/\/OVMF_CODE_4M[.]fd$/ { sub(/\/[^/]*$/, ""); print }')
test -n "$ovmf_dir" && \
  cp "$ovmf_dir/OVMF_CODE_4M.fd" "$ovmf_dir/OVMF_VARS_4M.fd" artifacts/firmware/
test -r /dev/kvm && test -w /dev/kvm
```

If the firmware pair is absent, use a package that supplies it. If KVM access
fails, enable hardware virtualization and grant your user access. WSL2 also
needs nested virtualization support. For clipboard support on Wayland without
data-control or XWayland, install `wl-clipboard`.

After building with Cargo and setting the API values:

```bash
mkdir -p workspace
./target/debug/efi-agent vm qemu-system-x86_64 \
  ./artifacts/firmware/OVMF_CODE_4M.fd ./artifacts/firmware/OVMF_VARS_4M.fd \
  ./target/x86_64-unknown-uefi/debug/efi-agent-uefi.efi ./workspace
```

On Windows:

```powershell
$qemu = (Get-Command qemu-system-x86_64.exe -CommandType Application -ErrorAction Stop).Source
New-Item -ItemType Directory -Force -Path ./workspace | Out-Null
./target/debug/efi-agent.exe vm $qemu ./artifacts/firmware/OVMF_CODE_4M.fd ./artifacts/firmware/OVMF_VARS_4M.fd ./target/x86_64-unknown-uefi/debug/efi-agent-uefi.efi ./workspace
```

You can also supply `artifacts/esp` or `artifacts/efi-agent-vm.img` as the boot
input after packaging. Release builds use `release` in the executable paths.
Append `--memory-mib 256` to allocate 256 MiB (default: 128 MiB).

The launcher copies the host workspace into a private writable FAT disk. Guest
file tools operate there; changed files are saved back after QEMU stops. Host
files do not change during the session. Conflicting host edits prevent saving;
the launcher reports an error and retains the disk for recovery. Workspace
contents are limited by FAT packaging to 48 MiB, 4096 entries, and 32 directory
levels, including boot files. Symlinks and special files are rejected.
Firmware variables use a temporary snapshot. The supplied boot input and OVMF
VARS template are not modified.

Use `/caps` and `/status` to check capabilities and API configuration. `/effort`
shows the setting; `/effort high` changes it for future requests. `/clear` clears
model history, Esc cancels a request, and `/exit` stops the VM. A cancelled
provider call is not retried. An incomplete stream cannot execute tool calls.

### Prepare OVMF manually on Windows

This procedure does not require Linux or an EDK2 build.

1. Open the [Ubuntu package search for ovmf](https://packages.ubuntu.com/search?keywords=ovmf).
   Select a currently supported Ubuntu release, open its `ovmf` package page,
   and follow the download link for the current `ovmf_*_all.deb` package.
   The package contains VM firmware that also works with Windows QEMU.
2. Open the package in 7-Zip. Open its `data.tar.*` member and the inner tar
   archive, then browse to `usr/share/OVMF`.
3. Create `artifacts\firmware` in the project directory. Extract `OVMF_CODE_4M.fd`
   and `OVMF_VARS_4M.fd` into that directory. Do not select `.ms` or `.secboot`
   variants. Do not mix package versions or 2 MiB and 4 MiB flash layouts.
4. Use the pair in the launch command below. After boot, confirm that the agent
   interface appears, run `/caps` to inspect firmware capabilities, and
   send a prompt to check the network and model request.

`OVMF_CODE_4M.fd` contains the firmware code. `OVMF_VARS_4M.fd` is the matching
UEFI variable-store template. Keep the extracted template: the launcher uses a
temporary snapshot and does not modify it.

### Copy OVMF from WSL instead

If your WSL distribution already provides OVMF, you can copy its firmware pair
instead of downloading and unpacking a package on Windows. On Ubuntu or Debian
under WSL, install or update the package if needed:

```bash
sudo apt update
sudo apt install ovmf
ovmf_dir=$(dpkg-query -L ovmf | awk '/\/OVMF_CODE_4M[.]fd$/ { sub(/\/[^/]*$/, ""); print }')
test -n "$ovmf_dir" && ls -l "$ovmf_dir/OVMF_CODE_4M.fd" "$ovmf_dir/OVMF_VARS_4M.fd"
wslpath -w "$ovmf_dir"
```

The last command prints the Windows UNC path for your distribution's OVMF
directory. If the package does not contain this firmware pair, select a package
that does before continuing. Then,

```powershell
$source = '<Windows path printed by wslpath>'
New-Item -ItemType Directory -Force -Path .\artifacts\firmware | Out-Null
Copy-Item -LiteralPath (Join-Path $source 'OVMF_CODE_4M.fd') -Destination .\artifacts\firmware\OVMF_CODE_4M.fd
Copy-Item -LiteralPath (Join-Path $source 'OVMF_VARS_4M.fd') -Destination .\artifacts\firmware\OVMF_VARS_4M.fd
```

## Release packages for VM and bare metal

The scripts default to Release for both programs. Set the API environment
variables described above, then run:

```bash
bash scripts/build.sh
```

```powershell
./scripts/build.ps1
```

Select Debug with `--profile debug` or `-Profile debug`. Set `CARGO_TARGET_DIR`
to change the target directory for both builds. Both scripts use the compiled
launcher's `package` command and create:

| Path | Contents |
|---|---|
| `artifacts/esp` | VM boot tree with `EFI/BOOT/BOOTX64.EFI` and `EFI/AGENT/VM.TXT` |
| `artifacts/efi-agent-vm.img` | GPT/FAT32 VM boot disk |
| `artifacts/native-esp` | Shell-first native boot tree, Agent, `EFI/AGENT/CONFIG.JSON`, and `work` |
| `artifacts/efi-agent-native.img` | GPT/FAT32 native boot disk with the same config and workspace |

The native tree has no `VM.TXT`. Existing native workspace files are retained
and included in later packages. Images contain a 64 MiB ESP and are 66 MiB in
total. Each generated image is replaced only after packaging succeeds. Scripts
never write physical disks. To package a custom tree separately:

```bash
./target/release/efi-agent pack ./custom-esp ./new-disk.img
```

On Windows, use `efi-agent.exe`. `pack` requires `EFI/BOOT/BOOTX64.EFI`, refuses
to overwrite an existing output, and requires output outside the source tree.

## Bare-metal UEFI run

The target needs x86-64 UEFI firmware, a writable FAT ESP, firmware text input
and output, and a NIC driver that exposes TCP4. Enable the firmware network
stack if needed. Connect to a network with DHCP and DNS TCP access to the server
specified in the config. Operating-system NIC support alone is not sufficient.

1. Set the API environment variables with the actual provider credentials, then
   build the native package.
2. Copy `artifacts/native-esp` contents to the root of a bootable USB FAT32 ESP.
   Ensure `EFI/AGENT/VM.TXT` is absent. Alternatively, write
   `artifacts/efi-agent-native.img` to a dedicated USB drive with an image writer;
   this replaces the drive's contents.
3. Firmware must permit the unsigned Shell and application to run. Disable
   Secure Boot for this run unless you sign both and enroll the signing key.
4. Select the drive's UEFI entry in the firmware boot menu. The EDK II Shell
   discovered from the submodule's EDK2 build output (or supplied as
   `EFI_AGENT_SHELL`) runs `connect -r` and starts
   `EFI\AGENT\AGENT.EFI` on its own volume.
   Press Esc during the Shell countdown to stay at the Shell prompt. Run
   `%homefilesystem%\EFI\AGENT\AGENT.EFI` to start Agent manually. Firmware can
   also launch that Agent path directly; Agent connects installed drivers itself.
5. Run `/caps` and `/status`, send a model prompt, and ask the agent to create
   and read a small file. Native tools use the configured boot-volume directory.
   `/exit` returns to Shell when started from Shell, or to firmware when booted directly.

Both trees include `EFI/TOOLS/SHELLX64.EFI` and its license. `/caps` reports
whether the Shell protocol is available. Model-driven Shell command tools are
not yet implemented. Shell cannot supply missing NIC drivers, TCP4, or RNG.
Agent can load explicitly listed boot-volume drivers at startup, then connect
installed drivers until discovery is stable. See [firmware drivers](docs/firmware-drivers.md)
for `DRIVERS.JSON`, supported images, and the network protocol counts.

OVMF testing verifies native text input, direct HTTPS, and FAT file tools.
Physical motherboard, NIC, and USB media support remain unverified.

## Verification

See [the verification record](docs/verification.md) for tested behavior and
limits. Direct-provider smoke tests use a local HTTPS provider with a temporary
CA, require no external credentials, and run with `uv`:

```bash
uv run scripts/smoke_vm.py --launcher target/debug/efi-agent \
  --code artifacts/firmware/OVMF_CODE_4M.fd --vars artifacts/firmware/OVMF_VARS_4M.fd \
  --esp artifacts/esp --output artifacts/direct-smoke
```

Use `--api-format responses` to test Responses. Use `--tls-failure untrusted`,
`hostname`, or `expired` to test certificate rejection. Linux-native tests also
use `scripts/smoke_native.py` for firmware text input and
`scripts/smoke_launcher.py` for a tmux terminal and host workspace persistence.
Image inspection requires `mtools`; `scripts/smoke_pack.py` additionally requires
`sgdisk` and `fsck.fat`. No external model provider has been verified.
