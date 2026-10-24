#include "efi_process.h"
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static void require(int condition, const char *message)
{
    if (!condition) {
        fprintf(stderr, "POSIX_NETWORK_FAILURE: %s (errno=%d)\n", message, errno);
        exit(1);
    }
}

static void check_resolver(void)
{
    struct addrinfo hints = {.ai_family = AF_INET, .ai_socktype = SOCK_STREAM,
                            .ai_flags = AI_NUMERICHOST | AI_CANONNAME};
    struct addrinfo *result = NULL;
    require(getaddrinfo("203.0.113.7", "49153", &hints, &result) == 0, "numeric resolver");
    struct sockaddr_in *address = (struct sockaddr_in *)result->ai_addr;
    require(result->ai_next == NULL && result->ai_protocol == IPPROTO_TCP &&
            result->ai_addrlen == sizeof *address, "resolver metadata");
    require(memcmp(&address->sin_addr, (uint8_t[]){203, 0, 113, 7}, 4) == 0, "numeric address bytes");
    require(memcmp(&address->sin_port, (uint8_t[]){0xc0, 0x01}, 2) == 0, "network port bytes");
    require(strcmp(result->ai_canonname, "203.0.113.7") == 0, "canonical numeric name");
    freeaddrinfo(result);
    for (unsigned port = 0; port <= 65535; port += 65535) {
        char service[16];
        snprintf(service, sizeof service, "%u", port);
        require(getaddrinfo(NULL, service, NULL, &result) == 0, "port boundary");
        address = (struct sockaddr_in *)result->ai_addr;
        require(ntohs(address->sin_port) == port &&
                memcmp(&address->sin_addr, (uint8_t[]){127, 0, 0, 1}, 4) == 0, "loopback and port");
        freeaddrinfo(result);
    }
    for (const char **service = (const char *[]) {"65536", "-1", "+53", "53x", "", "http", NULL}; *service; ++service) {
        result = (void *)1;
        require(getaddrinfo("127.0.0.1", *service, &hints, &result) == EAI_SERVICE && result == NULL, "invalid numeric service");
    }
    require(getaddrinfo("mixed.example", "53", &hints, &result) == EAI_NONAME, "numeric host forbids DNS");
    hints.ai_flags = AI_CANONNAME;
    require(getaddrinfo("mixed.example", "53", &hints, &result) == EAI_BADFLAGS && result == NULL, "hostname canonical target unavailable");
    hints.ai_flags = 0x8000;
    require(getaddrinfo("127.0.0.1", "53", &hints, &result) == EAI_BADFLAGS && result == NULL, "invalid resolver flags");
    hints.ai_flags = AI_PASSIVE;
    require(getaddrinfo(NULL, "53", &hints, &result) == 0, "passive address");
    address = (struct sockaddr_in *)result->ai_addr;
    require(address->sin_addr.s_addr == 0, "passive any address");
    freeaddrinfo(result);
    hints.ai_family = AF_INET6;
    require(getaddrinfo("127.0.0.1", "53", &hints, &result) == EAI_FAMILY && result == NULL, "unsupported family");
    require(socket(AF_INET6, SOCK_STREAM, 0) == -1 && errno == EAFNOSUPPORT, "socket family errno");
    require(socket(AF_INET, SOCK_DGRAM, 0) == -1 && errno == EPROTONOSUPPORT, "socket type errno");
    int cloexec = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    require(cloexec >= 0 && close(cloexec) == 0, "close on exec socket creation");
    freeaddrinfo(NULL);
}

