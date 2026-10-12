#include "efi_net.h"
#include <arpa/inet.h>
#include <errno.h>
#include <limits.h>
#include <netdb.h>
#include <netinet/in.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <wasi/api.h>

static int status(int error)
{
    if (error == 0) {
        return 0;
    }
    errno = error;
    return -1;
}

static int socket_stat(int descriptor, __wasi_fdstat_t *stat)
{
    int error = __wasi_fd_fdstat_get(descriptor, stat);
    if (error != 0) {
        return status(error);
    }
    return status(stat->fs_filetype == __WASI_FILETYPE_SOCKET_STREAM ? 0 : ENOTSOCK);
}

int socket(int domain, int type, int protocol)
{
    if (domain != AF_INET) {
        return status(EAFNOSUPPORT);
    }
    if ((type & ~(SOCK_NONBLOCK | SOCK_CLOEXEC)) != SOCK_STREAM ||
        (protocol != 0 && protocol != IPPROTO_TCP)) {
        return status(EPROTONOSUPPORT);
    }
    uint32_t descriptor;
    uint32_t flags = type & SOCK_NONBLOCK ? __WASI_FDFLAGS_NONBLOCK : 0;
    if (type & SOCK_CLOEXEC) {
        flags |= EFI_NET_CLOEXEC;
    }
    if (status(efi_net_tcp_open(flags, &descriptor)) != 0) {
        return -1;
    }
    return (int)descriptor;
}

int connect(int descriptor, const struct sockaddr *address, socklen_t length)
{
    if (address == NULL) {
        return status(EFAULT);
    }
    if (length < sizeof(struct sockaddr_in)) {
        return status(EINVAL);
    }
    struct sockaddr_in ipv4;
    memcpy(&ipv4, address, sizeof ipv4);
    if (ipv4.sin_family != AF_INET) {
        return status(EAFNOSUPPORT);
    }
    return status(efi_net_tcp_connect_fd(descriptor, (const uint8_t *)&ipv4.sin_addr,
                                        ntohs(ipv4.sin_port)));
}

int getsockopt(int descriptor, int level, int option, void *value, socklen_t *length)
{
    if (value == NULL || length == NULL) {
        return status(EFAULT);
    }
    if (*length < sizeof(int)) {
        return status(EINVAL);
    }
    __wasi_fdstat_t stat;
    if (socket_stat(descriptor, &stat) != 0) {
        return -1;
    }
    if (level != SOL_SOCKET || (option != SO_ERROR && option != SO_TYPE)) {
        return status(ENOPROTOOPT);
    }
    uint32_t result = SOCK_STREAM;
    if (option == SO_ERROR && status(efi_net_tcp_error(descriptor, &result)) != 0) {
        return -1;
    }
    int published = (int)result;
    memcpy(value, &published, sizeof published);
    *length = sizeof published;
    return 0;
}

int bind(int descriptor, const struct sockaddr *address, socklen_t length)
{
    __wasi_fdstat_t stat;
    if (socket_stat(descriptor, &stat) != 0) {
        return -1;
    }
    if (address == NULL) {
        return status(EFAULT);
    }
    if (length < sizeof(struct sockaddr_in)) {
        return status(EINVAL);
    }
    return status(EOPNOTSUPP);
}

int setsockopt(int descriptor, int level, int option, const void *value, socklen_t length)
{
    (void)level;
    (void)option;
    __wasi_fdstat_t stat;
    if (socket_stat(descriptor, &stat) != 0) {
        return -1;
    }
    if (value == NULL && length != 0) {
        return status(EFAULT);
    }
    return status(ENOPROTOOPT);
}

int getsockname(int descriptor, struct sockaddr *address, socklen_t *length)
{
    __wasi_fdstat_t stat;
    if (socket_stat(descriptor, &stat) != 0) {
        return -1;
    }
    if (address == NULL || length == NULL) {
        return status(EFAULT);
    }
    return status(EOPNOTSUPP);
}

ssize_t send(int descriptor, const void *bytes, size_t length, int flags)
{
    if (bytes == NULL && length != 0) {
        return status(EFAULT);
    }
    if (flags & ~MSG_NOSIGNAL) {
        return status(EOPNOTSUPP);
    }
    if (length > SSIZE_MAX) {
        return status(EINVAL);
    }
    __wasi_fdstat_t stat;
    if (socket_stat(descriptor, &stat) != 0) {
        return -1;
    }
    __wasi_ciovec_t vector = {bytes, length};
    size_t count;
    if (status(__wasi_fd_write(descriptor, &vector, 1, &count)) != 0) {
        return -1;
    }
    return (ssize_t)count;
}

