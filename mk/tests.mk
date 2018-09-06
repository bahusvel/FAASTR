FORCE:

TEST_PREFIX = build/tests

$(TEST_PREFIX): FORCE
	mkdir -p $@
	make $(TEST_PREFIX)/exit
	make $(TEST_PREFIX)/call

$(TEST_PREFIX)/%: test/%.c build/symbind
	gcc -nostdlib $< -o $@
	build/symbind -m $@
