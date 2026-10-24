/* Included in the upstream ash translation unit. Wire values are little-endian
 * integers and length-prefixed strings, never instance-local pointers.
 */
#define EFI_TASK_BYTES (512 * 1024)
#define EFI_TASK_DEPTH 256

struct efi_task_codec {
    unsigned char *bytes;
    size_t size;
    size_t cursor;
    unsigned depth;
    bool reading;
};

static void efi_task_bytes(struct efi_task_codec *codec, void *value, size_t size)
{
    if (size > codec->size - codec->cursor)
        ash_msg_and_raise_error("shell task state exceeds its transfer limit or is truncated");
    if (codec->reading)
        memcpy(value, codec->bytes + codec->cursor, size);
    else
        memcpy(codec->bytes + codec->cursor, value, size);
    codec->cursor += size;
}

static uint32_t efi_task_word(struct efi_task_codec *codec, uint32_t value)
{
    unsigned char bytes[4];
    if (!codec->reading) {
        for (unsigned index = 0; index < 4; ++index)
            bytes[index] = value >> (index * 8);
    }
    efi_task_bytes(codec, bytes, sizeof bytes);
    value = 0;
    for (unsigned index = 0; index < 4; ++index)
        value |= (uint32_t)bytes[index] << (index * 8);
    return value;
}

static char *efi_task_string(struct efi_task_codec *codec, char *value)
{
    uint32_t size = efi_task_word(codec, value ? strlen(value) + 1 : 0);
    if (!size)
        return NULL;
    if (size > codec->size - codec->cursor)
        ash_msg_and_raise_error(codec->reading ? "invalid shell task string size"
            : "shell task state exceeds its transfer limit");
    if (codec->reading)
        value = ckzalloc(size);
    efi_task_bytes(codec, value, size);
    if (value[size - 1] != 0 || memchr(value, 0, size - 1))
        ash_msg_and_raise_error("invalid shell task string");
    return value;
}

static union node *efi_task_node(struct efi_task_codec *, union node *);

static struct nodelist *efi_task_list(struct efi_task_codec *codec, struct nodelist *list)
{
    struct nodelist *head = NULL;
    struct nodelist **tail = &head;
    struct nodelist *entry = list;
    while (efi_task_word(codec, entry != NULL)) {
        if (codec->reading)
            entry = ckzalloc(sizeof *entry);
        entry->n = efi_task_node(codec, entry->n);
        if (codec->reading) {
            *tail = entry;
            tail = &entry->next;
        }
        entry = entry->next;
    }
    return codec->reading ? head : list;
}

