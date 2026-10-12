#include "efi_net.h"
#include "efi_process.h"
#include "efi_wasi.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wasi/api.h>

#define REQUEST_SIZE 196613
#define RESPONSE_SIZE 131089

static void require(int condition, const char *message)
{
    if (!condition) {
        fprintf(stderr, "NETWORK_FAILURE: %s\n", message);
        exit(1);
    }
}

int main(int argc, char **argv)
{
    if (argc == 4 && strcmp(argv[1], "dns") == 0) {
        uint8_t address[4] = {17, 31, 73, 101};
        int32_t result = efi_net_resolve_ipv4(argv[2], (uint32_t)strlen(argv[2]), address);
        int32_t expected = (int32_t)strtol(argv[3], NULL, 10);
        require(result == expected, "DNS result");
        if (expected == 0) {
            require(memcmp(address, (uint8_t[]){203, 0, 113, 7}, 4) == 0, "DNS address");
        } else {
            require(memcmp(address, (uint8_t[]){17, 31, 73, 101}, 4) == 0, "DNS failure preserves address");
        }
        puts("NETWORK_DNS_OK");
        return 0;
    }
    if (argc == 2 && strcmp(argv[1], "peer") == 0) {
        FILE *file = fopen("network-peer.txt", "wb");
        require(file != NULL, "peer file open");
        require(fwrite("PEER_PROGRESS", 1, 13, file) == 13, "peer file write");
        require(fclose(file) == 0, "peer file close");
        puts("NETWORK_PEER_PROGRESS");
        return 0;
    }
    require(argc == 3 || argc == 4, "arguments");
    unsigned octets[4];
    require(sscanf(argv[1], "%u.%u.%u.%u", &octets[0], &octets[1], &octets[2], &octets[3]) == 4, "address");
    uint8_t address[4];
    for (unsigned index = 0; index < 4; ++index) {
        require(octets[index] <= 255, "octet");
        address[index] = (uint8_t)octets[index];
    }
    uint32_t port = (uint32_t)strtoul(argv[2], NULL, 10);
    uint32_t descriptor = 0x12345678;
    int asynchronous = argc == 4 && (strcmp(argv[3], "async") == 0 || strcmp(argv[3], "async-refused") == 0);
    int32_t error;
    if (asynchronous) {
        int expected = strcmp(argv[3], "async-refused") == 0 ? __WASI_ERRNO_CONNREFUSED : 0;
        require(efi_net_tcp_open(__WASI_FDFLAGS_NONBLOCK, &descriptor) == 0, "open async socket");
        error = efi_net_tcp_connect_fd(descriptor, address, port);
        if (error == __WASI_ERRNO_INPROGRESS) {
            __wasi_subscription_t subscription = {0};
            subscription.userdata = 0x7193;
            subscription.u.tag = __WASI_EVENTTYPE_FD_WRITE;
            subscription.u.u.fd_write.file_descriptor = descriptor;
            __wasi_event_t event;
            size_t events = 0;
            require(__wasi_poll_oneoff(&subscription, &event, 1, &events) == 0 && events == 1 && event.userdata == 0x7193, "async connect poll");
            require(event.error == expected, "async connect event error");
        } else {
            require(error == expected, "immediate async connect result");
        }
        uint32_t pending = 0x12345678;
        require(efi_net_tcp_error(descriptor, &pending) == 0 && pending == (uint32_t)expected, "async socket error");
        require(efi_net_tcp_error(descriptor, &pending) == 0 && pending == 0, "async socket error cleared");
        if (expected) {
            require(__wasi_fd_close(descriptor) == 0, "close failed async socket");
            puts("NETWORK_ASYNC_REFUSED_OK");
            return 0;
        }
        require(efi_net_tcp_connect_fd(descriptor, address, port) == __WASI_ERRNO_ISCONN, "async already connected");
        error = 0;
    } else {
        error = efi_net_tcp_connect(address, port, &descriptor);
    }
    if (argc == 4 && strcmp(argv[3], "refused") == 0) {
        require(error == 14, "connection refused errno");
        require(descriptor == 0x12345678, "failure output unchanged");
        puts("NETWORK_REFUSED_OK");
        return 0;
    }
    require(error == 0, "connect");
    if (argc == 4 && (strcmp(argv[3], "nonblocking") == 0 || asynchronous)) {
        require(efi_process_dup2(descriptor, 42) == 42, "nonblocking duplicate");
        require(__wasi_fd_fdstat_set_flags(descriptor, __WASI_FDFLAGS_NONBLOCK) == 0, "set nonblocking");
        __wasi_fdstat_t stat;
        require(__wasi_fd_fdstat_get(42, &stat) == 0 && stat.fs_flags == __WASI_FDFLAGS_NONBLOCK, "shared flags");
        require(__wasi_fd_fdstat_set_flags(42, __WASI_FDFLAGS_NONBLOCK | __WASI_FDFLAGS_APPEND) == __WASI_ERRNO_NOTSUP, "reject append");
        uint8_t byte = 137;
        __wasi_iovec_t input = {&byte, 1};
        size_t count = 97831;
        require(__wasi_fd_read(42, &input, 1, &count) == __WASI_ERRNO_AGAIN, "pending read again");
        require(byte == 137 && count == 97831, "again preserves buffer/count");
        input.buf_len = 0;
        require(__wasi_fd_read(42, &input, 1, &count) == 0 && count == 0, "empty nonblocking read");
        require(__wasi_fd_fdstat_set_flags(42, 0) == 0, "restore blocking");
        require(__wasi_fd_fdstat_get(descriptor, &stat) == 0 && stat.fs_flags == 0, "shared blocking flags");
        // The server sends no bytes until this independent handshake arrives.
        byte = 0xa7;
        __wasi_ciovec_t output = {&byte, 1};
        require(__wasi_fd_write(descriptor, &output, 1, &count) == 0 && count == 1, "nonblocking handshake");
        const uint8_t expected[] = {19, 73, 211};
        input.buf_len = 1;
        for (size_t index = 0; index < sizeof expected; ++index) {
            require(__wasi_fd_read(42, &input, 1, &count) == 0 && count == 1 && byte == expected[index], "blocking read after reset");
        }
        require(__wasi_fd_fdstat_set_flags(descriptor, __WASI_FDFLAGS_NONBLOCK) == 0, "set nonblocking at EOF");
        __wasi_subscription_t subscription = {0};
        subscription.u.tag = __WASI_EVENTTYPE_FD_READ;
        subscription.u.u.fd_read.file_descriptor = 42;
        __wasi_event_t event;
        size_t events = 0;
        require(__wasi_poll_oneoff(&subscription, &event, 1, &events) == 0 && events == 1 && event.error == 0, "poll EOF");
        require(__wasi_fd_read(42, &input, 1, &count) == 0 && count == 0, "nonblocking EOF differs from again");
        require(__wasi_fd_close(descriptor) == 0 && __wasi_fd_close(42) == 0, "nonblocking close");
        puts(asynchronous ? "NETWORK_ASYNC_OK" : "NETWORK_NONBLOCKING_OK");
        return 0;
    }
    if (argc == 4 && strcmp(argv[3], "blocked") == 0) {
        puts("NETWORK_BLOCKED_READ");
        uint8_t byte;
        __wasi_iovec_t vector = {&byte, 1};
        size_t count = 0;
        require(__wasi_fd_read(descriptor, &vector, 1, &count) == 0, "blocked read");
        require(0, "blocked read returned without cancellation");
    }
    require(efi_process_dup2(descriptor, 42) == 42, "duplicate socket");
    require(__wasi_fd_close(descriptor) == 0, "close original socket");
    descriptor = 42;
    __wasi_fdstat_t stat;
    require(__wasi_fd_fdstat_get(descriptor, &stat) == 0 && stat.fs_filetype == __WASI_FILETYPE_SOCKET_STREAM, "socket type");
    uint8_t *request = malloc(REQUEST_SIZE);
    require(request != NULL, "request allocation");
    for (uint32_t index = 0; index < REQUEST_SIZE; ++index) {
        request[index] = (uint8_t)(index * 37 + index / 251 + 19);
    }
    size_t sent = 0;
    while (sent < REQUEST_SIZE) {
        __wasi_ciovec_t vector = {request + sent, REQUEST_SIZE - sent};
        size_t count = 0;
        require(__wasi_fd_write(descriptor, &vector, 1, &count) == 0, "write");
        require(count > 0 && count <= REQUEST_SIZE - sent, "write count");
        sent += count;
    }
    free(request);
    if (argc == 4 && strcmp(argv[3], "graceful") == 0) {
        require(efi_process_dup2(descriptor, 43) == 43, "duplicate before graceful close");
        require(efi_net_tcp_close(descriptor) == 0, "graceful close");
        require(__wasi_fd_fdstat_get(descriptor, &stat) == __WASI_ERRNO_BADF, "graceful close consumes descriptor");
        uint8_t byte;
        __wasi_iovec_t vector = {&byte, 1};
        size_t count = 765;
        require(__wasi_fd_read(43, &vector, 1, &count) == __WASI_ERRNO_NOTCONN, "closed shared socket");
        require(count == 765, "closed read preserves output");
        require(__wasi_fd_close(43) == 0, "close remaining duplicate");
        puts("NETWORK_GRACEFUL_CLOSE_OK");
        return 0;
    }
    const char *arguments[] = {"network", "peer"};
    char scratch[64];
    int32_t child = efi_process_spawn_argv(arguments, 2, scratch, sizeof scratch);
    require(child > 0, "spawn peer");
    __wasi_subscription_t subscription = {0};
    subscription.userdata = 0x7265616479;
    subscription.u.tag = __WASI_EVENTTYPE_FD_READ;
    subscription.u.u.fd_read.file_descriptor = descriptor;
    __wasi_event_t event;
    size_t events = 0;
    require(__wasi_poll_oneoff(&subscription, &event, 1, &events) == 0, "poll");
    require(events == 1 && event.userdata == subscription.userdata && event.error == 0, "readiness event");
    size_t received = 0;
    uint8_t bytes[997];
    FILE *file = fopen("network-response.bin", "wb");
    require(file != NULL, "response file");
    for (;;) {
        __wasi_iovec_t vector = {bytes, sizeof bytes};
        size_t count = 0;
        require(__wasi_fd_read(descriptor, &vector, 1, &count) == 0, "read");
        if (count == 0) {
            break;
        }
        require(received + count <= RESPONSE_SIZE, "response length");
        for (size_t index = 0; index < count; ++index) {
            size_t position = received + index;
            require(bytes[index] == (uint8_t)(position * 53 + position / 239 + 7), "response byte");
        }
        require(fwrite(bytes, 1, count, file) == count, "save response");
        received += count;
    }
    require(received == RESPONSE_SIZE, "complete response");
    require(fclose(file) == 0, "close response file");
    require(__wasi_fd_close(descriptor) == 0, "close duplicate socket");
    int32_t child_status = -1;
    require(efi_process_waitpid((uint32_t)child, &child_status) == child && child_status == 0, "wait peer");
    puts("NETWORK_EXCHANGE_OK");
    return 0;
}
