#ifndef EFI_POSIX_SOCKET_H
#define EFI_POSIX_SOCKET_H

#include_next <sys/socket.h>

#define SO_ERROR 4
#define SO_KEEPALIVE 9
#define SOL_IP 0
#define MSG_NOSIGNAL 0x4000

#ifdef __cplusplus
extern "C" {
#endif
int socket(int domain, int type, int protocol);
int connect(int descriptor, const struct sockaddr *address, socklen_t length);
int bind(int descriptor, const struct sockaddr *address, socklen_t length);
int setsockopt(int descriptor, int level, int option, const void *value, socklen_t length);
int getsockname(int descriptor, struct sockaddr *address, socklen_t *length);
#ifdef __cplusplus
}
#endif

#endif