static union node *efi_task_node(struct efi_task_codec *codec, union node *node)
{
    uint32_t type = efi_task_word(codec, node ? node->type + 1 : 0);
    if (!type)
        return NULL;
    if (type > N_NUMBER || ++codec->depth > EFI_TASK_DEPTH)
        ash_msg_and_raise_error("invalid shell task node type or depth");
    if (codec->reading) {
        node = ckzalloc(sizeof *node);
        node->type = type - 1;
    }
#define TASK_WORD(field) node->field = efi_task_word(codec, node->field)
#define TASK_NODE(field) node->field = efi_task_node(codec, node->field)
#define TASK_STRING(field) node->field = efi_task_string(codec, node->field)
    switch (node->type) {
    case NCMD:
        TASK_WORD(ncmd.linno);
        TASK_NODE(ncmd.assign);
        TASK_NODE(ncmd.args);
        TASK_NODE(ncmd.redirect);
        break;
    case NPIPE:
        TASK_WORD(npipe.pipe_backgnd);
        node->npipe.cmdlist = efi_task_list(codec, node->npipe.cmdlist);
        break;
    case NREDIR:
    case NBACKGND:
    case NSUBSHELL:
        TASK_WORD(nredir.linno);
        TASK_NODE(nredir.n);
        TASK_NODE(nredir.redirect);
        break;
    case NAND:
    case NOR:
    case NSEMI:
    case NWHILE:
    case NUNTIL:
        TASK_NODE(nbinary.ch1);
        TASK_NODE(nbinary.ch2);
        break;
    case NIF:
        TASK_NODE(nif.test);
        TASK_NODE(nif.ifpart);
        TASK_NODE(nif.elsepart);
        break;
    case NFOR:
        TASK_WORD(nfor.linno);
        TASK_STRING(nfor.var);
        TASK_NODE(nfor.args);
        TASK_NODE(nfor.body);
        break;
    case NCASE:
        TASK_WORD(ncase.linno);
        TASK_NODE(ncase.expr);
        TASK_NODE(ncase.cases);
        break;
    case NCLIST:
        TASK_NODE(nclist.next);
        TASK_NODE(nclist.pattern);
        TASK_NODE(nclist.body);
        break;
    case NDEFUN:
        TASK_WORD(ndefun.linno);
        TASK_STRING(ndefun.text);
        TASK_NODE(ndefun.body);
        break;
    case NARG:
        TASK_NODE(narg.next);
        TASK_STRING(narg.text);
        node->narg.backquote = efi_task_list(codec, node->narg.backquote);
        break;
    case NTO:
    case NCLOBBER:
    case NFROM:
    case NFROMTO:
    case NAPPEND:
        TASK_NODE(nfile.next);
        TASK_WORD(nfile.fd);
        TASK_NODE(nfile.fname);
        TASK_STRING(nfile.expfname);
        break;
    case NTOFD:
    case NFROMFD:
        TASK_NODE(ndup.next);
        TASK_WORD(ndup.fd);
        TASK_WORD(ndup.dupfd);
        TASK_NODE(ndup.vname);
        break;
    case NHERE:
    case NXHERE:
        TASK_NODE(nhere.next);
        TASK_WORD(nhere.fd);
        TASK_NODE(nhere.doc);
        break;
    case NNOT:
        TASK_NODE(nnot.com);
        break;
    default:
        ash_msg_and_raise_error("unsupported shell task node");
    }
#undef TASK_WORD
#undef TASK_NODE
#undef TASK_STRING
    --codec->depth;
    return node;
}

static union node *efi_task_state(struct efi_task_codec *codec, union node *node)
{
    if (efi_task_word(codec, 0x31545341) != 0x31545341)
        ash_msg_and_raise_error("invalid shell task state version");
    rootpid = efi_task_word(codec, rootpid);
    exitstatus = efi_task_word(codec, exitstatus);
    backgndpid = efi_task_word(codec, backgndpid);
    arg0 = efi_task_string(codec, (char *)arg0);
    uint32_t parameters = efi_task_word(codec, shellparam.nparam);
    if (parameters > EFI_TASK_BYTES / 4)
        ash_msg_and_raise_error("invalid shell task parameter count");
    if (codec->reading) {
        shellparam.p = ckzalloc((parameters + 1) * sizeof(char *));
        shellparam.nparam = parameters;
        shellparam.malloced = 1;
    }
    for (uint32_t index = 0; index < parameters; ++index)
        shellparam.p[index] = efi_task_string(codec, shellparam.p[index]);

    uint32_t variables = 0;
    if (!codec->reading) {
        for (unsigned index = 0; index < VTABSIZE; ++index)
            for (struct var *var = vartab[index]; var; var = var->next)
                ++variables;
    }
    variables = efi_task_word(codec, variables);
    if (variables > EFI_TASK_BYTES / 8)
        ash_msg_and_raise_error("invalid shell task variable count");
    if (codec->reading) {
        for (uint32_t index = 0; index < variables; ++index) {
            unsigned flags = efi_task_word(codec, 0);
            char *text = efi_task_string(codec, NULL);
            if (!text || flags & ~(VEXPORT | VREADONLY | VUNSET))
                ash_msg_and_raise_error("invalid shell task variable");
            setvareq(text, flags | VNOSAVE);
        }
    } else {
        for (unsigned index = 0; index < VTABSIZE; ++index) {
            for (struct var *var = vartab[index]; var; var = var->next) {
                efi_task_word(codec, var->flags & (VEXPORT | VREADONLY | VUNSET));
                efi_task_string(codec, (char *)var->var_text);
            }
        }
    }

