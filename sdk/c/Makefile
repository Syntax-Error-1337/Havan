# The C component, built and tested on a host.
#
# ESP-IDF uses CMakeLists.txt (below) and cross-compiles with xtensa-esp32-elf-gcc; this
# Makefile exists so the component can be tested on a laptop with clang and `make test`, which
# is how it was verified before any board existed.

CC ?= cc
CFLAGS ?= -std=c11 -Wall -Wextra -Werror -pedantic -O2
HEADERS := haven.h

.PHONY: all test clean

all: test

haven.o: haven.c $(HEADERS)
	$(CC) $(CFLAGS) -c haven.c -o haven.o

tests/test_haven: tests/test_haven.c haven.c $(HEADERS)
	$(CC) $(CFLAGS) tests/test_haven.c haven.c -o tests/test_haven

test: tests/test_haven
	@./tests/test_haven

clean:
	rm -f haven.o tests/test_haven