ssize_t recv(int descriptor, void *bytes, size_t length, int flags)
{
    if (bytes == NULL && length != 0) {
        return status(EFAULT);
    }
    if (flags != 0) {
        return status(EOPNOTSUPP);
    }
    if (length > SSIZE_MAX) {
        return status(EINVAL);
    }
    __wasi_fdstat_t stat;
    if (socket_stat(descriptor, &stat) != 0) {
        return -1;
    }
    __wasi_iovec_t vector = {bytes, length};
    size_t count;
    if (status(__wasi_fd_read(descriptor, &vector, 1, &count)) != 0) {
        return -1;
    }
    return (ssize_t)count;
}

int getaddrinfo(const char *node, const char *service, const struct addrinfo *hints,
                struct addrinfo **result)
{
    if (result == NULL) {
        errno = EFAULT;
        return EAI_SYSTEM;
    }
    *result = NULL;
    int flags = hints ? hints->ai_flags : 0;
    if (flags & ~(AI_PASSIVE | AI_CANONNAME | AI_NUMERICHOST | AI_NUMERICSERV)) {
        return EAI_BADFLAGS;
    }
    if (hints && hints->ai_family != AF_UNSPEC && hints->ai_family != AF_INET) {
        return EAI_FAMILY;
    }
    if (hints && ((hints->ai_socktype && hints->ai_socktype != SOCK_STREAM) ||
                  (hints->ai_protocol && hints->ai_protocol != IPPROTO_TCP))) {
        return EAI_SOCKTYPE;
    }
    if (node == NULL && service == NULL) {
        return EAI_NONAME;
    }
    unsigned port = 0;
    if (service) {
        if (*service == '\0') {
            return EAI_SERVICE;
        }
        for (const unsigned char *cursor = (const unsigned char *)service; *cursor; ++cursor) {
            if (*cursor < '0' || *cursor > '9') {
                return EAI_SERVICE;
            }
            port = port * 10 + (*cursor - '0');
            if (port > UINT16_MAX) {
                return EAI_SERVICE;
            }
        }
    }
    struct in_addr address;
    if (node == NULL) {
        address.s_addr = htonl(flags & AI_PASSIVE ? INADDR_ANY : INADDR_LOOPBACK);
    } else if (inet_pton(AF_INET, node, &address) != 1) {
        if (flags & AI_NUMERICHOST) {
            return EAI_NONAME;
        }
        if (flags & AI_CANONNAME) {
            return EAI_BADFLAGS; // The address-only DNS ABI does not publish CNAME targets.
        }
        int error = efi_net_resolve_ipv4(node, strlen(node), (uint8_t *)&address);
        if (error) {
            if (error == ENOENT) {
                return EAI_NONAME;
            }
            if (error == EINVAL) {
                return EAI_FAIL;
            }
            if (error == EAGAIN || error == ETIMEDOUT) {
                return EAI_AGAIN;
            }
            errno = error;
            return EAI_SYSTEM;
        }
    }
    struct addrinfo *entry = calloc(1, sizeof *entry);
    if (entry == NULL) {
        return EAI_MEMORY;
    }
    entry->ai_addr = calloc(1, sizeof(struct sockaddr_in));
    if (entry->ai_addr == NULL) {
        free(entry);
        return EAI_MEMORY;
    }
    if ((flags & AI_CANONNAME) && node) {
        entry->ai_canonname = strdup(node);
        if (entry->ai_canonname == NULL) {
            freeaddrinfo(entry);
            return EAI_MEMORY;
        }
    }
    struct sockaddr_in *ipv4 = (struct sockaddr_in *)entry->ai_addr;
    ipv4->sin_family = AF_INET;
    ipv4->sin_port = htons((uint16_t)port);
    ipv4->sin_addr = address;
    entry->ai_family = AF_INET;
    entry->ai_socktype = SOCK_STREAM;
    entry->ai_protocol = IPPROTO_TCP;
    entry->ai_addrlen = sizeof *ipv4;
    entry->ai_flags = flags;
    *result = entry;
    return 0;
}

void freeaddrinfo(struct addrinfo *result)
{
    while (result) {
        struct addrinfo *next = result->ai_next;
        free(result->ai_addr);
        free(result->ai_canonname);
        free(result);
        result = next;
    }
}

const char *gai_strerror(int error)
{
    switch (error) {
    case 0: return "Success";
    case EAI_BADFLAGS: return "Unsupported resolver flags";
    case EAI_NONAME: return "Name not found";
    case EAI_AGAIN: return "Temporary resolution failure";
    case EAI_FAIL: return "Resolution failed";
    case EAI_FAMILY: return "Unsupported address family";
    case EAI_SOCKTYPE: return "Unsupported socket type or protocol";
    case EAI_SERVICE: return "Service must be a numeric port";
    case EAI_MEMORY: return "Out of memory";
    case EAI_SYSTEM: return "System error; inspect errno";
    default: return "Unknown resolver error";
    }
}