    if (codec->reading) {
        while (efi_task_word(codec, 0)) {
            union node *function = efi_task_node(codec, NULL);
            if (!function || function->type != NDEFUN)
                ash_msg_and_raise_error("invalid shell task function");
            defun(function);
        }
    } else {
        for (unsigned index = 0; index < CMDTABLESIZE; ++index) {
            for (struct tblentry *entry = cmdtable[index]; entry; entry = entry->next) {
                if (entry->cmdtype == CMDFUNCTION) {
                    efi_task_word(codec, 1);
                    efi_task_node(codec, &entry->param.func->n);
                }
            }
        }
        efi_task_word(codec, 0);
    }
    if (codec->reading) {
        while (efi_task_word(codec, 0)) {
            char *name = efi_task_string(codec, NULL);
            char *value = efi_task_string(codec, NULL);
            if (!name || !value)
                ash_msg_and_raise_error("invalid shell task alias");
            setalias(name, value);
            free(name);
            free(value);
        }
    } else {
        for (unsigned index = 0; index < ATABSIZE; ++index) {
            for (struct alias *alias = atab[index]; alias; alias = alias->next) {
                if (!(alias->flag & ALIASDEAD)) {
                    efi_task_word(codec, 1);
                    efi_task_string(codec, alias->name);
                    efi_task_string(codec, alias->val);
                }
            }
        }
        efi_task_word(codec, 0);
    }
    shellparam.optind = efi_task_word(codec, shellparam.optind);
    shellparam.optoff = efi_task_word(codec, shellparam.optoff);
    for (unsigned index = 0; index < NOPTS; ++index)
        optlist[index] = efi_task_word(codec, optlist[index]);
    if (codec->reading) {
        iflag = mflag = 0;
        optschanged();
    }
    return efi_task_node(codec, node);
}

int efi_ash_spawn_task(union node *node, uint32_t input, uint32_t output, unsigned flags)
{
    if (!node)
        ash_msg_and_raise_error("shell task requires a syntax tree");
    unsigned char *bytes = ckzalloc(EFI_TASK_BYTES);
    struct efi_task_codec codec = {bytes, EFI_TASK_BYTES, 0, 0, false};
    efi_task_word(&codec, flags);
    efi_task_state(&codec, node);
    int descriptor = efi_ash_input_snapshot(bytes, codec.cursor);
    free(bytes);
    if (descriptor < 0)
        ash_msg_and_raise_error("cannot create shell task input");
    if (efi_process_set_fd_flags(descriptor, 1) != 0)
        __builtin_trap();
    struct efi_process_fd_mapping mappings[127];
    unsigned count = 0;
    bool occupied[128] = {false};
    /* Explicit stdio can select marked internal pipe ends. The remaining
     * entries retain user descriptors and omit internal saved descriptors.
     */
    uint32_t sources[] = {input, output, 2};
    for (unsigned target = 0; target < 3; ++target) {
        if (sources[target] != (uint32_t)descriptor
            && efi_process_get_fd_flags(sources[target]) >= 0)
            mappings[count++] = (struct efi_process_fd_mapping){target, sources[target]};
    }
    for (unsigned fd = 4; fd < 128; ++fd) {
        if (efi_process_get_fd_flags(fd) == 0) {
            mappings[count++] = (struct efi_process_fd_mapping){fd, fd};
            occupied[fd] = true;
        }
    }
    unsigned task_fd = 4;
    while (task_fd < 128 && occupied[task_fd])
        ++task_fd;
    if (task_fd == 128) {
        close(descriptor);
        return -24;
    }
    mappings[count++] = (struct efi_process_fd_mapping){task_fd, descriptor};
    char size[32];
    snprintf(size, sizeof size, "%zu", codec.cursor);
    char fd_text[16];
    snprintf(fd_text, sizeof fd_text, "%u", task_fd);
    const char *arguments[] = {"/bin/ash", "--efi-task", size, fd_text};
    char scratch[128];
    int32_t argument_size = efi_process_pack_argv(arguments, 4, scratch, sizeof scratch);
    int32_t pid = argument_size < 0 ? argument_size
        : efi_process_spawn_mapped_fds_raw(scratch, argument_size, NULL, 0,
            mappings, count);
    /* The child's copy retains the snapshot after successful spawn. On failure
     * this close releases the only copy, before the caller reports the error.
     */
    if (close(descriptor) != 0)
        ash_msg_and_raise_error("cannot close shell task input");
    return pid;
}

