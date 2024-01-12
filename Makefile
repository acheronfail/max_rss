# Makefile for these samples.
#
# Eli Bendersky [http://eli.thegreenplace.net]
# This code is in the public domain.
CC = gcc
CCFLAGS = -std=gnu99 -Wall -O0 -no-pie

LDFLAGS = -L. -ldebug

EXECUTABLES = \
	max_rss \
	sample

ADDR = `objdump -d sample | grep do_stuff | head -n 1 | awk '{print $1;}'`

.PHONY: all clean test_max_rss

all: $(EXECUTABLES)

libdebug.a: debuglib.c debuglib.h
	$(CC) $(CCFLAGS) -O -c debuglib.c
	ar rcs libdebug.a debuglib.o

max_rss: max_rss.c libdebug.a
	$(CC) $(CCFLAGS) $< -o $@ $(LDFLAGS)

test_max_rss: max_rss sample
	./max_rss sample 0x$(ADDR)

sample: sample.c
	$(CC) $(CCFLAGS) $^ -o $@

clean:
	rm -f $(EXECUTABLES) *.o *.a