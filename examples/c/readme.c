/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

/* The C snippet of the README, compiled (scripts/check) so it cannot rot; scripts/readme-snippets checks the README shows
 * exactly the marked lines. Run: readme <file>. */
#include <stdio.h>
#include "gamemeta.h"

static int32_t read_file(void *user, uint64_t offset, uint8_t *buf, size_t len) {
    FILE *f = (FILE *)user;
    if (fseek(f, (long)offset, SEEK_SET) != 0) return GM_READ_RESULT_ERROR;
    return fread(buf, 1, len, f) == len ? GM_READ_RESULT_OK : GM_READ_RESULT_OUT_OF_RANGE;
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    FILE *file = fopen(argv[1], "rb");
    if (!file) return 4;
    fseek(file, 0, SEEK_END);
    uint64_t size = (uint64_t)ftell(file);
    /* readme:begin */
    GmInfo *info = NULL;
    GmStatus status = gm_read_callback(size, read_file, file, "game.3ds", &info);
    if (status == GM_STATUS_OK) {
        printf("%s\n", gm_info_title(info, (uint32_t)gm_info_preferred_language(info), GM_TITLE_FIELD_NAME));
        gm_info_free(info);
    }
    /* readme:end */
    fclose(file);
    return status == GM_STATUS_OK ? 0 : 1;
}
