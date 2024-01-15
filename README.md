# `max_rss`

A small utility to measure resident set size (rss) of a process.

I created this because the I wanted to track the memory usage of programs in https://github.com/acheronfail/count, but the `max_rss` value from Linux's `getrusage` is inaccurate.

## How does it work?

It uses Linux's ptrace api (`man 2 ptrace`) and tracks when the process forks, clones or exits, and sums up the Resident Set Size from each process where appropriate.

If you go through various Linux man pages, you'll discover that the `max_rss` field from `getrusage` isn't accurate, and also that `man 5 proc` mentions its `rss` field and some others are inaccurate. It recommends reading `/proc/$PID/smaps` instead.

Hence the need for this program. Here are also some other people I've found encountering the same thing:

- https://jkz.wtf/random-linux-oddity-1-ru_maxrss
- https://tbrindus.ca/sometimes-the-kernel-lies-about-process-memory-usage/
- https://github.com/ziglang/gotta-go-fast/issues/23
- https://github.com/golang/go/issues/32054
