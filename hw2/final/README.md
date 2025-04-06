# Running

The program runs exactly as indicated in the project description.

- `docker build . -t prj2` to build the image

# Errors

I have added some error checkers for trivial program states:

- Error thrown if the hostsname are not found or arguments are being used incorrectly
- Error thrown if file parser fail
- Error thrown when at any moment the TCP connection is lost between peers, or if a token/marker message can't be sent. This ensure that all communications are guaranteed between peer