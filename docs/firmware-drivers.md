# Firmware driver activation

Agent prepares drivers at startup, before it opens long-lived network or
console protocols. This works with direct boot and Shell boot. It does not
require a Shell command tool or a model request.

1. Read the optional `\EFI\AGENT\DRIVERS.JSON` on Agent's boot volume.
2. Validate each listed x64 PE32+ EFI boot-service or runtime driver, then use
   `LoadImage()` and `StartImage()` in list order. Pass its full boot-volume
   device path to firmware as well as its bytes. Firmware applies its signing
   policy. Release filesystem and path protocol references before StartImage.
3. Leave successful drivers resident. A loaded image with the same full device
   path is not loaded again when Agent is restarted from Shell.
4. Call `ConnectController(handle, NULL, NULL, TRUE)` for each firmware handle.
   Repeat when the set of handles or Driver Binding instances changes. Stop
   when both sets remain unchanged after a pass.
5. Report the loading result, connection passes/errors, and protocol counts
   through `/caps`. An optional-driver error stops the rest of its list, but
   existing firmware drivers still get connected and the interface remains
   available. Drivers already started are not rolled back.

## Supply additional drivers

Copy the required driver binaries under `EFI/AGENT/DRIVERS` in the native ESP
tree. Create `EFI/AGENT/DRIVERS.JSON` as an ordered JSON array, for example:

```json
[
  "\\EFI\\AGENT\\DRIVERS\\NicDriver.efi",
  "\\EFI\\AGENT\\DRIVERS\\MnpDxe.efi",
  "\\EFI\\AGENT\\DRIVERS\\ArpDxe.efi",
  "\\EFI\\AGENT\\DRIVERS\\Ip4Dxe.efi",
  "\\EFI\\AGENT\\DRIVERS\\Dhcp4Dxe.efi",
  "\\EFI\\AGENT\\DRIVERS\\TcpDxe.efi"
]
```

These names illustrate a driver set; the package does not include these
binaries. Supply a coherent set for the actual NIC and firmware. Additional
dependencies can be required. A public EDK II network driver does not replace
the hardware-specific NIC driver. TLS uses CPU RDRAND and ChaCha20, with no
firmware RNG driver requirement. The loader is not a dependency resolver.

The manifest is limited to 64 KiB and 32 distinct paths. Each image is limited
to 8 MiB; their total is limited to 32 MiB. Paths must be absolute, stay under
`\EFI\AGENT\DRIVERS`, and use `.efi`. Application images, other architectures,
relative or cross-volume paths, traversal, and duplicate case aliases are
rejected. The firmware loader checks the rest of the image format.

Packaging retains additional files in the generated trees. Add these files
before a later package operation to include them in its image, or copy them
directly onto the FAT boot media. Direct VM launch from a single Agent `.efi`
contains no extra drivers; use an ESP tree/image when extra drivers are needed.
The model's workspace file tools do not select boot drivers. Supplied drivers
run with full firmware privileges; only use drivers you intend to run.

## Interpret the network counts

| `/caps` result | What it establishes |
|---|---|
| SNP present | A firmware packet-level network interface exists |
| MNP service binding present | A managed network layer exists |
| IP4 service binding present | An IPv4 layer exists |
| TCP4 service binding present | A TCP4 layer exists for Agent's transport |
| IPv4 Config2 present | Firmware exposes IPv4 configuration, including DHCP policy |
| RNG request succeeds | The RDRAND-seeded ChaCha20 generator can supply bytes |

Counts do not establish physical link, address assignment, routing, DNS access,
or connectivity to the provider. Agent already preserves an existing address
or requests DHCP through IPv4 Config2 before creating a TCP child. It does not
manually reset or initialize SNP while upper network drivers may own it.

If the firmware exposes IP4/TCP4 but not IPv4 Config2, configure a static
address in `CONFIG.JSON`:

```json
"ipv4": {
  "address": [192, 168, 1, 20],
  "subnet_mask": [255, 255, 255, 0],
  "gateway": [192, 168, 1, 1]
}
```

The launcher writes this object when all of
`EFI_AGENT_IPV4_ADDRESS`, `EFI_AGENT_IPV4_NETMASK`, and
`EFI_AGENT_IPV4_GATEWAY` are set. Agent applies the station address and mask
to each TCP4 child and adds its default route. Without the object, the existing
firmware address or IPv4 Config2 DHCP path is used.

## UEFI standard findings

The following UEFI 2.11 sections define the services used here:

- [7.3.12 ConnectController](https://uefi.org/specs/UEFI/2.11/07_Services_Boot_Services.html#efi-boot-services-connectcontroller)
  recursively connects child controllers through Driver Binding. Firmware
  handles platform override, driver-family, bus override, and general binding
  selection. An empty driver list leaves that standard selection in control.
- [7.4 Image services](https://uefi.org/specs/UEFI/2.11/07_Services_Boot_Services.html#image-services)
  define LoadImage and StartImage. A successful driver entry point leaves the
  image resident; an application's successful return has different semantics.
- [11.3 Platform Driver Override](https://uefi.org/specs/UEFI/2.11/11_Protocols_UEFI_Driver_Model.html#efi-platform-driver-override-protocol-protocols-uefi-driver-model)
  distinguishes image handles returned by GetDriver from paths returned by
  GetDriverPath. ConnectController consumes handles. Loading paths requires a
  separate component. Agent's own driver selection uses its explicit manifest.
- [3 Boot Manager](https://uefi.org/specs/UEFI/2.11/03_Boot_Manager.html)
  defines DriverOrder and Driver#### as boot-manager driver load options. Agent
  does not rewrite these persistent variables or replay the boot manager.
- [24.1 Simple Network](https://uefi.org/specs/UEFI/2.11/24_Network_Protocols_SNP_PXE_BIS.html)
  defines Start and Initialize for an existing packet-level interface. Those
  operations do not supply a missing NIC driver or TCP/IP implementation.
- [35 HII configuration](https://uefi.org/specs/UEFI/2.11/35_HII_Configuration_Processing_and_Browser_Protocol.html)
  provides driver-specific configuration routing. It does not define a
  portable BIOS variable or command that enables every platform's network stack.

There is no universal UEFI application service to enable an absent optional
component. Firmware build choices, platform setup policy, NIC support, and
driver availability still apply. DXE dependency dispatch is a PI firmware
mechanism, not a portable UEFI boot service for applications. Agent does not
patch private setup variables, invoke private DXE services, disable image
verification, or add deterministic TLS randomness.
