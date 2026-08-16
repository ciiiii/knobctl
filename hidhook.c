// DYLD interposer: logs every hid_write / hid_send_feature_report the vendor
// config app sends, plus which device path it opened, then forwards to the real call.
// Build:  clang -dynamiclib -o hidhook.dylib hidhook.c -undefined dynamic_lookup
// Run:    DYLD_INSERT_LIBRARIES=$PWD/hidhook.dylib \
//           /path/to/Vendor.app/Contents/MacOS/Vendor
// Log:    /tmp/hidhook.log

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <stdint.h>

// hidapi prototypes (opaque device handle).
extern int  hid_write(void *dev, const unsigned char *data, size_t length);
extern int  hid_send_feature_report(void *dev, const unsigned char *data, size_t length);
extern int  hid_read(void *dev, unsigned char *data, size_t length);
extern int  hid_read_timeout(void *dev, unsigned char *data, size_t length, int ms);
extern int  hid_get_feature_report(void *dev, unsigned char *data, size_t length);
extern void *hid_open_path(const char *path);

#define LOGPATH "/tmp/hidhook.log"

static void logline(const char *tag, const unsigned char *data, size_t len) {
    char buf[8192];
    int n = snprintf(buf, sizeof buf, "%s len=%zu:", tag, len);
    for (size_t i = 0; i < len && n < (int)sizeof buf - 4; i++)
        n += snprintf(buf + n, sizeof buf - n, " %02x", data[i]);
    n += snprintf(buf + n, sizeof buf - n, "\n");
    int fd = open(LOGPATH, O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (fd >= 0) { (void)write(fd, buf, n); close(fd); }
}

static void logmsg(const char *msg) {
    int fd = open(LOGPATH, O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (fd >= 0) { (void)write(fd, msg, strlen(msg)); close(fd); }
}

static int my_hid_write(void *dev, const unsigned char *data, size_t length) {
    logline("WRITE", data, length);
    return hid_write(dev, data, length);
}

static int my_hid_send_feature_report(void *dev, const unsigned char *data, size_t length) {
    logline("FEATURE", data, length);
    return hid_send_feature_report(dev, data, length);
}

static int my_hid_read(void *dev, unsigned char *data, size_t length) {
    int n = hid_read(dev, data, length);
    if (n > 0) logline("READ", data, (size_t)n);
    return n;
}

static int my_hid_read_timeout(void *dev, unsigned char *data, size_t length, int ms) {
    int n = hid_read_timeout(dev, data, length, ms);
    if (n > 0) logline("READ", data, (size_t)n);
    return n;
}

static int my_hid_get_feature_report(void *dev, unsigned char *data, size_t length) {
    int n = hid_get_feature_report(dev, data, length);
    if (n > 0) logline("GETFEATURE", data, (size_t)n);
    return n;
}

static void *my_hid_open_path(const char *path) {
    char m[1024];
    snprintf(m, sizeof m, "OPEN path=%s\n", path ? path : "(null)");
    logmsg(m);
    return hid_open_path(path);
}

__attribute__((used)) static struct { const void *r, *o; }
_i_write    __attribute__((section("__DATA,__interpose"))) = { (const void*)&my_hid_write, (const void*)&hid_write },
_i_feature  __attribute__((section("__DATA,__interpose"))) = { (const void*)&my_hid_send_feature_report, (const void*)&hid_send_feature_report },
_i_read     __attribute__((section("__DATA,__interpose"))) = { (const void*)&my_hid_read, (const void*)&hid_read },
_i_readto   __attribute__((section("__DATA,__interpose"))) = { (const void*)&my_hid_read_timeout, (const void*)&hid_read_timeout },
_i_getfeat  __attribute__((section("__DATA,__interpose"))) = { (const void*)&my_hid_get_feature_report, (const void*)&hid_get_feature_report },
_i_open     __attribute__((section("__DATA,__interpose"))) = { (const void*)&my_hid_open_path, (const void*)&hid_open_path };
