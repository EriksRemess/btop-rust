/* APIs used by Rust's std but absent from Snow Leopard's libSystem.
 * These are linked into btoprs only; no system libraries are replaced. */
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <mach/mach_time.h>
#include <math.h>
#include <pthread.h>
#include <stdint.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

static pthread_once_t timebase_once = PTHREAD_ONCE_INIT;
static mach_timebase_info_data_t timebase;
static void init_timebase(void) { mach_timebase_info(&timebase); }

int clock_gettime(unsigned int clock, struct timespec *value)
{
    if (clock == 0) { /* Darwin CLOCK_REALTIME */
        struct timeval now;
        if (gettimeofday(&now, NULL) < 0) return -1;
        value->tv_sec = now.tv_sec; value->tv_nsec = now.tv_usec * 1000;
        return 0;
    }
    if (clock == 8) { /* Darwin CLOCK_UPTIME_RAW: exactly mach_absolute_time */
        pthread_once(&timebase_once, init_timebase);
        if (!timebase.denom) { errno = EIO; return -1; }
        uint64_t ticks = mach_absolute_time();
        uint64_t ns = (ticks / timebase.denom) * timebase.numer +
            (ticks % timebase.denom) * timebase.numer / timebase.denom;
        value->tv_sec = ns / 1000000000; value->tv_nsec = ns % 1000000000;
        return 0;
    }
    errno = EINVAL; return -1;
}

int CCRandomGenerateBytes(void *buffer, size_t length)
{
    int fd = open("/dev/urandom", O_RDONLY);
    if (fd < 0) return -1;
    unsigned char *bytes = buffer;
    while (length) {
        size_t amount = length > SSIZE_MAX ? SSIZE_MAX : length;
        ssize_t received = read(fd, bytes, amount);
        if (received < 0 && errno == EINTR) continue;
        if (received <= 0) { close(fd); return -1; }
        bytes += received; length -= received;
    }
    close(fd); return 0;
}

/* The SDK exposes this as a macro over its public DIR layout. */
#undef dirfd
int dirfd(DIR *directory) { return directory->__dd_fd; }

/* Preserve the Darwin two-double return ABI without the newer libm helper.
 * Compile with -fno-builtin-sin/-cos so this cannot fold back into itself. */
struct sine_cosine { double sine, cosine; };
struct sine_cosine __sincos_stret(double angle)
{
    struct sine_cosine value = {sin(angle), cos(angle)};
    return value;
}

/* std::fs::hard_link uses linkat with both paths relative to the current
 * directory and flags=0. This is exactly Snow Leopard's link operation.
 * Other *at variants are deliberately unsupported, not path-emulated. */
int linkat(int source_fd, const char *source, int dest_fd, const char *dest, int flags)
{
    if (source_fd != -2 || dest_fd != -2 || flags != 0) { errno = ENOSYS; return -1; }
    return link(source, dest);
}

extern int proc_listpids(uint32_t type, uint32_t info, void *buffer, int size);
int proc_listallpids(void *buffer, int size)
{
    int result = proc_listpids(1, 0, buffer, size);
    return result > 0 ? result / (int)sizeof(int) : result;
}
