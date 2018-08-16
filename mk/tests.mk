FORCE:

test/exit: FORCE
	gcc -nostdlib $@.c -o $@
