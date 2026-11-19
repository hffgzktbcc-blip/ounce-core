/* elf-workload calendar: civil-date arithmetic with no clock and no time zone. Own days-from-civil
 * and civil-from-days conversions (checked against libc timegm/gmtime_r for 1900..2200), leap years,
 * weekdays, ISO week numbers, strftime formats on fixed instants and a month-grid printer.
 * Provenance: original work written for ounce-core (MIT); the day-count algorithm is the standard
 * proleptic-Gregorian civil-calendar derivation. */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

static int is_leap(int y) { return (y % 4 == 0 && y % 100 != 0) || y % 400 == 0; }
static int mdays(int y, int m) { static const int d[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31}; return m == 2 && is_leap(y) ? 29 : d[m - 1]; }

static int64_t days_from_civil(int64_t y, unsigned m, unsigned d) {
    y -= m <= 2;
    int64_t era = (y >= 0 ? y : y - 399) / 400;
    unsigned yoe = (unsigned)(y - era * 400);
    unsigned doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    unsigned doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + (int64_t)doe - 719468;
}
static void civil_from_days(int64_t z, int *y, unsigned *m, unsigned *d) {
    z += 719468;
    int64_t era = (z >= 0 ? z : z - 146096) / 146097;
    unsigned doe = (unsigned)(z - era * 146097);
    unsigned yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    int64_t yy = (int64_t)yoe + era * 400;
    unsigned doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    unsigned mp = (5 * doy + 2) / 153;
    *d = doy - (153 * mp + 2) / 5 + 1;
    *m = mp < 10 ? mp + 3 : mp - 9;
    *y = (int)(yy + (*m <= 2));
}
static int weekday(int64_t days) { return (int)(((days % 7) + 11) % 7); } /* 0 = Sunday; 1970-01-01 was a Thursday */

static int iso_week(int y, int m, int d, int *iso_year) {
    int64_t dn = days_from_civil(y, (unsigned)m, (unsigned)d);
    int wd = (weekday(dn) + 6) % 7;                 /* Monday = 0 */
    int64_t thursday = dn - wd + 3;
    int ty; unsigned tm, td;
    civil_from_days(thursday, &ty, &tm, &td);
    *iso_year = ty;
    return (int)((thursday - days_from_civil(ty, 1, 1)) / 7) + 1;
}

int workload_run(FILE *o) {
    int fails = 0;
    fprintf(o, "days(1970-01-01)=%lld days(2000-03-01)=%lld days(1900-01-01)=%lld days(2100-12-31)=%lld\n",
            (long long)days_from_civil(1970, 1, 1), (long long)days_from_civil(2000, 3, 1),
            (long long)days_from_civil(1900, 1, 1), (long long)days_from_civil(2100, 12, 31));
    if (days_from_civil(1970, 1, 1) != 0 || days_from_civil(2000, 3, 1) != 11017) fails++;

    /* round trip and libc agreement for every day in 1900..2200 */
    long mismatches = 0, rt_bad = 0, days_checked = 0;
    for (int y = 1900; y <= 2200; y++) for (int m = 1; m <= 12; m++) for (int d = 1; d <= mdays(y, m); d++) {
        int64_t dn = days_from_civil(y, (unsigned)m, (unsigned)d);
        int ry; unsigned rm, rd;
        civil_from_days(dn, &ry, &rm, &rd);
        if (ry != y || (int)rm != m || (int)rd != d) rt_bad++;
        struct tm tmv;
        memset(&tmv, 0, sizeof tmv);
        tmv.tm_year = y - 1900; tmv.tm_mon = m - 1; tmv.tm_mday = d;
        time_t t = timegm(&tmv);
        if ((int64_t)t != dn * 86400 || tmv.tm_wday != weekday(dn)) mismatches++;
        days_checked++;
    }
    fprintf(o, "checked %ld days: round-trip failures=%ld libc mismatches=%ld\n", days_checked, rt_bad, mismatches);
    if (rt_bad || mismatches) fails++;

    int leaps = 0;
    for (int y = 1900; y <= 2100; y++) leaps += is_leap(y);
    fprintf(o, "leap years 1900..2100: %d (1900:%d 2000:%d 2024:%d 2100:%d)\n", leaps, is_leap(1900), is_leap(2000), is_leap(2024), is_leap(2100));
    if (leaps != 49) fails++;

    static const int dates[][3] = {{2020, 12, 31}, {2021, 1, 1}, {2021, 1, 4}, {2026, 11, 18}, {2024, 2, 29}, {2015, 12, 31}, {2016, 1, 3}};
    static const char *dn_[] = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
    for (unsigned i = 0; i < sizeof dates / sizeof *dates; i++) {
        int iy, w = iso_week(dates[i][0], dates[i][1], dates[i][2], &iy);
        int64_t dn = days_from_civil(dates[i][0], (unsigned)dates[i][1], (unsigned)dates[i][2]);
        fprintf(o, "%04d-%02d-%02d %s ISO %d-W%02d\n", dates[i][0], dates[i][1], dates[i][2], dn_[weekday(dn)], iy, w);
    }

    /* strftime on fixed instants (UTC only) */
    static const time_t instants[] = {0, 951782400, 1000000000, 1234567890, 1700000000, 2147483647, 4102444800LL};
    for (unsigned i = 0; i < sizeof instants / sizeof *instants; i++) {
        struct tm tmv;
        char buf[96];
        gmtime_r(&instants[i], &tmv);
        strftime(buf, sizeof buf, "%Y-%m-%d %H:%M:%S %a %b %j %u %U", &tmv);
        fprintf(o, "%11lld -> %s\n", (long long)instants[i], buf);
    }

    /* a month grid */
    int y = 2026, m = 11;
    int first = weekday(days_from_civil(y, (unsigned)m, 1));
    fprintf(o, "   November %d\nSu Mo Tu We Th Fr Sa\n", y);
    for (int i = 0; i < first; i++) fputs("   ", o);
    for (int d = 1; d <= mdays(y, m); d++) {
        fprintf(o, "%2d%c", d, (first + d) % 7 == 0 || d == mdays(y, m) ? '\n' : ' ');
    }

    /* Friday the 13ths in 2026 and total of them in a 400 year cycle (688) */
    int f13 = 0;
    for (int yy = 2000; yy < 2400; yy++) for (int mm = 1; mm <= 12; mm++)
        f13 += weekday(days_from_civil(yy, (unsigned)mm, 13)) == 5;
    fprintf(o, "Friday the 13ths in 400 years: %d\n", f13);
    if (f13 != 688) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
