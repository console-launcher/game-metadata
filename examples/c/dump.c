/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

/* Reads a file through the C API with a read callback over stdio: dump <file>. */
#include <stdio.h>
#include "gamemeta.h"

static int32_t read_file(void *user, uint64_t offset, uint8_t *buf, size_t len) {
    FILE *f = (FILE *)user;
    if (fseek(f, (long)offset, SEEK_SET) != 0) return GM_READ_RESULT_ERROR;
    return fread(buf, 1, len, f) == len ? GM_READ_RESULT_OK : GM_READ_RESULT_OUT_OF_RANGE;
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    if (gm_abi_version() != GM_ABI_VERSION) return 3;
    FILE *f = fopen(argv[1], "rb");
    if (!f) return 4;
    fseek(f, 0, SEEK_END);
    uint64_t size = (uint64_t)ftell(f);
    GmInfo *info = NULL;
    GmStatus status = gm_read_callback(size, read_file, f, argv[1], &info);
    fclose(f);
    if (status != GM_STATUS_OK) { printf("status %d\n", (int)status); return 1; }
    int lang = gm_info_preferred_language(info);
    uint32_t w = 0, h = 0;
    const uint8_t *px = gm_info_icon(info, GM_ICON_LARGE, &w, &h);
    printf("title=%s\npublisher=%s\ntitle_id=%s\nicon=%ux%u first=%02x%02x%02x%02x\n",
           gm_info_title(info, (uint32_t)lang, GM_TITLE_FIELD_NAME),
           gm_info_title(info, (uint32_t)lang, GM_TITLE_FIELD_PUBLISHER),
           gm_info_id(info, GM_ID_TITLE_ID), w, h, px[0], px[1], px[2], px[3]);
    gm_info_free(info);
    return 0;
}
