# `max_rss`

A small utility to measure resident set size (rss) of a process.

It uses Linux's ptrace api (`man 2 ptrace`) and tracks when the process forks, clones or exits, and sums up the Resident Set Size from each process where appropriate.

I created this because the `max_rss` value from Linux's `getrusage` is inaccurate, and I wanted a more accurate way of measuring a given process' rss size.

## Why is `getrusage`'s `max_rss` inaccurate?

If you read `man 5 proc` you'll see that it mentions the `rss` field and some others are inaccurate, and then it recommends reading `/proc/$PID/smaps` instead. Also, these links have some deeper dives and investigations, too:

- https://jkz.wtf/random-linux-oddity-1-ru_maxrss
- https://tbrindus.ca/sometimes-the-kernel-lies-about-process-memory-usage/
- https://github.com/ziglang/gotta-go-fast/issues/23
- https://github.com/golang/go/issues/32054
