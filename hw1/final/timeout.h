#ifndef TIMEOUT_H
#define TIMEOUT_H

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <netinet/in.h>
#include <netdb.h>

// Constants
#define PORT "8888"
#define RECV_TIMEOUT_SEC 1
#define MAX_PEERS 100

// Function prototype
void set_recv_timeout(int sockfd, int sec);

#endif