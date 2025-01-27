#include "timeout.h"

// Helper: set a short receive timeout on the socket
void set_recv_timeout(int sockfd, int sec)
{
    struct timeval tv;
    tv.tv_sec = sec;
    tv.tv_usec = 0;
    setsockopt(sockfd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

int main(int argc, char *argv[])
{
    // 1. Parse command-line arguments
    char *hostsfile = NULL;
    int state = 0;

    for (int i = 1; i < argc; i++)
    {
        if (strcmp(argv[i], "-h") == 0 && i + 1 < argc)
        {
            hostsfile = argv[++i];
        }
        else if (strcmp(argv[i], "-x") == 0)
        {
            state = 1; // If -x is present, set STATE to 1
        }
        else if (strcmp(argv[i], "-t") == 0 && i + 1 < argc)
        {
            ++i;
        }
        else
        {
            fprintf(stderr, "Unknown option: %s\n", argv[i]);
            return 1;
        }
    }

    if (!hostsfile)
    {
        fprintf(stderr, "Error: Missing hostsfile path. Usage: %s -h <hostsfile> [-x]\n", argv[0]);
        return 1;
    }

    if (access(hostsfile, F_OK) != 0)
    {
        perror("Error: Hostsfile not found");
        return 1;
    }

    // 2. Get our own hostname and read the list of peers from the hostsfile
    char myName[256];
    if (gethostname(myName, sizeof(myName)) != 0)
    {
        perror("gethostname failed");
        return 1;
    }
    myName[255] = '\0'; // Ensure null-termination

    FILE *fp = fopen(hostsfile, "r");
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

        peers[peer_count] = strdup(line); // Allocate & copy
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

    // Determine our ID and calculate predecessor and successor
    int myID = -1;
    for (int i = 0; i < peer_count; i++)
    {
        if (strcmp(myName, peers[i]) == 0)
        {
            myID = i + 1;
            break;
        }
    }

    if (myID == -1)
    {
        fprintf(stderr, "Error: Hostname '%s' not found in the hostsfile\n", myName);
        return 1;
    }

    // Would be great if I can do modulo arithmetic here but I'm not sure how to when the range is 1 to peer_count
    int predecessor = (myID == 1) ? peer_count : (myID - 1);
    int successor = (myID == peer_count) ? 1 : (myID + 1);

    printf("{id: %d, state: %d, predecessor: %d, successor: %d}\n",
           myID, state, predecessor, successor);

    // 3. Create and bind a UDP socket on port 8888
    // All clients will be listening on port 8888
    int sockfd = -1;
    {
        struct addrinfo hints, *res;
        memset(&hints, 0, sizeof hints);
        hints.ai_family = AF_UNSPEC;
        hints.ai_socktype = SOCK_DGRAM;
        hints.ai_flags = AI_PASSIVE;

        int rv = getaddrinfo(NULL, PORT, &hints, &res);
        if (rv != 0)
        {
            fprintf(stderr, "getaddrinfo: %s\n", gai_strerror(rv));
            return 1;
        }

        struct addrinfo *p;
        for (p = res; p != NULL; p = p->ai_next)
        {
            sockfd = socket(p->ai_family, p->ai_socktype, p->ai_protocol);
            if (sockfd < 0)
                continue;

            if (bind(sockfd, p->ai_addr, p->ai_addrlen) == 0) // success
            {
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

    int all_online_printed = 0; // to ensure we only print once

    // Main loop: keep pinging until all peers are online
    while (1)
    {
        // 4. Send "ping:myName" to every peer not yet marked online, except ourselves
        for (int i = 0; i < peer_count; i++)
        {
            if (online[i])
            {
                continue; // already online
            }
            if (strcmp(peers[i], myName) == 0)
            {
                online[i] = 1; // mark self as online
                continue;
            }

            struct addrinfo hints, *res;
            memset(&hints, 0, sizeof hints);
            hints.ai_family = AF_UNSPEC;
            hints.ai_socktype = SOCK_DGRAM;

            int rv = getaddrinfo(peers[i], PORT, &hints, &res);
            if (rv != 0)
            {
                continue;
            }

            char msg[300];
            snprintf(msg, sizeof(msg), "ping:%s", myName);

            // Send to the first valid address
            int sent_ok = 0;
            for (struct addrinfo *paddr = res; paddr != NULL; paddr = paddr->ai_next)
            {
                ssize_t sent = sendto(sockfd, msg, strlen(msg), 0,
                                      paddr->ai_addr, paddr->ai_addrlen);
                if (sent >= 0)
                {
                    sent_ok = 1;
                    break;
                }
            }
            freeaddrinfo(res);

            if (!sent_ok)
            {
                printf("Failed to send ping to %s\n", peers[i]);
                fflush(stdout);
            }
        }

        // 5. Listen for responses
        struct sockaddr_storage sender_addr;
        socklen_t addr_len = sizeof(sender_addr);
        char buffer[300];

        ssize_t received = recvfrom(sockfd, buffer, sizeof(buffer) - 1, 0,
                                    (struct sockaddr *)&sender_addr, &addr_len);
        if (received < 0)
        {
            if (errno != EAGAIN && errno != EWOULDBLOCK)
            {
                perror("recvfrom");
            }
            // If no messages, skip to the next iteration
            usleep(100000); // 100 milliseconds (I have tried lower values but the differences are indisguishable)
            continue;
        }

        buffer[received] = '\0';

        // Process messages
        if (strncmp(buffer, "ping:", 5) == 0)
        {
            char reply[300];
            snprintf(reply, sizeof(reply), "pong:%s", myName);
            if (sendto(sockfd, reply, strlen(reply), 0,
                       (struct sockaddr *)&sender_addr, addr_len) < 0)
            {
                perror("sendto (pong) failed :(");
            }
        }
        else if (strncmp(buffer, "pong:", 5) == 0)
        {
            const char *theirName = buffer + 5;
            for (int i = 0; i < peer_count; i++)
            {
                if (strcmp(peers[i], theirName) == 0)
                {
                    if (!online[i])
                    {
                        online[i] = 1;
                    }
                }
            }
        }
        else
        {
            printf("Got unknown message: %s\n", buffer);
            fflush(stdout);
        }

        int all_online = 1;

        for (int i = 0; i < peer_count; i++)
        {
            if (!online[i])
            {
                all_online = 0;
            }
        }

        if (all_online && !all_online_printed)
        {
            printf("READY\n");
                fflush(stdout);
            all_online_printed = 1;
        }

        // Wait a bit before the next iteration
        usleep(100000); // 100 milliseconds
    }

    // Never reach here unless manually stop the container
    close(sockfd);
    for (int i = 0; i < peer_count; i++)
    {
        free(peers[i]);
    }
    free(online);

    return 0;
}