int main(int argc, char **argv)
{
    check_resolver();
    if (argc == 2 && strcmp(argv[1], "check") == 0) {
        puts("POSIX_CHECK_OK");
        return 0;
    }
    if (argc == 4 && strcmp(argv[1], "dns") == 0) {
        struct addrinfo *result = NULL;
        struct addrinfo hints = {.ai_family = AF_INET, .ai_socktype = SOCK_STREAM};
        int error = getaddrinfo(argv[2], "49153", &hints, &result);
        require(error == atoi(argv[3]), "DNS getaddrinfo result");
        if (error == 0) {
            struct sockaddr_in *address = (struct sockaddr_in *)result->ai_addr;
            require(memcmp(&address->sin_addr, (uint8_t[]){203, 0, 113, 7}, 4) == 0 && ntohs(address->sin_port) == 49153, "DNS resolved address/port");
        } else {
            require(result == NULL, "DNS failure result null");
        }
        freeaddrinfo(result);
        puts("POSIX_DNS_OK");
        return 0;
    }
    require(argc == 3 || argc == 4, "arguments");
    struct addrinfo *result = NULL;
    struct addrinfo hints = {.ai_family = AF_INET, .ai_socktype = SOCK_STREAM,
                            .ai_flags = AI_NUMERICHOST | AI_NUMERICSERV};
    require(getaddrinfo(argv[1], argv[2], &hints, &result) == 0, "connect address");
    int descriptor = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, IPPROTO_TCP);
    require(descriptor >= 0, "socket");
    int keepalive = 1;
    require(setsockopt(descriptor, SOL_SOCKET, SO_KEEPALIVE, &keepalive, sizeof keepalive) == -1 && errno == ENOPROTOOPT, "unsupported option error");
    require(bind(descriptor, result->ai_addr, result->ai_addrlen) == -1 && errno == EOPNOTSUPP, "unsupported bind error");
    uint8_t guard = 137;
    require(recv(descriptor, NULL, 1, 0) == -1 && errno == EFAULT, "recv null buffer");
    require(send(descriptor, NULL, 1, 0) == -1 && errno == EFAULT, "send null buffer");
    require(recv(descriptor, &guard, 1, MSG_PEEK) == -1 && errno == EOPNOTSUPP && guard == 137, "unsupported peek preserves buffer");
    int mode = fcntl(descriptor, F_GETFL);
    require(mode >= 0 && (mode & O_NONBLOCK), "fcntl nonblocking");
    require(connect(descriptor, result->ai_addr, sizeof(sa_family_t)) == -1 && errno == EINVAL, "short sockaddr");
    int expected = argc == 4 && strcmp(argv[3], "refused") == 0 ? ECONNREFUSED : 0;
    if (argc == 4 && strcmp(argv[3], "timeout") == 0) {
        expected = ETIMEDOUT;
    }
    if (argc == 4 && strcmp(argv[3], "reset") == 0) {
        expected = ECONNRESET;
    }
    int connected = connect(descriptor, result->ai_addr, result->ai_addrlen);
    freeaddrinfo(result);
    if (connected == -1 && errno == EINPROGRESS) {
        struct pollfd ready = {.fd = descriptor, .events = POLLOUT};
        require(poll(&ready, 1, -1) == 1, "connect poll");
        require(ready.revents & (expected ? POLLERR : POLLOUT), "connect readiness");
    } else {
        require(expected ? connected == -1 && errno == expected : connected == 0, "immediate connection");
    }
    int pending = 7913;
    socklen_t size = sizeof pending - 1;
    require(getsockopt(descriptor, SOL_SOCKET, SO_ERROR, &pending, &size) == -1 && errno == EINVAL && pending == 7913, "short SO_ERROR preserves result");
    size = sizeof pending;
    require(getsockopt(descriptor, SOL_SOCKET, SO_ERROR, &pending, &size) == 0 && size == sizeof pending, "SO_ERROR call");
    if (pending != expected) {
        fprintf(stderr, "POSIX_SOCKET_ERROR: actual=%d expected=%d\n", pending, expected);
    }
    require(pending == expected, "SO_ERROR value");
    require(getsockopt(descriptor, SOL_SOCKET, SO_ERROR, &pending, &size) == 0 && pending == 0, "SO_ERROR clear");
    if (expected) {
        require(close(descriptor) == 0, "close refused");
        puts(expected == ETIMEDOUT ? "POSIX_TIMEOUT_OK" :
             expected == ECONNRESET ? "POSIX_RESET_OK" : "POSIX_REFUSED_OK");
        return 0;
    }
    require(getsockopt(descriptor, SOL_SOCKET, SO_TYPE, &pending, &size) == 0 && pending == SOCK_STREAM, "SO_TYPE");
    uint8_t byte = 137;
    require(recv(descriptor, &byte, 1, 0) == -1 && errno == EAGAIN && byte == 137, "nonblocking recv");
    require(efi_process_dup2(descriptor, 42) == 42, "duplicate");
    require(fcntl(42, F_SETFL, mode & ~O_NONBLOCK) == 0, "restore blocking");
    require(!(fcntl(descriptor, F_GETFL) & O_NONBLOCK), "shared fcntl flags");
    byte = 0xa7;
    require(send(descriptor, &byte, 1, MSG_NOSIGNAL) == 1, "send handshake");
    for (size_t index = 0; index < 3; ++index) {
        require(recv(42, &byte, 1, 0) == 1 && byte == ((uint8_t[]){19, 73, 211})[index], "recv exact bytes");
    }
    require(recv(descriptor, &byte, 1, 0) == 0, "recv EOF");
    require(close(descriptor) == 0 && close(42) == 0, "close sockets");
    require(send(1, &byte, 1, 0) == -1 && errno == ENOTSOCK, "send rejects console");
    puts("POSIX_NETWORK_OK");
    return 0;
}
