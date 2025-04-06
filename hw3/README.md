# Running

The program runs exactly as indicated in the project description.

- `docker build . -t prj3` to build the image

This implementation does not work for leader crash at the moment. To view this in Debug mode, uncomment all DEBUG prints

# Errors

All instances that lead to a process crashing print out an error. The order of error codes are ordered in most to least important
- leader does not respond with NEWVIEW during join or delete protocol
- leader crash
- protocol message is in wrong format
- any parsing error from hostsfile like duplicated users or empty user
