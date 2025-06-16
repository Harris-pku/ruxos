#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define READ_TIMES 1000

int main() {
    FILE *file;
    char buffer[1024];
    clock_t start, end;
    double cpu_time_used;

    // 打开文件
    file = fopen("test.txt", "r");
    if (file == NULL) {
        perror("无法打开文件");
        return 1;
    }

    // 记录开始时间
    start = clock();

    // 循环读取文件 1000 次
    for (int i = 0; i < READ_TIMES; i++) {
        rewind(file); // 将文件指针重置到文件开头
        while (fgets(buffer, sizeof(buffer), file) != NULL) {
            // 这里只是读取，不做其他处理
        }
    }

    // 记录结束时间
    end = clock();

    // 关闭文件
    fclose(file);

    // 计算运行时间
    cpu_time_used = ((double) (end - start)) / CLOCKS_PER_SEC;

    printf("读取文件 %d 次的运行时间: %f 秒\n", READ_TIMES, cpu_time_used);

    return 0;
}