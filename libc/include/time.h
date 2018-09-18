#ifndef TIME_H
#define TIME_H 1

typedef unsigned long duration_t;

const duration_t Nanosecond = 1;
const duration_t Microsecond = 1000 * Nanosecond;
const duration_t Millisecond = 1000 * Microsecond;
const duration_t Second = 1000 * Millisecond;
const duration_t Minute = 60 * Second;
const duration_t Hour = 60 * Minute;

#endif
