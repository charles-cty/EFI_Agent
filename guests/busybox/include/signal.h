#ifndef EFI_ASH_SIGNAL_H
#define EFI_ASH_SIGNAL_H

/* Numbering used by ash's parser and trap table. Actual guest signal delivery
 * is not implemented; the port must reject unsupported signal operations.
 */
#include <__typedef_sigset_t.h>
#define NSIG 65
#define SIGHUP 1
#define SIGINT 2
#define SIGQUIT 3
#define SIGILL 4
#define SIGABRT 6
#define SIGKILL 9
#define SIGPIPE 13
#define SIGALRM 14
#define SIGTERM 15
#define SIGCHLD 17
#define SIGCONT 18
#define SIGSTOP 19
#define SIGTSTP 20
#define SIGTTIN 21
#define SIGTTOU 22
#define SIG_BLOCK 0
#define SIG_UNBLOCK 1
#define SIG_SETMASK 2
#define SIG_DFL ((void (*)(int))0)
#define SIG_IGN ((void (*)(int))1)
#define SIG_ERR ((void (*)(int))-1)
struct sigaction {
    void (*sa_handler)(int);
    sigset_t sa_mask;
    int sa_flags;
};
int sigaction(int number, const struct sigaction *action, struct sigaction *previous);
int sigfillset(sigset_t *mask);
int sigemptyset(sigset_t *mask);
int sigprocmask(int how, const sigset_t *mask, sigset_t *previous);
void (*signal(int number, void (*handler)(int)))(int);
int raise(int number);
int sigsuspend(const sigset_t *mask);
int kill(int pid, int number);
#endif
