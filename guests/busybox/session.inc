/* Host-only session loop. Code and completion frames never use stdio. */
static int efi_session;

static int efi_ash_session_eval(char *source, int interactive)
{
    if (!interactive)
        return evalstring(source, 0);

    struct jmploc *volatile saved_handler = exception_handler;
    struct jmploc handler;
    int raised = setjmp(handler.loc);
    if (!raised) {
        exception_handler = &handler;
        exitstatus = evalstring(source, 0);
    }
    exception_handler = saved_handler;
    if (raised) {
        if (exception_type != EXERROR && exception_type != EXINT)
            longjmp(exception_handler->loc, raised);
        exitreset();
        reset();
        FORCE_INT_ON;
    }
    return exitstatus;
}
static int efi_ash_session_read(int fd, void *buffer, size_t size)
{
    char *cursor = buffer;
    while (size) {
        ssize_t count = read(fd, cursor, size);
        if (count == 0 && cursor == (char *)buffer)
            return 0;
        if (count <= 0)
            ash_msg_and_raise_error("code session channel read failed");
        cursor += count;
        size -= count;
    }
    return 1;
}

static void efi_ash_session_loop(void)
{
    int fd = open("/.efi-session", O_RDWR | O_CLOEXEC);
    if (fd < 0)
        ash_msg_and_raise_perror("cannot open code session channel");
    for (;;) {
        uint32_t header[3];
        if (!efi_ash_session_read(fd, header, sizeof(header))) {
            close(fd);
            exitshell();
        }
        if (header[1] > 60 * 1024 || header[2] > 1)
            ash_msg_and_raise_error("invalid code session frame");
        char *source = xmalloc(header[1] + 1);
        if (!efi_ash_session_read(fd, source, header[1]))
            ash_msg_and_raise_error("incomplete code session frame");
        source[header[1]] = '\0';
        exitstatus = efi_ash_session_eval(source, header[2]);
        evalskip &= ~(SKIPFUNC | SKIPFUNCDEF);
        free(source);
        uint32_t response[2] = {header[0], (uint32_t)exitstatus};
        if (fflush(NULL) != 0)
            response[1] = 1;
        if (write(fd, response, sizeof(response)) != sizeof(response))
            ash_msg_and_raise_error("code session channel reply failed");
    }
}
