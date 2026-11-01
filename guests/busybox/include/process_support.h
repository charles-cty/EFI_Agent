#ifndef EFI_BUSYBOX_PROCESS_SUPPORT_H
#define EFI_BUSYBOX_PROCESS_SUPPORT_H
#define COMM_LEN 256
typedef struct procps_status_t {
	DIR *dir;
	IF_FEATURE_SHOW_THREADS(DIR *task_dir;)
	uint8_t shift_pages_to_bytes;
	uint8_t shift_pages_to_kb;
/* Fields are set to 0/NULL if failed to determine (or not requested) */
	uint16_t argv_len;
	char *argv0;
	char *exe;
	IF_SELINUX(char *context;)
	IF_FEATURE_SHOW_THREADS(unsigned main_thread_pid;)
	/* Everything below must contain no ptrs to malloc'ed data:
	 * it is memset(0) for each process in procps_scan() */
	unsigned long vsz, rss; /* we round it to kbytes */
	unsigned long stime, utime;
	unsigned long start_time;
	unsigned pid;
	unsigned ppid;
	unsigned pgid;
	unsigned sid;
	unsigned uid;
	unsigned gid;
#if ENABLE_FEATURE_PS_ADDITIONAL_COLUMNS
	unsigned ruid;
	unsigned rgid;
	int niceness;
#endif
	unsigned tty_major,tty_minor;
#if ENABLE_FEATURE_TOPMEM
	struct smaprec smaps;
#endif
	char state[4];
	/* basename of executable in exec(2), read from /proc/N/stat
	 * (if executable is symlink or script, it is NOT replaced
	 * by link target or interpreter name) */
	char comm[COMM_LEN];
	/* user/group? - use passwd/group parsing functions */
#if ENABLE_FEATURE_TOP_SMP_PROCESS
	int last_seen_on_cpu;
#endif
} procps_status_t;
/* flag bits for procps_scan(xx, flags) calls */
enum {
	PSSCAN_PID      = 1 << 0,
	PSSCAN_PPID     = 1 << 1,
	PSSCAN_PGID     = 1 << 2,
	PSSCAN_SID      = 1 << 3,
	PSSCAN_UIDGID   = 1 << 4,
	PSSCAN_COMM     = 1 << 5,
	/* PSSCAN_CMD      = 1 << 6, - use read_cmdline instead */
	PSSCAN_ARGV0    = 1 << 7,
	PSSCAN_EXE      = 1 << 8,
	PSSCAN_STATE    = 1 << 9,
	PSSCAN_VSZ      = 1 << 10,
	PSSCAN_RSS      = 1 << 11,
	PSSCAN_STIME    = 1 << 12,
	PSSCAN_UTIME    = 1 << 13,
	PSSCAN_TTY      = 1 << 14,
	PSSCAN_SMAPS	= (1 << 15) * ENABLE_FEATURE_TOPMEM,
	/* NB: used by find_pid_by_name(). Any applet using it
	 * needs to be mentioned here. */
	PSSCAN_ARGVN    = (1 << 16) * (ENABLE_KILLALL
				|| ENABLE_PGREP || ENABLE_PKILL
				|| ENABLE_PIDOF
				|| ENABLE_SESTATUS
				),
	PSSCAN_CONTEXT  = (1 << 17) * ENABLE_SELINUX,
	PSSCAN_START_TIME = 1 << 18,
	PSSCAN_CPU      = (1 << 19) * ENABLE_FEATURE_TOP_SMP_PROCESS,
	PSSCAN_NICE     = (1 << 20) * ENABLE_FEATURE_PS_ADDITIONAL_COLUMNS,
	PSSCAN_RUIDGID  = (1 << 21) * ENABLE_FEATURE_PS_ADDITIONAL_COLUMNS,
	PSSCAN_TASKS	= (1 << 22) * ENABLE_FEATURE_SHOW_THREADS,
};
//procps_status_t* alloc_procps_scan(void) FAST_FUNC;
void free_procps_scan(procps_status_t* sp) FAST_FUNC;
procps_status_t* procps_scan(procps_status_t* sp, int flags) FAST_FUNC;
/* Format cmdline (up to col chars) into char buf[size] */
/* Puts [comm] if cmdline is empty (-> process is a kernel thread) */
void read_cmdline(char *buf, int size, unsigned pid, const char *comm) FAST_FUNC;

pid_t getsid(pid_t pid);
pid_t *find_pid_by_name(const char *name);
char *smart_ulltoa4(unsigned long long value, char *buffer, const char *suffixes);
void print_signames(void);
#endif
