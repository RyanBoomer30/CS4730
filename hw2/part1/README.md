# Running

The program runs exactly as indicated in the project description.

- `docker build . -t prj2` to build the image

# Errors

I have added some error checkers naturally as we are working in C:

- Error thrown if the hostsname are not found or arguments are being used incorrectly
- Error thrown if the IP from getaddrinfo is invalid
- Error thrown if the given peer name is not in the hostsfile
- Error thrown if the message received from another peer is invalid (not ping or pong)
- Error thrown if TCP connections fail