#ifndef EFI_BUSYBOX_SORT_SUPPORT_H
#define EFI_BUSYBOX_SORT_SUPPORT_H

/* Lengths and original positions survive upstream key extraction and qsort. */
struct sort_record {
    size_t length;
    uint32_t ordinal;
    char bytes[];
};

static struct sort_record *sort_record(char *bytes)
{
    return (void *)(bytes - offsetof(struct sort_record, bytes));
}

static size_t sort_length(char *bytes)
{
    return sort_record(bytes)->length;
}

static char *sort_duplicate(const char *bytes, size_t length)
{
    struct sort_record *record = xmalloc(sizeof(*record) + length + 1);
    record->length = length;
    record->ordinal = 0;
    memcpy(record->bytes, bytes, length);
    record->bytes[length] = '\0';
    return record->bytes;
}

static void sort_free(char *bytes)
{
    free(sort_record(bytes));
}

static int sort_compare(char *left, char *right)
{
    size_t a = sort_length(left), b = sort_length(right);
    int order = memcmp(left, right, a < b ? a : b);
    if (order)
        return (order > 0) - (order < 0);
    return (a > b) - (a < b);
}

static char *sort_read(FILE *stream, int delimiter)
{
    size_t length;
    char *bytes = bb_read_record(stream, delimiter, &length);
    if (!bytes)
        return NULL;
    char *record = sort_duplicate(bytes, length);
    free(bytes);
    return record;
}

static size_t sort_field_start(const char *bytes, size_t length, unsigned field, char separator)
{
    size_t offset = 0;
    for (unsigned i = 1; i < field && offset < length; ++i) {
        if (separator) {
            while (offset < length) {
                if (bytes[offset++] == separator)
                    break;
            }
        } else {
            while (offset < length && isspace((unsigned char)bytes[offset]))
                ++offset;
            while (offset < length && !isspace((unsigned char)bytes[offset]))
                ++offset;
        }
    }
    return offset;
}

#endif
