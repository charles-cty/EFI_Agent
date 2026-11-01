#include "busybox.h"
#include "utility_support.h"
#include "process_support.h"
#include "terminal_support.h"

int get_terminal_width(int fd);

typedef uint8_t recurse_flags_t;
