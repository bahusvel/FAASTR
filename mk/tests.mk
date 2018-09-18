FORCE:

TEST_PREFIX = build/tests

$(TEST_PREFIX): FORCE
	mkdir -p $@
	make $(TEST_PREFIX)/exit
	make $(TEST_PREFIX)/call

$(TEST_PREFIX)/%: test/%.c build/symbind FORCE
	make -C libc libc.a
	gcc -fno-builtin -nostdinc -nostdlib -fno-stack-protector -Ilibc/include -c $< -o $@.o
	ld -e 0x0 -o $@ $@.o libc/*.o
	build/symbind -m $@
