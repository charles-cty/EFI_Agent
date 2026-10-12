#ifndef EFI_NET_H
#define EFI_NET_H

#include <stdint.h>

#define EFI_NET_CLOEXEC 0x10000u

/* Allocate an unconnected IPv4 stream descriptor. Flags combine WASI NONBLOCK
 * and EFI_NET_CLOEXEC. Close-on-exec is local to each descriptor; duplication
 * and explicit spawn_fds mappings clear it. Default spawn inheritance honors it.
 * Output remains unchanged on failure. */
__attribute__((import_module("efi_net"), import_name("tcp_open")))
int32_t efi_net_tcp_open(uint32_t flags, uint32_t *descriptor);

/* Connect an existing socket, preserving its descriptor on failure.
 * NONBLOCK returns INPROGRESS (26) while pending; poll write readiness then
 * read tcp_error. A repeated pending connect returns ALREADY (7), and a
 * connected socket returns ISCONN (30). Failed sockets can start a new attempt. */
__attribute__((import_module("efi_net"), import_name("tcp_connect_fd")))
int32_t efi_net_tcp_connect_fd(uint32_t descriptor, const uint8_t address[4], uint32_t port);

/* Publish and clear the shared pending connection error, like SO_ERROR.
 * A zero error while still connecting does not imply connection completion.
 * Invalid output ranges neither progress nor clear the error. */
__attribute__((import_module("efi_net"), import_name("tcp_error")))
int32_t efi_net_tcp_error(uint32_t descriptor, uint32_t *error);

/* Address contains four IPv4 octets. Port is host order, 1..65535.
 * Success publishes one connected descriptor; failure leaves it unchanged.
 * Pending connections suspend the guest. Use WASI fd_read/fd_write,
 * poll_oneoff and fd_close on the connected descriptor. WASI NONBLOCK can be
 * changed with fd_fdstat_set_flags; duplicates share the flag. Pending data
 * operations return AGAIN in that mode, while poll still waits for readiness. */
__attribute__((import_module("efi_net"), import_name("tcp_connect")))
int32_t efi_net_tcp_connect(const uint8_t address[4], uint32_t port, uint32_t *descriptor);

/* Wait for accepted sends and close the shared connection normally.
 * Consumes this descriptor on completion or transport error. Duplicates refer
 * to the closed connection. fd_close alone remains an abortive last close. */
__attribute__((import_module("efi_net"), import_name("tcp_close")))
int32_t efi_net_tcp_close(uint32_t descriptor);

/* Resolve one ASCII hostname to IPv4 with a bounded DNS A query over TCP.
 * Host bytes omit NUL. Failure preserves the four-octet result. */
__attribute__((import_module("efi_net"), import_name("resolve_ipv4")))
int32_t efi_net_resolve_ipv4(const char *name, uint32_t size, uint8_t address[4]);

#endif
