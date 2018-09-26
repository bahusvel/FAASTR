FORCE:

build/symbind: symbind/*.go
	cd symbind && go build -o ../$@

ivshrpcd: FORCE
	cd ivshrpcd && cargo run