#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// Own disposable child, with a maximum lifetime even if the test runner exits.
int main(int argc, char **argv) {
    pid_t child = fork();
    if (child < 0) return 1;
    if (child > 0) return 0;
    if (argc > 1 && strcmp(argv[1], "ignore-term") == 0) signal(SIGTERM, SIG_IGN);
    alarm(20);
    printf("%d\n", getpid());
    fflush(stdout);
    close(STDIN_FILENO); close(STDOUT_FILENO); close(STDERR_FILENO);
    for (;;) pause();
}
