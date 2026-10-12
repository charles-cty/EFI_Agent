#include <errno.h>
#include <limits.h>
#include <netdb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
#include <wolfssh/ssh.h>
#include <wolfssh/error.h>
#include <wolfssl/wolfcrypt/sha256.h>
#include <wolfssl/wolfcrypt/hash.h>

struct client {
    int socket;
    byte pin[WC_SHA256_DIGEST_SIZE];
    byte password[4096];
    word32 password_size;
    int password_attempted;
};

static int socket_error(int error) {
    switch (error) {
    case EAGAIN: return WS_CBIO_ERR_WANT_READ;
    case EINTR: return WS_CBIO_ERR_ISR;
    case ECONNRESET: return WS_CBIO_ERR_CONN_RST;
    case EPIPE: return WS_CBIO_ERR_CONN_CLOSE;
    case ETIMEDOUT: return WS_CBIO_ERR_TIMEOUT;
    default: return WS_CBIO_ERR_GENERAL;
    }
}

static int receive(WOLFSSH *ssh, void *buffer, word32 size, void *context) {
    (void)ssh;
    struct client *client = context;
    ssize_t count = recv(client->socket, buffer, size, 0);
    if (count < 0) return socket_error(errno);
    if (count == 0) return WS_CBIO_ERR_CONN_CLOSE;
    return (int)count;
}

static int send_bytes(WOLFSSH *ssh, void *buffer, word32 size, void *context) {
    (void)ssh;
    struct client *client = context;
    ssize_t count = send(client->socket, buffer, size, MSG_NOSIGNAL);
    if (count < 0) return socket_error(errno);
    if (count == 0) return WS_CBIO_ERR_CONN_CLOSE;
    return (int)count;
}

static int authenticate(byte type, WS_UserAuthData *data, void *context) {
    struct client *client = context;
    if (type != WOLFSSH_USERAUTH_PASSWORD)
        return WOLFSSH_USERAUTH_INVALID_AUTHTYPE;
    if (client->password_attempted) return WOLFSSH_USERAUTH_FAILURE;
    client->password_attempted = 1;
    data->sf.password.password = client->password;
    data->sf.password.passwordSz = client->password_size;
    return WOLFSSH_USERAUTH_SUCCESS;
}

static int check_host_key(const byte *key, word32 size, void *context) {
    struct client *client = context;
    byte digest[WC_SHA256_DIGEST_SIZE];
    if (wc_Sha256Hash(key, size, digest) != 0) return -1;
    unsigned int difference = 0;
    for (size_t index = 0; index < sizeof(digest); ++index)
        difference |= digest[index] ^ client->pin[index];
    if (difference != 0) {
        fputs("wolfssh: host key does not match SHA256 pin\n", stderr);
        return -1;
    }
    return 0;
}

static int hex_digit(char digit) {
    if (digit >= '0' && digit <= '9') return digit - '0';
    if (digit >= 'a' && digit <= 'f') return digit - 'a' + 10;
    if (digit >= 'A' && digit <= 'F') return digit - 'A' + 10;
    return -1;
}

static int parse_pin(struct client *client, const char *text) {
    if (strlen(text) != sizeof(client->pin) * 2) return -1;
    for (size_t index = 0; index < sizeof(client->pin); ++index) {
        int high = hex_digit(text[index * 2]);
        int low = hex_digit(text[index * 2 + 1]);
        if (high < 0 || low < 0) return -1;
        client->pin[index] = (byte)((high << 4) | low);
    }
    return 0;
}

static int load_password(struct client *client, const char *path) {
    FILE *file = fopen(path, "rb");
    if (!file) return -1;
    size_t size = fread(client->password, 1, sizeof(client->password), file);
    int next = fgetc(file);
    int failed = ferror(file) || next != EOF;
    if (fclose(file) != 0) failed = 1;
    if (failed || memchr(client->password, 0, size)) return -1;
    if (size && client->password[size - 1] == '\n') {
        --size;
        if (size && client->password[size - 1] == '\r') --size;
    }
    client->password_size = (word32)size;
    return 0;
}

static int connect_host(const char *host, const char *port) {
    const struct addrinfo hints = {
        .ai_family = AF_INET, .ai_socktype = SOCK_STREAM,
        .ai_flags = AI_NUMERICSERV
    };
    struct addrinfo *addresses = NULL;
    int error = getaddrinfo(host, port, &hints, &addresses);
    if (error != 0) {
        fprintf(stderr, "wolfssh: resolution failed: %s\n", gai_strerror(error));
        return -1;
    }
    int descriptor = -1;
    for (struct addrinfo *address = addresses; address; address = address->ai_next) {
        descriptor = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
        if (descriptor < 0) break;
        if (connect(descriptor, address->ai_addr, address->ai_addrlen) == 0) break;
        close(descriptor);
        descriptor = -1;
    }
    freeaddrinfo(addresses);
    if (descriptor < 0) perror("wolfssh: connect");
    return descriptor;
}

static int drain_stderr(WOLFSSH *ssh) {
    byte buffer[8192];
    int count;
    while ((count = wolfSSH_extended_data_read(ssh, buffer, sizeof(buffer))) > 0) {
        if (fwrite(buffer, 1, (size_t)count, stderr) != (size_t)count)
            return WS_ERROR;
    }
    return count;
}

