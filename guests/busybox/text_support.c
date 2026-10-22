#include "busybox.h"
#include "utility_support.h"

char *bb_read_record(FILE *stream, int delimiter, size_t *length)
{
    char *line = NULL;
    size_t capacity = 0;
    ssize_t count = getdelim(&line, &capacity, delimiter, stream);
    if (count < 0) {
        free(line);
        return NULL;
    }
    if (count && (unsigned char)line[count - 1] == delimiter)
        --count;
    line[count] = '\0';
    *length = count;
    return line;
}

int bb_compare_record(const char *left, const char *right, size_t length, int fold)
{
    if (!fold)
        return memcmp(left, right, length);
    for (size_t i = 0; i < length; ++i) {
        int a = tolower((unsigned char)left[i]);
        int b = tolower((unsigned char)right[i]);
        if (a != b)
            return a - b;
    }
    return 0;
}

/* Compare decimal prefixes without rounding integer or fractional digits. */
struct decimal {
    const char *integer;
    size_t integers;
    const char *fraction;
    size_t fractions;
    const char *tail;
    int negative;
};

static struct decimal decimal_prefix(const char *text)
{
    struct decimal value = {0};
    while (isspace((unsigned char)*text))
        ++text;
    if (*text == '-') {
        value.negative = 1;
        ++text;
    }
    while (*text == '0')
        ++text;
    value.integer = text;
    while (isdigit((unsigned char)*text))
        ++text;
    value.integers = text - value.integer;
    value.fraction = text;
    if (*text == '.') {
        value.fraction = ++text;
        while (isdigit((unsigned char)*text))
            ++text;
        value.fractions = text - value.fraction;
        while (value.fractions && value.fraction[value.fractions - 1] == '0')
            --value.fractions;
    }
    if (!value.integers && !value.fractions)
        value.negative = 0;
    value.tail = text;
    return value;
}

int bb_compare_decimal(const char *left, const char *right)
{
    struct decimal a = decimal_prefix(left);
    struct decimal b = decimal_prefix(right);
    if (a.negative != b.negative)
        return a.negative ? -1 : 1;
    int order;
    if (a.integers != b.integers) {
        order = a.integers > b.integers ? 1 : -1;
    } else {
        order = memcmp(a.integer, b.integer, a.integers);
        if (!order) {
            size_t common = a.fractions < b.fractions ? a.fractions : b.fractions;
            order = memcmp(a.fraction, b.fraction, common);
            if (!order && a.fractions != b.fractions)
                order = a.fractions > b.fractions ? 1 : -1;
        }
    }
    order = (order > 0) - (order < 0);
    return a.negative ? -order : order;
}

static int human_suffix(const char *tail)
{
    const char *suffixes = "kmgtpezy";
    if (!*tail)
        return -1;
    const char *suffix = strchr(suffixes, tolower((unsigned char)*tail));
    if (!suffix || (suffix != suffixes && *tail >= 'a'))
        return -1;
    return suffix - suffixes;
}

int bb_compare_human(const char *left, const char *right)
{
    struct decimal a = decimal_prefix(left);
    struct decimal b = decimal_prefix(right);
    int a_sign = a.integers || a.fractions ? (a.negative ? -1 : 1) : 0;
    int b_sign = b.integers || b.fractions ? (b.negative ? -1 : 1) : 0;
    if (a_sign != b_sign)
        return (a_sign > b_sign) - (a_sign < b_sign);
    if (!a_sign)
        return 0;
    int a_suffix = human_suffix(a.tail);
    int b_suffix = human_suffix(b.tail);
    if (a_suffix != b_suffix)
        return a_sign * ((a_suffix > b_suffix) - (a_suffix < b_suffix));
    return bb_compare_decimal(left, right);
}
