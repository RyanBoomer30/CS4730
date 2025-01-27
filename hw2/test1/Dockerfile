FROM ubuntu:22.04

# Install what we need to build C code
RUN apt-get update && apt-get install -y gcc

# Set the working directory for the app
WORKDIR /app

# Copy the hosts file and the C source file
COPY timeout.h /app/timeout.h
COPY peer.c /app/peer.c
COPY hostsfile.txt /app/hostsfile.txt

# Compile the program
RUN gcc /app/peer.c -o /app/peer

# Set the entrypoint to our compiled binary
ENTRYPOINT ["/app/peer"]