static int transient(int error) {
    return error == WS_WANT_READ || error == WS_WANT_WRITE || error == WS_REKEYING;
}

static int read_command(WOLFSSH *ssh) {
    byte buffer[8192];
    int eof = 0;
    for (;;) {
        int result = eof ? wolfSSH_worker(ssh, NULL)
                         : wolfSSH_stream_read(ssh, buffer, sizeof(buffer));
        int error = result == WS_ERROR ? wolfSSH_get_error(ssh) : result;
        if (!eof && result > 0) {
            if (fwrite(buffer, 1, (size_t)result, stdout) != (size_t)result)
                return WS_ERROR;
            continue;
        }
        if (error == WS_CHANNEL_CLOSED) return WS_SUCCESS;
        /* stream_read maps a retired final channel to EOF. There is no
           remaining stderr buffer or channel for worker to service. */
        if (error == WS_EOF && wolfSSH_ChannelNext(ssh, NULL) == NULL)
            return WS_SUCCESS;
        if (error == WS_EXTDATA || error == WS_EOF) {
            int drained = drain_stderr(ssh);
            if (drained < 0) return drained;
            if (error == WS_EOF) eof = 1;
            continue;
        }
        if (transient(error) || (eof && error == WS_SUCCESS)) continue;
        return error;
    }
}

int main(int argc, char **argv) {
    const char *host = NULL, *port = "22", *user = NULL;
    const char *pin = NULL, *password_file = NULL, *command = NULL;
    for (int index = 1; index < argc; index += 2) {
        if (!strcmp(argv[index], "--") && index + 2 == argc) {
            command = argv[index + 1];
            break;
        }
        if (index + 1 >= argc) break;
        if (!strcmp(argv[index], "--host")) host = argv[index + 1];
        else if (!strcmp(argv[index], "--port")) port = argv[index + 1];
        else if (!strcmp(argv[index], "--user")) user = argv[index + 1];
        else if (!strcmp(argv[index], "--host-key-sha256")) pin = argv[index + 1];
        else if (!strcmp(argv[index], "--password-file")) password_file = argv[index + 1];
        else break;
    }
    struct client client = { .socket = -1 };
    if (!host || !user || !pin || !password_file || !command || !*command ||
        parse_pin(&client, pin) != 0) {
        fputs("Usage: wolfssh --host HOST [--port PORT] --user USER "
              "--host-key-sha256 HEX --password-file FILE -- COMMAND\n", stderr);
        return 2;
    }
    if (load_password(&client, password_file) != 0) {
        fputs("wolfssh: cannot read bounded password file\n", stderr);
        return 255;
    }
    if (wolfSSH_Init() != WS_SUCCESS) return 255;
    WOLFSSH_CTX *context = wolfSSH_CTX_new(WOLFSSH_ENDPOINT_CLIENT, NULL);
    WOLFSSH *ssh = NULL;
    int status = 255;
    if (!context) goto cleanup;
    wolfSSH_SetIORecv(context, receive);
    wolfSSH_SetIOSend(context, send_bytes);
    wolfSSH_SetUserAuth(context, authenticate);
    wolfSSH_CTX_SetPublicKeyCheck(context, check_host_key);
    ssh = wolfSSH_new(context);
    if (!ssh) goto cleanup;
    wolfSSH_SetIOReadCtx(ssh, &client);
    wolfSSH_SetIOWriteCtx(ssh, &client);
    wolfSSH_SetUserAuthCtx(ssh, &client);
    wolfSSH_SetPublicKeyCheckCtx(ssh, &client);
    if (wolfSSH_SetUsername(ssh, user) != WS_SUCCESS ||
        wolfSSH_SetChannelType(ssh, WOLFSSH_SESSION_EXEC,
                              (byte *)command, (word32)strlen(command)) != WS_SUCCESS)
        goto cleanup;
    /* Distinguish a missing exit-status request from remote success. */
    wolfSSH_SetExitStatus(ssh, UINT_MAX);
    client.socket = connect_host(host, port);
    if (client.socket < 0) goto cleanup;
    int result;
    do {
        result = wolfSSH_connect(ssh);
    } while (result != WS_SUCCESS && transient(wolfSSH_get_error(ssh)));
    if (result != WS_SUCCESS) {
        fprintf(stderr, "wolfssh: handshake failed: %d\n", wolfSSH_get_error(ssh));
        goto cleanup;
    }
    /* This exec client provides no stdin. Keep receiving until channel close,
       because exit-status can arrive after the peer's EOF. */
    if (wolfSSH_stream_send_eof(ssh) != WS_SUCCESS) goto cleanup;
    result = read_command(ssh);
    if (result != WS_SUCCESS) {
        fprintf(stderr, "wolfssh: channel failed: %d\n", result);
        goto cleanup;
    }
    status = wolfSSH_GetExitStatus(ssh);
    if (status < 0 || status > 255) {
        fputs("wolfssh: missing or out-of-range remote exit status\n", stderr);
        status = 255;
    }
cleanup:
    wolfSSH_free(ssh);
    wolfSSH_CTX_free(context);
    if (client.socket >= 0) close(client.socket);
    wolfSSH_Cleanup();
    volatile byte *secret = client.password;
    for (size_t index = 0; index < sizeof(client.password); ++index) secret[index] = 0;
    return status;
}
