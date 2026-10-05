# EFI Agent

A Rust UEFI coding agent with a Ratatui interface. The firmware application uses
`no_std` and `alloc`. It does not load an operating system and it depends on UEFI
services (and UEFI drivers) for networking, file systems, and more.

See [UEFI safety](docs/uefi-safety.md) for UEFI safety rules.

## Building and running on Windows

Use PowerShell 7.4 or later with Rust 1.93 or later:

```powershell
rustup.exe target add x86_64-unknown-uefi
./scripts/build.ps1 -Release
cargo.exe test
```

The build creates `artifacts/esp/EFI/BOOT/BOOTX64.EFI` and
`artifacts/efi-agent-vm.img`, a GPT disk with a 64 MiB FAT32 EFI System Partition.
The `VM.TXT` marker
selects serial input and output. For bare metal, copy `BOOTX64.EFI` to a FAT ESP
and omit this marker. Bare metal uses firmware text input and output.

For native agent use, create `EFI/AGENT/NATIVE.JSON` on the boot volume:

```json
{
  "relay_address": [192, 168, 1, 73],
  "relay_port": 7420,
  "workspace": "\\work"
}
```

The workspace directory must exist on the boot volume. Model file tools use
EFI_SIMPLE_FILE_SYSTEM_PROTOCOL inside that directory. Tool paths are relative
and absolute paths are rejected. 

Requests can use EFI TCP4 and DHCP to reach a configured model relay. On a trusted
LAN, run `efi-agent serve <workspace> 0.0.0.0:7420` on the relay machine with the API
environment variables below. In native mode the relay only receives model requests,
and file tools execute in the firmware application.

The current relay transport is unencrypted and unauthenticated. Use it only on
a trusted LAN.

## VM launch

Install QEMU and supply an OVMF image that includes VirtioSerialDxe and
VirtioNetDxe. WHPX must be enabled on Windows; KVM must be available on Linux.
You may need to update QEMU to its latest version.

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

### Start the VM

The launcher uses the current terminal, with no graphical QEMU window.

```powershell
$qemu = (Get-Command qemu-system-x86_64.exe -CommandType Application -ErrorAction Stop).Source
./target/debug/efi-agent.exe vm $qemu ./artifacts/firmware/OVMF_CODE_4M.fd ./artifacts/firmware/OVMF_VARS_4M.fd ./artifacts/esp ./workspace
```

If QEMU is not on PATH, set `$qemu` to your executable's actual path instead.

VM memory defaults to 128 MiB. Append `--memory-mib 256` to the launch command
to allocate 256 MiB, or supply another positive integer in MiB.

The launcher accepts an ESP directory (read-only QEMU vvfat) or a raw boot disk
image (read-only virtio-blk). HostBridge supplies writable host files separately.
Supply matching OVMF code and variable-store images. QEMU uses a temporary snapshot
of the variable store, so booting does not modify the supplied template. Read-only
vvfat is attached through virtio-blk.

## Boot image packaging

Both build scripts create `artifacts/efi-agent-vm.img`. Pass this path instead
of the ESP directory to `efi-agent vm`. The image has a protective MBR, primary
and backup GPT tables, and a FAT32 EFI System Partition starting at sector 2048.
Its total size is 66 MiB. Each build replaces only this generated artifact after
the new image is complete.

To package a separate boot tree:

```powershell
./target/debug/efi-agent.exe pack ./native-esp ./efi-agent-native.img
```

The source must contain `EFI/BOOT/BOOTX64.EFI`. For native mode, omit
`EFI/AGENT/VM.TXT`, include `EFI/AGENT/NATIVE.JSON`, and create the configured
workspace directory in that tree before packaging. Packaging copies all regular
files, including workspace contents.

`pack` creates a new regular file and refuses to overwrite an existing path.
Keep its output outside the source tree. It does not flash hardware or write a
physical disk. The native disk image has been tested with writable virtio-blk
under OVMF; physical USB media and firmware remain unverified. Firmware must
permit this unsigned application to run.

To check the package with independent Linux tools:

```sh
uv run scripts/smoke_pack.py --launcher target/debug/efi-agent \
  --efi artifacts/esp/EFI/BOOT/BOOTX64.EFI --output artifacts/pack-smoke
```

This needs sgdisk, fsck.fat, and mtools. Native protocol testing now also uses
the pack command:

```sh
uv run scripts/smoke_native.py --launcher target/debug/efi-agent \
  --efi artifacts/esp/EFI/BOOT/BOOTX64.EFI \
  --code artifacts/firmware/OVMF_CODE_4M.fd \
  --vars artifacts/firmware/OVMF_VARS_4M.fd --output artifacts/native-smoke
```

## API setup

API credentials stay on the host:

```powershell
$env:EFI_AGENT_API_BASE = 'https://provider.example/v1'
$env:EFI_AGENT_API_KEY = 'your-key'
$env:EFI_AGENT_MODEL = 'your-model'
./target/debug/efi-agent.exe serve ./workspace
```

Set the base URL to the API root, for example `https://provider.example/v1`.
The host appends `/chat/completions` by default. Set the key and model on the
launcher or relay host; credentials do not enter the UEFI application.

`EFI_AGENT_API_FORMAT` selects the API format. It defaults to `chat_completions`.
Set it to `responses` to use `/responses` and request reasoning summaries with
`reasoning.summary: "auto"`. For example, on the Windows host:

```powershell
$env:EFI_AGENT_API_FORMAT = 'responses'
```

Currently, it supports:
- Chat Completions API
- Responses API


