FROM ubuntu:22.04

RUN apt-get update
RUN apt-get install -y gcc

WORKDIR /app
COPY udp_server /usr/local/bin/udp_server
COPY hostsfile.txt /app/hostsfile.txt
ENTRYPOINT ["udp_server", "/app/hostsfile.txt"]