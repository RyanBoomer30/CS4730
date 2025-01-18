#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <fcntl.h>
#include <stdbool.h>

#define PORT 9999 // Port to listen on
#define BUF_SIZE 1024
#define DELAY_MICROSECONDS 500000 // 0.5 seconds

void error(const char *msg) {
    perror(msg);
    exit(EXIT_FAILURE);
}

void send_ready_messages(const char *self_hostname, const char *self_ip, char hostnames[][BUF_SIZE], char ips[][BUF_SIZE], int host_count) {
    int sockfd;
    struct sockaddr_in destaddr;

    fprintf(stderr, "[DEBUG] Entering send_ready_messages function\n");
    if ((sockfd = socket(AF_INET, SOCK_DGRAM, 0)) < 0) {
        error("Socket creation failed");
    }

    memset(&destaddr, 0, sizeof(destaddr));
    destaddr.sin_family = AF_INET;
    destaddr.sin_port = htons(PORT);

    for (int i = 0; i < host_count; i++) {
        if (strcmp(self_hostname, hostnames[i]) == 0) {
            fprintf(stderr, "[DEBUG] Skipping self: %s\n", self_hostname);
            continue; // Skip sending to self
        }

        destaddr.sin_addr.s_addr = inet_addr(ips[i]);

        fprintf(stderr, "[DEBUG] Sending to %s (%s)\n", hostnames[i], ips[i]);
        usleep(DELAY_MICROSECONDS); // Introduce delay to avoid overloading

        if (sendto(sockfd, self_hostname, strlen(self_hostname), 0,
                   (const struct sockaddr *)&destaddr, sizeof(destaddr)) < 0) {
            perror("Failed to send message");
        } else {
            fprintf(stderr, "[DEBUG] Sent READY message to %s (%s)\n", hostnames[i], ips[i]);
        }
    }

    close(sockfd);
    fprintf(stderr, "[DEBUG] Exiting send_ready_messages function\n");
}

int main(int argc, char *argv[]) {
    fprintf(stderr, "[DEBUG] Starting main function\n");

    if (argc != 2) {
        fprintf(stderr, "Usage: %s <hostfile>\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    const char *hostfile = argv[1];
    fprintf(stderr, "[DEBUG] Hostfile: %s\n", hostfile);

    // Open the hostfile
    FILE *file = fopen(hostfile, "r");
    if (!file) {
        error("Failed to open hostfile");
    }

    // Load hostnames and IPs into memory
    char line[BUF_SIZE];
    char hostnames[BUF_SIZE][BUF_SIZE];
    char ips[BUF_SIZE][BUF_SIZE];
    int host_count = 0;

    fprintf(stderr, "[DEBUG] Reading hostfile\n");
    while (fgets(line, BUF_SIZE, file)) {
        // Remove trailing newline
        line[strcspn(line, "\n")] = '\0';

        // Parse hostname and IP address
        char *token = strtok(line, ":");
        if (!token) {
            fprintf(stderr, "Invalid hostfile format. Expected hostname: IP\n");
            exit(EXIT_FAILURE);
        }
        strncpy(hostnames[host_count], token, BUF_SIZE);

        token = strtok(NULL, "/");
        if (!token) {
            fprintf(stderr, "Invalid hostfile format. Expected hostname: IP\n");
            exit(EXIT_FAILURE);
        }
        strncpy(ips[host_count], token, BUF_SIZE);

        fprintf(stderr, "[DEBUG] Loaded host: %s, IP: %s\n", hostnames[host_count], ips[host_count]);
        host_count++;
    }
    fclose(file);

    if (host_count <= 1) {
        fprintf(stderr, "Hostfile must contain at least two hostnames.\n");
        exit(EXIT_FAILURE);
    }

    const char *self_hostname = hostnames[host_count - 1]; // Assume last entry is self
    const char *self_ip = ips[host_count - 1];
    fprintf(stderr, "[DEBUG] Self hostname: %s, IP: %s\n", self_hostname, self_ip);

    // Set up the UDP socket
    int sockfd;
    struct sockaddr_in servaddr, cliaddr;
    char buffer[BUF_SIZE];

    if ((sockfd = socket(AF_INET, SOCK_DGRAM, 0)) < 0) {
        error("Socket creation failed");
    }

    // Configure server address structure
    memset(&servaddr, 0, sizeof(servaddr));
    servaddr.sin_family = AF_INET;
    servaddr.sin_addr.s_addr = INADDR_ANY;
    servaddr.sin_port = htons(PORT);

    // Bind the socket to the port
    if (bind(sockfd, (const struct sockaddr *)&servaddr, sizeof(servaddr)) < 0) {
        error("Bind failed");
    }

    fprintf(stderr, "[DEBUG] UDP listener started on port %d\n", PORT);

    // Send READY messages to all other containers
    send_ready_messages(self_hostname, self_ip, hostnames, ips, host_count);

    bool ready[host_count];
    memset(ready, 0, sizeof(ready));

    int ready_count = 0;

    while (ready_count < host_count - 1) {
        socklen_t len = sizeof(cliaddr);
        memset(buffer, 0, BUF_SIZE);

        fprintf(stderr, "[DEBUG] Waiting for messages\n");
        // Receive message from any container
        ssize_t n = recvfrom(sockfd, (char *)buffer, BUF_SIZE, 0,
                             (struct sockaddr *)&cliaddr, &len);
        if (n < 0) {
            error("recvfrom failed");
        }

        buffer[n] = '\0'; // Null-terminate the message
        fprintf(stderr, "[DEBUG] Received message: %s\n", buffer);

        // Check if the message matches a known hostname
        for (int i = 0; i < host_count; i++) {
            if (strcmp(buffer, hostnames[i]) == 0) {
                if (!ready[i]) {
                    ready[i] = true;
                    ready_count++;
                    fprintf(stderr, "READY: Received message from %s (%s)\n", hostnames[i], ips[i]);
                }
                break;
            }
        }

        // Introduce delay to avoid overloading
        usleep(DELAY_MICROSECONDS);

        // Respond to sender
        fprintf(stderr, "[DEBUG] Sending response to %s\n", inet_ntoa(cliaddr.sin_addr));
        if (sendto(sockfd, self_hostname, strlen(self_hostname), 0,
                   (const struct sockaddr *)&cliaddr, len) < 0) {
            perror("Failed to send response");
        }
    }

    fprintf(stderr, "[DEBUG] All containers are READY\n");

    // Clean up
    for (int i = 0; i < host_count; i++) {
        // No dynamic memory allocation, no need to free
    }

    close(sockfd);
    fprintf(stderr, "[DEBUG] Exiting main function\n");
    return 0;
}

