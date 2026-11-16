# The C component, built and tested on a host.
#
# ESP-IDF uses CMakeLists.txt (below) and cross-compiles with xtensa-esp32-elf-gcc; this
# Makefile exists so the component can be tested on a laptop with clang and `make test`, which
# is how it was verified before any board existed.

CC ?= cc
CFLAGS ?= -std=c11 -Wall -Wextra -Werror -pedantic -O2
HEADERS := haven.h haven_port.h haven_runtime.h

.PHONY: all test clean

all: test

haven.o: haven.c $(HEADERS)
	$(CC) $(CFLAGS) -c haven.c -o haven.o

haven_runtime.o: haven_runtime.c $(HEADERS)
	$(CC) $(CFLAGS) -c haven_runtime.c -o haven_runtime.o

tests/test_haven: tests/test_haven.c haven.c $(HEADERS)
	$(CC) $(CFLAGS) tests/test_haven.c haven.c -o tests/test_haven

tests/test_runtime: tests/test_runtime.c tests/fake_port.c haven_runtime.c haven.c $(HEADERS)
	$(CC) $(CFLAGS) tests/test_runtime.c tests/fake_port.c haven_runtime.c haven.c \
		-o tests/test_runtime

test: tests/test_haven tests/test_runtime
	@./tests/test_haven
	@./tests/test_runtime

clean:
	rm -f haven.o haven_runtime.o tests/test_haven tests/test_runtime
