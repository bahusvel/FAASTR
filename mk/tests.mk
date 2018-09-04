FORCE:

TEST_PREFIX = build/tests

$(TEST_PREFIX): FORCE
	mkdir -p $@
	make build/tests/exit

$(TEST_PREFIX)/exit: test/exit.c build/symbind
	gcc -nostdlib $< -o $@
	build/symbind -m $@