int efi_ash_spawn_node(union node *node, int mode)
{
    (void)mode;
    return efi_ash_spawn_task(node, 0, 1, 0);
}

static size_t efi_task_unsigned(const char *text, size_t maximum)
{
    size_t size = 0;
    for (const char *digit = text; *digit; ++digit) {
        if (*digit < '0' || *digit > '9'
            || size > maximum / 10
            || (size == maximum / 10 && (unsigned)(*digit - '0') > maximum % 10))
            ash_msg_and_raise_error("invalid shell task transfer size");
        size = size * 10 + (unsigned)(*digit - '0');
    }
    if (!size)
        ash_msg_and_raise_error("invalid shell task transfer size");
    return size;
}

static void efi_ash_task_entry(const char *length, const char *fd_text)
{
    size_t size = efi_task_unsigned(length, EFI_TASK_BYTES);
    unsigned fd = efi_task_unsigned(fd_text, 127);
    if (fd < 4)
        ash_msg_and_raise_error("invalid shell task input descriptor");
    unsigned char *bytes = ckzalloc(size);
    size_t received = 0;
    while (received < size) {
        ssize_t count = read(fd, bytes + received, size - received);
        if (count <= 0) {
            close(fd);
            free(bytes);
            ash_msg_and_raise_error("invalid or truncated shell task input");
        }
        received += count;
    }
    unsigned char extra;
    ssize_t remaining = read(fd, &extra, 1);
    int closed = close(fd);
    if (remaining != 0 || closed != 0) {
        free(bytes);
        ash_msg_and_raise_error("invalid shell task input length");
    }
    struct efi_task_codec codec = {bytes, size, 0, 0, true};
    unsigned flags = efi_task_word(&codec, 0);
    if (flags & ~(EFI_TASK_CLEAR_ERREXIT | EFI_TASK_TESTED | EFI_TASK_REDIR_EXPANDED))
        ash_msg_and_raise_error("invalid shell task execution flags");
    union node *node = efi_task_state(&codec, NULL);
    if (!node || codec.cursor != codec.size)
        ash_msg_and_raise_error("invalid shell task trailing bytes or root");
    free(bytes);
    if (flags & EFI_TASK_CLEAR_ERREXIT)
        eflag = 0;
    int evaluation_flags = EV_EXIT | ((flags & EFI_TASK_TESTED) ? EV_TESTED : 0);
    if (flags & EFI_TASK_REDIR_EXPANDED) {
        if (node->type != NSUBSHELL && node->type != NBACKGND)
            ash_msg_and_raise_error("invalid expanded shell task redirection");
        /* The parent already expanded these redirections. Do not repeat
         * substitutions or parameter-assignment side effects in the child.
         */
        redirect(node->nredir.redirect, 0);
        evaltreenr(node->nredir.n, evaluation_flags);
    }
    if (node->type == NBACKGND)
        node->type = NSUBSHELL;
    evaltreenr(node, evaluation_flags);
}
