#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <netinet/in.h>
#include <netdb.h>

// We'll use this UDP port for pings
#define PORT "8888"

// Timeout in seconds for each iteration's receive window
#define RECV_TIMEOUT_SEC 1

// Max number of peers to read from the file
#define MAX_PEERS 100

// Helper: set a short receive timeout on the socket
static void set_recv_timeout(int sockfd, int sec)
{
    struct timeval tv;
    tv.tv_sec = sec;
    tv.tv_usec = 0;
    setsockopt(sockfd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

int main(int argc, char *argv[])
{
    if (argc != 2)
    {
        fprintf(stderr, "Usage: %s <hostsfile.txt>\n", argv[0]);
        return 1;
    }

    // 1. Get our own hostname (assigned by Docker if needed)
    char myName[256];
    if (gethostname(myName, sizeof(myName)) != 0)
    {
        perror("gethostname");
        return 1;
    }
    myName[255] = '\0';
    printf("I am '%s'. Reading peers from %s\n", myName, argv[1]);

    // 2. Read the file of peer hostnames
    FILE *fp = fopen(argv[1], "r");
    if (!fp)
    {
        perror(argv[1]);
        return 1;
    }

    char *peers[MAX_PEERS];
    int peer_count = 0;
    memset(peers, 0, sizeof(peers));

    char line[256];
    while (fgets(line, sizeof(line), fp))
    {
        // Strip newline
        char *nl = strchr(line, '\n');
        if (nl)
            *nl = '\0';
        // Skip empty lines
        if (strlen(line) == 0)
            continue;

        peers[peer_count] = strdup(line); // allocate & copy
        if (!peers[peer_count])
        {
            perror("strdup");
            fclose(fp);
            return 1;
        }
        peer_count++;
        if (peer_count >= MAX_PEERS)
            break;
    }
    fclose(fp);

    // 3. Create and bind a UDP socket on port 8888
    int sockfd = -1;
    {
        struct addrinfo hints, *res;
        memset(&hints, 0, sizeof hints);
        hints.ai_family = AF_UNSPEC;    // IPv4 or IPv6
        hints.ai_socktype = SOCK_DGRAM; // UDP
        hints.ai_flags = AI_PASSIVE;    // For binding

        int rv = getaddrinfo(NULL, PORT, &hints, &res);
        if (rv != 0)
        {
            fprintf(stderr, "getaddrinfo: %s\n", gai_strerror(rv));
            return 1;
        }

        // Try binding
        struct addrinfo *p;
        for (p = res; p != NULL; p = p->ai_next)
        {
            sockfd = socket(p->ai_family, p->ai_socktype, p->ai_protocol);
            if (sockfd < 0)
                continue;

            if (bind(sockfd, p->ai_addr, p->ai_addrlen) == 0)
            {
                // success
                break;
            }
            close(sockfd);
            sockfd = -1;
        }
        freeaddrinfo(res);

        if (sockfd < 0)
        {
            perror("bind");
            return 1;
        }
    }

    // We’ll store an "online" flag for each peer
    int *online = calloc(peer_count, sizeof(int));
    if (!online)
    {
        perror("calloc");
        return 1;
    }

    printf("Beginning to ping peers...\n");

    int all_online_printed = 0; // to ensure we only print once

    // // Main loop: keep pinging until ALL peers are online, but don't exit.
    while (1) {
        // 4a. Send "ping:myName" to every peer not yet marked online, except ourselves
        for (int i = 0; i < peer_count; i++) {
            if (online[i]) {
                continue; // already online
            }
            if (strcmp(peers[i], myName) == 0) {
                online[i] = 1; // mark self as online
                continue;
            }

            // Resolve the peer
            struct addrinfo hints, *res;
            memset(&hints, 0, sizeof hints);
            hints.ai_family   = AF_UNSPEC;
            hints.ai_socktype = SOCK_DGRAM;

            int rv = getaddrinfo(peers[i], PORT, &hints, &res);
            if (rv != 0) {
                fprintf(stderr, "getaddrinfo(%s): %s\n", peers[i], gai_strerror(rv));
                fflush(stderr);
                continue;
            }

            char msg[300];
            snprintf(msg, sizeof(msg), "ping:%s", myName);

            // Send to the first valid address
            int sent_ok = 0;
            for (struct addrinfo *paddr = res; paddr != NULL; paddr = paddr->ai_next) {
                ssize_t sent = sendto(sockfd, msg, strlen(msg), 0,
                                    paddr->ai_addr, paddr->ai_addrlen);
                if (sent >= 0) {
                    sent_ok = 1;
                    break;
                }
            }
            freeaddrinfo(res);

            if (!sent_ok) {
                printf("Failed to send ping to %s\n", peers[i]);
                fflush(stdout);
            }
        }

        // 4b. Listen for responses
        struct sockaddr_storage sender_addr;
        socklen_t addr_len = sizeof(sender_addr);
        char buffer[300];

        ssize_t received = recvfrom(sockfd, buffer, sizeof(buffer) - 1, 0,
                                    (struct sockaddr *)&sender_addr, &addr_len);
        if (received < 0) {
            if (errno != EAGAIN && errno != EWOULDBLOCK) {
                perror("recvfrom");
            }
            // If no messages, skip to the next iteration
            sleep(1); // Small delay to avoid a tight loop
            continue;
        }

        buffer[received] = '\0';

        // Process messages
        if (strncmp(buffer, "ping:", 5) == 0) {
            char reply[300];
            snprintf(reply, sizeof(reply), "pong:%s", myName);
            if (sendto(sockfd, reply, strlen(reply), 0,
                    (struct sockaddr *)&sender_addr, addr_len) < 0) {
                perror("sendto (pong)");
            }
        } else if (strncmp(buffer, "pong:", 5) == 0) {
            const char *theirName = buffer + 5;
            for (int i = 0; i < peer_count; i++) {
                if (strcmp(peers[i], theirName) == 0) {
                    if (!online[i]) {
                        online[i] = 1;
                        printf("Peer '%s' is now ONLINE.\n", theirName);
                        fflush(stdout);
                    }
                }
            }
        } else {
            printf("Got unknown message: %s\n", buffer);
            fflush(stdout);
        }

        // 4c. Check if all peers are online
        int all_online = 1;
        for (int i = 0; i < peer_count; i++) {
            if (!online[i]) {
                all_online = 0;
                break;
            }
        }

        if (all_online && !all_online_printed) {
            printf("All peers are ONLINE.\n");
            fflush(stdout);
            all_online_printed = 1;
        }

        // Sleep briefly to avoid a tight loop
        sleep(1);
    }

    // We never reach here unless you manually stop the container or break the loop
    close(sockfd);
    for (int i = 0; i < peer_count; i++)
    {
        free(peers[i]);
    }
    free(online);

    return 0;
}