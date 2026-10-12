#include "efi_process.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <poll.h>

#define PAYLOAD_BYTES 196609

static void require(int condition)
{
    if (!condition) {
        exit(99);
    }
}

static unsigned char payload_byte(size_t index)
{
    return 'a' + (index * 7 + index / 11) % 26;
}

static void write_all(const unsigned char *bytes, size_t size)
{
    size_t done = 0;
    while (done < size) {
        ssize_t count = write(1, bytes + done, size - done);
        require(count > 0);
        done += count;
    }
}

static void produce(void)
{
    unsigned char bytes[4093];
    size_t position = 0;
    while (position < PAYLOAD_BYTES) {
        size_t size = PAYLOAD_BYTES - position;
        if (size > sizeof bytes)
            size = sizeof bytes;
        for (size_t index = 0; index < size; ++index)
            bytes[index] = payload_byte(position + index);
        write_all(bytes, size);
        position += size;
    }
}

static void consume(int verify, int descriptor)
{
    unsigned char bytes[257];
    size_t position = 0;
    for (;;) {
        ssize_t size = read(descriptor, bytes, sizeof bytes);
        require(size >= 0);
        if (!size)
            break;
        if (verify) {
            for (ssize_t index = 0; index < size; ++index)
                require(bytes[index] == payload_byte(position + index));
        } else {
            write_all(bytes, size);
        }
        position += size;
    }
    if (verify) {
        require(position == PAYLOAD_BYTES);
        puts("ASH_BOUNDED_PIPELINE_OK");
    }
}

int main(int argc, char **argv)
{
    require(efi_process_getppid() > 0);
    if (argc == 3 && (strcmp(argv[1], "emit") == 0 || strcmp(argv[1], "delay_emit") == 0)) {
        if (strcmp(argv[1], "delay_emit") == 0)
            require(poll(NULL, 0, 2000) == 0);
        write_all((const unsigned char *)argv[2], strlen(argv[2]));
        return 0;
    }
    if (argc == 2 && strcmp(argv[1], "produce") == 0) {
        produce();
        return 0;
    }
    if (argc == 2 && (strcmp(argv[1], "copy") == 0 || strcmp(argv[1], "verify") == 0)) {
        consume(strcmp(argv[1], "verify") == 0, 0);
        return 0;
    }
    if (argc == 3 && strcmp(argv[1], "copy_fd") == 0) {
        consume(0, atoi(argv[2]));
        return 0;
    }
    if (argc == 3 && strcmp(argv[1], "exit") == 0) {
        return atoi(argv[2]);
    }
    require(argc == 5 && strcmp(argv[1], "inspect") == 0);
    require(strcmp(argv[2], "") == 0);
    require(strcmp(argv[3], "a'b") == 0);
    require(strcmp(argv[4], "λ") == 0);
    const char *exported = getenv("EXPORTED");
    const char *temporary = getenv("TEMP");
    require(exported && strcmp(exported, "override") == 0);
    require(temporary && strcmp(temporary, "temporary") == 0);
    require(getenv("LOCAL") == NULL);
    puts("ASH_EXTERNAL_ARGUMENTS_ENV_OK");
    fputs("ASH_EXTERNAL_STDERR_OK\n", stderr);
    return 7;
}
