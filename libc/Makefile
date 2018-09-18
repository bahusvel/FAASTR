CFLAGS = -Iinclude

SRC=$(wildcard *.c)
OBJS=$(SRC:.c=.ll)

%.ll: %.c
	clang -fpic -fno-builtin -nostdinc -Iinclude -S -emit-llvm -fno-stack-protector -g -o $@ $<

%.o: %.c
	clang -fpic -fno-builtin -nostdinc -nostdlib -Iinclude -fno-stack-protector -g -o $@ -c $<

libc.a: sos.o string.o
	ar rcs $@ $^

clean:
	rm *.ll || true
	rm *.o || true
	rm *.bc || true

link: $(OBJS)
	llvm-link $^ -o stdlib.bc


compile: clean link
