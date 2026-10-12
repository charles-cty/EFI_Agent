# /// script
# requires-python = ">=3.11"
# ///
"""Prepare upstream BusyBox ash for the Windows-native WASI build."""
import argparse
from pathlib import Path
import re


ENABLED = {
    "ASH", "SHELL_ASH", "ASH_ALIAS", "ASH_CMDCMD", "ASH_GETOPTS",
    "ASH_INTERNAL_GLOB", "ASH_ECHO", "ASH_PRINTF", "ASH_TEST", "FEATURE_TEST_64",
    "FEATURE_SH_MATH", "FEATURE_SH_MATH_64", "FEATURE_SH_MATH_BASE",
    "FEATURE_FANCY_ECHO", "FEATURE_SH_NOFORK",
}


def replace_once(text, marker, replacement):
    if text.count(marker) != 1:
        raise ValueError(f"Upstream source marker changed: {marker!r}")
    return text.replace(marker, replacement)


def prepare_read(source: Path, output: Path):
    """Keep upstream splitting and escaping; adapt only platform boundaries."""
    common = (source / "shell/shell_common.c").read_text(encoding="utf-8")
    start = common.index("const char* FAST_FUNC\nshell_builtin_read(")
    end = common.index("\n/* ulimit builtin */", start)
    read = common[start:end]
    read = read.replace("\tstruct termios tty, old_tty;\n", "")
    start = read.index("\tif (nchars || (read_flags & BUILTIN_READ_SILENT)) {")
    end = read.index("\n\tretval =", start)
    read = read[:start] + '''    if (read_flags & BUILTIN_READ_SILENT)
        return "read: terminal echo control is unavailable";
''' + read[end:]
    read = read.replace("\tif (read_flags & BUILTIN_READ_SILENT)\n\t\ttcsetattr(fd, TCSANOW, &old_tty);\n", "")
    read = read.replace("\tif (params->opt_t)\n\t\tend_ms += (unsigned)monotonic_ms();", '''    if (params->opt_t) {
        unsigned now;
        if (efi_read_monotonic_ms(&now) < 0)
            return "read: monotonic timeout clock is unavailable";
        end_ms += now;
    }''')
    read = read.replace("\t\t\ttimeout = end_ms - (unsigned)monotonic_ms();", '''            unsigned now;
            if (efi_read_monotonic_ms(&now) < 0) {
                err = errno;
                retval = (const char *)(uintptr_t)1;
                goto ret;
            }
            timeout = end_ms - now;''')
    read = read.replace("return (const char *)(uintptr_t)(r <= 0);",
                        "return (const char *)(uintptr_t)(r <= 0 || (pfd[0].revents & (POLLERR | POLLNVAL)));")
    preamble = '''#include "busybox.h"
#include "shell_common.h"
#include <poll.h>

static int efi_read_monotonic_ms(unsigned *value)
{
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) < 0)
        return -1;
    *value = (uint64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
    return 0;
}

'''
    license_header = common[:common.index('#include "libbb.h"')]
    (output / "shell_read.c").write_text(license_header + preamble + read, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    files = [args.source / "shell/ash.c", args.source / "shell/shell_common.h",
             args.source / "shell/shell_common.c",
             args.source / "coreutils/printf.c",
             args.source / "coreutils/test.c", args.source / "shell/math.c",
             args.source / "include/unicode.h"]
    names = set(ENABLED)
    for path in files:
        text = path.read_text(encoding="utf-8")
        names.update(re.findall(r"\bENABLE_([A-Z0-9_]+)", text))
        names.update(re.findall(r"\bIF_(?:NOT_)?([A-Z0-9_]+)", text))
    # BASH_* helper macros are defined by upstream ash itself.
    names = {name for name in names if not name.startswith("BASH_") or name == "BASH_IS_ASH"}
    lines = ["#ifndef EFI_ASH_CONFIG_H", "#define EFI_ASH_CONFIG_H"]
    for name in sorted(names):
        enabled = name in ENABLED
        lines.append(f"#define ENABLE_{name} {int(enabled)}")
        lines.append(f"#define IF_{name}(...) {'__VA_ARGS__' if enabled else ''}")
        lines.append(f"#define IF_NOT_{name}(...) {'' if enabled else '__VA_ARGS__'}")
    lines.extend(["#define HAVE_DEV_FD 0", "#define CONFIG_FEATURE_EDITING_MAX_LEN 1024", "#endif", ""])
    (args.output / "ash_config.h").write_text("\n".join(lines), encoding="utf-8")
    (args.output / "NUM_APPLETS.h").write_text("#define NUM_APPLETS 1\n", encoding="utf-8")
    ash = (args.source / "shell/ash.c").read_text(encoding="utf-8")
    printf = (args.source / "coreutils/printf.c").read_text(encoding="utf-8")
    # Successful stdio output can change errno. Keep numeric conversion
    # failures separate from libc's internal descriptor probes and writes.
    printf = replace_once(printf, "\tchar *have_prec, *have_width;",
                          "\tchar *have_prec, *have_width;\n\tint conversion_error = 0;")
    for conversion in ["llv = my_xstrtoll(skip_whitespace(argument));",
                       "llv = my_xstrtoull(skip_whitespace(argument));",
                       "dv = my_xstrtod(argument);"]:
        printf = replace_once(printf, conversion, conversion + "\n\t\tconversion_error = errno;")
    printf = replace_once(printf, "\tformat[fmt_length] = saved;",
                          "\tformat[fmt_length] = saved;\n\terrno = conversion_error;")
    (args.output / "printf.c").write_text(printf, encoding="utf-8")
    test = (args.source / "coreutils/test.c").read_text(encoding="utf-8")
    # Asyncify longjmp first returns while unwinding saved frames. Keep the
    # error helper epilogue, as for the ash translation unit below.
    test = test.replace("NORETURN", "NOINLINE")
    test = replace_once(test, 'bb_error_msg("%s: %s"+4, msg);', 'bb_error_msg("%s", msg);')
    (args.output / "test.c").write_text(test, encoding="utf-8")
    math = (args.source / "shell/math.c").read_text(encoding="utf-8")
    # Upstream suppresses ternary branches, but evaluates both sides of &&/||.
    # Each logical operator retains the enclosing suppression state until its
    # right operand has been parsed; restore it before combining the values.
    math = replace_once(math, "operator *opstack, *opstackptr;",
                        "operator *opstack, *opstackptr;\n    unsigned *logical_disabled;")
    marker = "opstackptr = opstack = alloca(expr_len * sizeof(opstack[0]));"
    math = replace_once(math, marker, marker + "\n        logical_disabled = alloca(expr_len * sizeof(logical_disabled[0]));")
    marker = "errmsg = arith_apply(math_state, prev_op, numstack, &numstackptr);"
    math = replace_once(math, marker, '''if (prev_op == TOK_AND || prev_op == TOK_OR)
                    math_state->evaluation_disabled = logical_disabled[opstackptr - opstack];
                ''' + marker)
    marker = "*opstackptr++ = lasttok = op;"
    math = replace_once(math, marker, '''if (op == TOK_AND || op == TOK_OR) {
            logical_disabled[opstackptr - opstack] = math_state->evaluation_disabled;
            if (!math_state->evaluation_disabled &&
                ((op == TOK_AND && numstackptr[-1].val == 0) ||
                 (op == TOK_OR && numstackptr[-1].val != 0)))
                math_state->evaluation_disabled = UINT_MAX;
        }
        ''' + marker)
    (args.output / "math.c").write_text(math, encoding="utf-8")
    # Error helpers return during an Asyncify unwind. They must not carry
    # C noreturn promises, which would let LLVM discard the unwind epilogue.
    ash = ash.replace("NORETURN", "NOINLINE")
    ash = ash.replace("execve(cmd, argv, envp);", "efi_ash_exec(cmd, argv, envp);")
    # Expanded here-documents already live in the current shell's stack.
    # Snapshot them as a read-only stream; no pipe writer or fork is needed.
    start = ash.index("static int\nopenhere(union node *redir)\n{")
    end = ash.index("\nstatic int\nopenredirect(", start)
    ash = ash[:start] + '''static int
openhere(union node *redir)
{
    char *text = redir->nhere.doc->narg.text;
    if (redir->type == NXHERE) {
        expandhere(redir->nhere.doc);
        text = stackblock();
    }
    int descriptor = efi_ash_input_snapshot(text, strlen(text));
    if (descriptor < 0)
        ash_msg_and_raise_perror("cannot create here-document input");
    return descriptor;
}
''' + ash[end:]
    # Saved parser/redirection descriptors must not enter fresh tasks.
    ash, duplicates = re.subn(r"fcntl\(([^,\n]+), F_DUPFD(_CLOEXEC)?, ([^)\n]+)\)",
                             lambda match: f"efi_ash_dupfd{'_cloexec' if match[2] else ''}({match[1]}, {match[3]})", ash)
    if duplicates != 5:
        raise ValueError("BusyBox descriptor duplication call sites changed")
    marker = "#if BASH_READ_D\n\t\tcase 'd':\n\t\t\tparams.opt_d = optionarg;\n\t\t\tbreak;\n#endif"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash read delimiter marker changed")
    ash = ash.replace(marker, "\t\tcase 'd':\n\t\t\tparams.opt_d = optionarg;\n\t\t\tbreak;")
    # Registered programs use the cooperative process ABI directly. This keeps
    # ordinary command execution in the parent and avoids cloning shell memory.
    start = ash.index("\tdefault: {\n", ash.index("\nevalcommand(union node *cmd, int flags)"))
    end = ash.index("\tcase CMDBUILTIN:", start)
    ash = ash[:start] + '''\tdefault: {
        char **environment = listvars(VEXPORT, VUNSET, varlist.list, NULL);
        int command_status = efi_ash_run(argv, environment);
        if (command_status < 0) {
            command_status = errno == ENOENT ? 127 : 126;
            ash_msg("%s: %s", argv[0], strerror(errno));
        }
        exitstatus = command_status;
        break;
    }
''' + ash[end:]
    # Builtins and functions retain upstream precedence. The registry query is
    # live and uncached, so loading a program after a shell starts is visible.
    marker = "\t/* We have to search path. */"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash command lookup marker changed")
    ash = ash.replace(marker, '''    if (efi_process_program_exists(name, strlen(name)) == 1) {
        entry->cmdtype = CMDNORMAL;
        entry->u.index = -1;
        return;
    }
''' + marker)
    # The process ABI requires unique environment names. Compare names rather
    # than NAME=value strings, and discard earlier command-local assignments.
    marker = "if (strcmp(lp1->text, vp->var_text) == 0)"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash environment overlay marker changed")
    ash = ash.replace(marker, "if (varcmp(lp1->text, vp->var_text) == 0)")
    marker = "\twhile (lp) {\n\t\tif (ep == stackstrend())"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash temporary environment marker changed")
    ash = ash.replace(marker, '''\twhile (lp) {
        struct strlist *later = lp->next;
        while (later && varcmp(lp->text, later->text) != 0)
            later = later->next;
        if (later) {
            lp = lp->next;
            continue;
        }
\t\tif (ep == stackstrend())''')
    # These values are used only while unwinding. Asyncify skips the caller's
    # remaining work until its driver regains control.
    returns = {
        'ash_msg_and_raise_error(err_msg, name);': 'return NULL;',
        'ash_msg_and_raise_error("can\'t open %s: %s", fname, errmsg(errno, "no such file"));': 'return -1;',
        'ash_msg_and_raise_error("%s: not found", basename);': 'return NULL;',
        'raise_exception(EXEXIT);': 'return 0;',
        'TRACE(("End of main reached\\n"));\n\texitshell();': 'return 0;',
    }
    for marker, returned in returns.items():
        if ash.count(marker) != 1:
            raise ValueError(f"Upstream ash return marker changed: {marker}")
        ash = ash.replace(marker, marker + "\n\t" + returned)
    start = ash.index("static int\nforkshell(struct job *jp, union node *n, int mode)\n{")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static int
forkshell(struct job *jp, union node *n, int mode)
{
    int pid = efi_ash_spawn_node(n, mode);
    if (pid < 0) {
        if (jp)
            freejob(jp);
        ash_msg_and_raise_error("cannot start shell task: %d", pid);
    }
    forkparent(jp, n, mode, pid);
    return pid;
}
''' + ash[end:]
    replacements = {
        'static void\nsetsignal(int signo)\n{': '''static void
setsignal(int signo)
{
    if (trap[signo] != NULL)
        ash_msg_and_raise_error("asynchronous signals are unavailable");
}
''',
        'static void\nraise_interrupt(void)\n{': '''static void
raise_interrupt(void)
{
    pending_int = 0;
    exitstatus = 130;
    raise_exception(EXINT);
}
''',
        'static int\nwaitproc(int block, int *status)\n{': '''static int
waitproc(int block, int *status)
{
    for (;;) {
        int active = 0;
        for (unsigned index = 0; index < njobs; ++index) {
            struct job *job = &jobtab[index];
            if (!job->used)
                continue;
            for (unsigned process = 0; process < job->nprocs; ++process) {
                struct procstat *entry = &job->ps[process];
                if (entry->ps_status != -1)
                    continue;
                active = 1;
                int32_t code;
                int32_t result = efi_process_waitpid_nohang(entry->ps_pid, &code);
                if (result < 0)
                    __builtin_trap();
                if (result > 0) {
                    *status = ((uint32_t)code & 255) << 8;
                    return result;
                }
            }
        }
        if (!active) {
            errno = ECHILD;
            return -1;
        }
        if (!block)
            return 0;
    }
}
''',
        'static int FAST_FUNC\ntimescmd(int argc UNUSED_PARAM, char **argv UNUSED_PARAM)\n{': '''static int FAST_FUNC
timescmd(int argc UNUSED_PARAM, char **argv UNUSED_PARAM)
{
    ash_msg_and_raise_error("process CPU clocks are unavailable");
    return 2;
}
''',
    }
    for marker, replacement in replacements.items():
        start = ash.index(marker)
        end = ash.index("\n}\n", start) + 3
        ash = ash[:start] + replacement + ash[end:]
    start = ash.index("static int\nevalpipe(union node *n, int flags)\n{")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static int
evalpipe(union node *n, int flags)
{
    int count = 0;
    for (struct nodelist *entry = n->npipe.cmdlist; entry; entry = entry->next)
        ++count;
    INT_OFF;
    struct job *job = makejob(count);
    int input = 0;
    for (struct nodelist *entry = n->npipe.cmdlist; entry; entry = entry->next) {
        int descriptors[2] = {-1, -1};
        if (entry->next && pipe(descriptors) < 0)
            ash_msg_and_raise_perror("cannot create shell pipe");
        int output = entry->next ? descriptors[1] : 1;
        int pid = efi_ash_spawn_task(entry->n, input, output,
            (flags & EV_TESTED) ? EFI_TASK_TESTED : 0);
        if (input != 0)
            close(input);
        if (entry->next)
            close(output);
        if (pid < 0) {
            if (entry->next)
                close(descriptors[0]);
            ash_msg_and_raise_error("cannot start pipeline task: %d", pid);
        }
        forkparent(job, entry->n, n->npipe.pipe_backgnd, pid);
        input = descriptors[0];
    }
    int status = n->npipe.pipe_backgnd ? 0 : waitforjob(job);
    INT_ON;
    return status;
}
''' + ash[end:]
    start = ash.index("static void FAST_FUNC\nevalbackcmd(union node *n, struct backcmd *result")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static void FAST_FUNC
evalbackcmd(union node *node, struct backcmd *result)
{
    result->fd = -1;
    result->buf = NULL;
    result->nleft = 0;
    result->jp = NULL;
    if (!node)
        return;
    int descriptors[2];
    if (pipe(descriptors) < 0)
        ash_msg_and_raise_perror("cannot create command substitution pipe");
    struct job *job = makejob(1);
    int pid = efi_ash_spawn_task(node, 0, descriptors[1], EFI_TASK_CLEAR_ERREXIT);
    close(descriptors[1]);
    if (pid < 0) {
        close(descriptors[0]);
        freejob(job);
        ash_msg_and_raise_error("cannot start command substitution: %d", pid);
    }
    forkparent(job, node, FORK_NOJOB, pid);
    result->fd = descriptors[0];
    result->jp = job;
}
''' + ash[end:]
    start = ash.index("static int\nevalsubshell(union node *n, int flags)\n{")
    end = ash.index("\n}\n", start) + 3
    ash = ash[:start] + '''static int
evalsubshell(union node *node, int flags)
{
    int background = node->type == NBACKGND;
    errlinno = lineno = node->nredir.linno;
    expredir(node->nredir.redirect);
    if (!background && (flags & EV_EXIT) && !may_have_traps) {
        redirect(node->nredir.redirect, 0);
        evaltreenr(node->nredir.n, flags);
    }
    INT_OFF;
    struct job *job = makejob(1);
    unsigned task_flags = EFI_TASK_REDIR_EXPANDED;
    if (!background && (flags & EV_TESTED))
        task_flags |= EFI_TASK_TESTED;
    int pid = efi_ash_spawn_task(node, 0, 1, task_flags);
    if (pid < 0) {
        freejob(job);
        ash_msg_and_raise_error("cannot start subshell task: %d", pid);
    }
    forkparent(job, node, background, pid);
    int status = background ? 0 : waitforjob(job);
    INT_ON;
    return status;
}
''' + ash[end:]
    ash = ash.replace("\tif (iflag)\n\t\tsignal(SIGHUP, SIG_DFL);",
                      "\t/* No asynchronous signal state exists in the guest. */")
    marker = "/*\n * Main routine.  We initialize things"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash task include marker changed")
    ash = ash.replace(marker, '#include "tasks.inc"\n\n' + marker)
    marker = "\tlogin_sh = procargs(argv);"
    if ash.count(marker) != 1:
        raise ValueError("Upstream ash startup marker changed")
    ash = ash.replace(marker, '''    if (argv[1] && strcmp(argv[1], "--efi-task") == 0) {
        arg0 = argv[0];
        if (!argv[2] || !argv[3] || argv[4])
            ash_msg_and_raise_error("shell task requires size and descriptor arguments");
        efi_ash_task_entry(argv[2], argv[3]);
    }
''' + marker)
    (args.output / "ash.c").write_text(ash, encoding="utf-8")
    prepare_read(args.source, args.output)
    (args.output / "LICENSE").write_bytes((args.source / "LICENSE").read_bytes())


if __name__ == "__main__":
    main()
