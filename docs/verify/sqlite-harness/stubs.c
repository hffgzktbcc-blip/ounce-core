/* The Switch libc has no file ownership calls; SQLite's unix VFS references them but an
   in-memory database never reaches them. */
#include <sys/types.h>
int fchown(int fd, uid_t u, gid_t g){(void)fd;(void)u;(void)g;return 0;}
uid_t geteuid(void){return 0;}
