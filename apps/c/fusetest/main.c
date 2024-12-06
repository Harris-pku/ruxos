/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

int main() {
    printf("Hello, %c app!\n", 'C');

    int file = open("testfile.txt", O_RDONLY);
    if (file == -1) {
        perror("Error opening file");
        return 1;
    }

    char buffer[256];
    ssize_t bytesRead;
    while ((bytesRead = read(file, buffer, sizeof(buffer) - 1)) > 0) {
        buffer[bytesRead] = '\0';
        printf("%s\n", buffer);
    }

    if (bytesRead == -1) {
        perror("Error reading file");
    }

    close(file);
    return 0;
}