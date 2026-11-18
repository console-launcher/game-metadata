# SuperTuxKart demo files

One tiny file per format the library reads, built by `gamemeta-fixtures` (`write-demo`) from SuperTuxKart's free-licensed art. Headers and art only: no game code. See [CREDITS.md](CREDITS.md).

| File | Platform | What the library reads from it |
|---|---|---|
| `tux.nds` | Nintendo DS | 8 titles, 32x32 icon, game code, maker |
| `tux.dsi` | Nintendo DSi | 8 titles, the 8-frame animated icon, the DSi banner block |
| `tux.zip` | Nintendo DS | the ROM inside a zip (stored): its CRC-32 and size, then the DS reader |
| `tux-deflate.zip` | Nintendo DS | the same with the ROM deflated |
| `tux.3ds` | Nintendo 3DS | 12 titles, publisher, 24x24 and 48x48 icons, title id, product code, the animated HOME banner |
| `tux.cia` | Nintendo 3DS | the same as an installable CIA |
| `tux-gc.iso` | GameCube | BNR2 titles in 6 languages, 96x32 banner, game id |
| `tux-gc.ciso` | GameCube | the same disc in a CISO container |
| `tux-gc.rvz` | GameCube | the same disc in an RVZ container |
| `tux-wii.rvz` | Wii | channel titles in 10 languages, the logo and an animated channel banner (the kart slides) from opening.bnr, game id |
| `tux-wii.iso` | Wii | the unencrypted disc header: game id, title |
| `tux-wii.wbfs` | Wii | the same header in a WBFS container |
| `tux.wad` | Wii | title id and game id of a channel |
| `tux-wiiu/` | Wii U | an unpacked title: meta.xml (12 titles), icon and boot images, a stub .rpx |
| `tux.wua` | Wii U | the same title in a .wua |
| `tux.pbp` | PSP | EBOOT.PBP: PARAM.SFO title and disc id (ICON0.PNG and PIC1.PNG inside) |
| `tux-psp.iso` | PSP | a disc image: PARAM.SFO title and disc id (ICON0.PNG and PIC1.PNG inside) |
| `tux-ps1.bin` | PlayStation | raw 2352-byte sectors: serial from SYSTEM.CNF |
| `tux-ps1.cue` | PlayStation | the cue sheet for tux-ps1.bin |
| `tux-ps2.iso` | PlayStation 2 | serial from SYSTEM.CNF |
| `tux.chd` | MAME CHD | v5 header: the raw SHA-1 of tux-ps2.iso |
| `tux.nsp` | Switch | title id from the PFS0 file names |
| `tux.xbe` | Xbox | title id, region and title name from the certificate |
| `tux.gb` | Game Boy | header fields: title, checksums, logo (no art) |
| `tux.gbc` | Game Boy Color | header fields: CGB flag |
| `tux.gba` | Game Boy Advance | header fields: title, game code, complement |
| `tux.z64` | Nintendo 64 | header fields, big-endian dump |
| `tux.v64` | Nintendo 64 | the same, byte-swapped: read as the same game |
| `tux.n64` | Nintendo 64 | the same, word-swapped: read as the same game |
| `tux.sfc` | Super Nintendo | header fields: title, map mode, checksum and complement |
| `tux.nes` | NES | iNES header: mapper, PRG and CHR sizes |
| `tux.md` | Mega Drive | header fields: titles, serial, region, checksum |
| `tux.sms` | Master System | header fields: product code, region, size, checksum |
| `tux.gg` | Game Gear | the same header with the Game Gear region |

The 3DS HOME banner in `tux.3ds` and `tux.cia` (the ExeFS `banner` entry) is the SuperTuxKart showcase banner, `fixtures/showcase/supertuxkart/banner.cbmd` (its art is credited in that folder's CREDITS.md); `write-demo --banner3ds <file>` puts another in.

The ids are homebrew style (maker `00` or none, codes and title ids in ranges no retail title uses). The publisher is "SuperTuxKart Team".
