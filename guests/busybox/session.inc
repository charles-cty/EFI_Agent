/* Host-only session loop. Code and completion frames never use stdio. */
static int efi_session;
static void efi_ash_session_read(int fd, void *buffer, size_t size)
{
    char *cursor = buffer;
    while (size) {
        ssize_t count = read(fd, cursor, size);
        if (count <= 0)
            ash_msg_and_raise_error("code session channel read failed");
        cursor += count;
        size -= count;
    }
}

static void efi_ash_session_loop(void)
{
    int fd = open("/.efi-session", O_RDWR | O_CLOEXEC);
    if (fd < 0)
        ash_msg_and_raise_perror("cannot open code session channel");
    for (;;) {
        uint32_t header[2];
        efi_ash_session_read(fd, header, sizeof(header));
        if (header[1] > 60 * 1024)
            ash_msg_and_raise_error("code session frame exceeds 60 KiB");
        char *source = xmalloc(header[1] + 1);
        efi_ash_session_read(fd, source, header[1]);
        source[header[1]] = '\0';
        exitstatus = evalstring(source, 0);
        evalskip &= ~(SKIPFUNC | SKIPFUNCDEF);
        free(source);
        uint32_t response[2] = {header[0], (uint32_t)exitstatus};
        if (fflush(NULL) != 0)
            response[1] = 1;
        if (write(fd, response, sizeof(response)) != sizeof(response))
            ash_msg_and_raise_error("code session channel reply failed");
    }
}
