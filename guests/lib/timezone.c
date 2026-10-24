/* Supply bounded TZif file buffers to the upstream musl timezone engine. */
#include "efi_timezone.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TZFILE_LIMIT (1024 * 1024)

static int number(const char **text, unsigned maximum, unsigned *result)
{
    const char *p = *text;
    unsigned value = 0;
    if (*p < '0' || *p > '9')
        return 0;
    while (*p >= '0' && *p <= '9') {
        unsigned digit = (unsigned)(*p++ - '0');
        if (digit > maximum || value > (maximum - digit) / 10)
            return 0;
        value = value * 10 + digit;
    }
    *text = p;
    *result = value;
    return 1;
}

static int zone_name(const char **text)
{
    const char *p = *text;
    const char *start;
    if (*p == '<') {
        start = ++p;
        while ((*p >= 'A' && *p <= 'Z') || (*p >= 'a' && *p <= 'z') ||
               (*p >= '0' && *p <= '9') || *p == '+' || *p == '-')
            ++p;
        if (p - start < 3 || *p != '>')
            return 0;
        ++p;
    } else {
        start = p;
        while ((*p >= 'A' && *p <= 'Z') || (*p >= 'a' && *p <= 'z'))
            ++p;
        if (p - start < 3)
            return 0;
    }
    *text = p;
    return 1;
}

static int offset(const char **text, unsigned maximum)
{
    const char *p = *text;
    unsigned value;
    if (*p == '+' || *p == '-')
        ++p;
    if (!number(&p, maximum, &value))
        return 0;
    for (unsigned i = 0; i < 2 && *p == ':'; ++i) {
        ++p;
        if (!number(&p, 59, &value))
            return 0;
    }
    *text = p;
    return 1;
}

static int rule(const char **text)
{
    const char *p = *text;
    unsigned value;
    if (*p == 'M') {
        ++p;
        if (!number(&p, 12, &value) || !value || *p++ != '.' ||
            !number(&p, 5, &value) || !value || *p++ != '.' ||
            !number(&p, 6, &value))
            return 0;
    } else {
        int julian = *p == 'J';
        p += julian;
        if (!number(&p, 365, &value) || (julian && !value))
            return 0;
    }
    if (*p == '/') {
        ++p;
        if (!offset(&p, 167))
            return 0;
    }
    *text = p;
    return 1;
}

static int valid_posix(const char *text)
{
    const char *p = text;
    if (!zone_name(&p))
        return 0;
    if (!*p)
        return !strcmp(text, "UTC") || !strcmp(text, "GMT");
    if (!offset(&p, 24))
        return 0;
    if (!*p)
        return 1;
    if (!zone_name(&p))
        return 0;
    if (*p && *p != ',' && !offset(&p, 24))
        return 0;
    if (!*p)
        return 1;
    if (*p++ != ',' || !rule(&p) || *p++ != ',' || !rule(&p))
        return 0;
    return !*p;
}

const char *efi_timezone_setting(const char *name)
{
    const char *value = getenv(name);
    if (!value || !*value || *value == ':' || *value == '/' || *value == '.')
        return value;
    const char *p = value;
    /* Named IANA zones go through file lookup. Validate POSIX syntax before
     * passing numeric fields and rule separators to the upstream parser. */
    if (*p == '<') {
        const char *end = strchr(p, '>');
        if (!end)
            return "UTC";
        p = end + 1;
    } else {
        while ((*p >= 'A' && *p <= 'Z') || (*p >= 'a' && *p <= 'z'))
            ++p;
    }
    if (*p == '+' || *p == '-' || (*p >= '0' && *p <= '9'))
        return valid_posix(value) ? value : "UTC";
    return value;
}

static uint32_t read32(const unsigned char *p)
{
    return (uint32_t)p[0] << 24 | (uint32_t)p[1] << 16 |
           (uint32_t)p[2] << 8 | p[3];
}

static int block_end(const unsigned char *data, size_t size, size_t start,
                     unsigned width, size_t *end)
{
    if (start > size || size - start < 44 || memcmp(data + start, "TZif", 4))
        return 0;
    const unsigned char *header = data + start;
    uint32_t counts[6];
    const unsigned widths[6] = {1, 1, width + 4, width + 1, 6, 1};
    size_t remaining = size - start - 44;
    size_t length = 0;
    for (unsigned i = 0; i < 6; ++i) {
        counts[i] = read32(header + 20 + 4 * i);
        if (counts[i] > remaining / widths[i])
            return 0;
        size_t bytes = (size_t)counts[i] * widths[i];
        remaining -= bytes;
        length += bytes;
    }
    if (!counts[4] || counts[4] > 256 || !counts[5] ||
        (counts[0] && counts[0] != counts[4]) ||
        (counts[1] && counts[1] != counts[4]))
        return 0;
    const unsigned char *transitions = header + 44;
    const unsigned char *indices = transitions + (size_t)counts[3] * width;
    const unsigned char *types = indices + counts[3];
    const unsigned char *names = types + 6 * counts[4];
    for (uint32_t i = 0; i < counts[3]; ++i) {
        if (indices[i] >= counts[4])
            return 0;
        if (i) {
            const unsigned char *previous = transitions + (size_t)(i - 1) * width;
            const unsigned char *current = previous + width;
            /* Compare big-endian signed timestamps without signed overflow. */
            int order = (int)(previous[0] ^ 128) - (int)(current[0] ^ 128);
            if (order > 0 || (!order && memcmp(previous + 1, current + 1, width - 1) >= 0))
                return 0;
        }
    }
    for (uint32_t i = 0; i < counts[4]; ++i) {
        const unsigned char *type = types + 6 * i;
        int32_t displacement = (int32_t)read32(type);
        if (displacement < -89999 || displacement > 93599 ||
            type[4] > 1 || type[5] >= counts[5] ||
            !memchr(names + type[5], 0, counts[5] - type[5]))
            return 0;
    }
    *end = start + 44 + length;
    return 1;
}

static int valid_tzif(unsigned char *data, size_t size)
{
    size_t end;
    if (!block_end(data, size, 0, 4, &end))
        return 0;
    if (!data[4] || data[4] == '1')
        return end == size;
    if (data[4] != '2' && data[4] != '3' && data[4] != '4')
        return 0;
    if (!block_end(data, size, end, 8, &end))
        return 0;
    /* The rules engine searches backward for the POSIX footer's first LF. */
    if (size - end < 2 || data[end] != '\n' || data[size - 1] != '\n' ||
        memchr(data + end + 1, 0, size - end - 2) ||
        memchr(data + end + 1, '\n', size - end - 2))
        return 0;
    data[size - 1] = 0;
    int valid = size - end == 2 || valid_posix((const char *)data + end + 1);
    data[size - 1] = '\n';
    return valid;
}

const unsigned char *efi_timezone_map(const char *path, size_t *size)
{
    FILE *file = fopen(path, "rb");
    if (!file)
        return NULL;
    unsigned char *data = malloc(TZFILE_LIMIT + 1);
    if (!data) {
        fclose(file);
        return NULL;
    }
    size_t length = fread(data, 1, TZFILE_LIMIT + 1, file);
    int failed = ferror(file);
    if (fclose(file))
        failed = 1;
    if (failed || length > TZFILE_LIMIT || !valid_tzif(data, length)) {
        free(data);
        return NULL;
    }
    /* POSIX footer parsing can use ordinary NUL-terminated string helpers. */
    data[length] = 0;
    *size = length;
    return data;
}

int efi_timezone_unmap(void *data, size_t size)
{
    (void)size;
    free(data);
    return 0;
}
