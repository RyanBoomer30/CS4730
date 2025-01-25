FROM ubuntu:22.04

# Install what we need to build C code
RUN apt-get update && apt-get install -y gcc

# Create a directory for the app
WORKDIR /app

# Copy the hosts file and the C source file
COPY hostsfile.txt /app/hostsfile.txt
COPY peer.c /app/peer.c

# Compile the program
RUN gcc /app/peer.c -o /app/udp_server

# Set the entrypoint to our compiled binary
# Docker will run "/app/udp_server" plus any extra arguments from the 'command:' line
ENTRYPOINT ["/app/udp_server"]