This is entire standard library provided by FaaSTR, all of it is implemented as untrusted guest code. All code here must obey by the following rules:
* There may not be any system calls.
* There may not be any use of global variables except for constants.
* There may not be any external dependencies.
