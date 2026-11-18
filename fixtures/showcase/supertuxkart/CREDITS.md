# Credits

This banner (`banner.cbmd`, its scene `banner.json`, the reference frames in `banner.home/`, and the derived files in
`assets/` and `source/`) is licensed under [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/), like the art it carries and the demo files it goes into (`demo/tux.3ds`, `demo/tux.cia`). It is built
from SuperTuxKart's free-licensed art, listed below. Every file is CC BY-SA 3.0 or later, or CC0, and was used under its
terms.

SuperTuxKart: <https://supertuxkart.net>. The sources are SuperTuxKart's asset repository,
`https://svn.code.sf.net/p/supertuxkart/code/`, and its code repository, `https://github.com/supertuxkart/stk-code`.
Licences are as stated in their `licenses.txt` files. Nothing here is endorsed by the SuperTuxKart team.

| file here | from | author | licence | what changed |
|---|---|---|---|---|
| `source/tux-kart.mesh` | `media/trunk/karts/tux/tux.blend` | Julian "XGhost" Schönbächler, 2015 | CC BY-SA 3.0 | Frame 20 posed, triangulated, axes converted, split by bone (`source/export_blend.py`); shading baked into vertex colours, turned to a three-quarter view |
| `assets/tuxkart.png` | `stk-assets/karts/tux/tuxkart.png` | Julian "XGhost" Schönbächler | CC BY-SA 3.0 | Resized to 256 x 256, ETC1 |
| `assets/tux_body.png` | `stk-assets/karts/tux/tux_body.png` | Julian "XGhost" Schönbächler | CC BY-SA 3.0 | Resized to 256 x 256, ETC1 |
| `assets/engine.png` | `stk-assets/karts/tux/engine.png` | Julian "XGhost" Schönbächler | CC BY-SA 3.0 | Resized to 128 x 128, ETC1 |
| `assets/wheel.png` | `stk-assets/karts/tux/wheel.png` | Julian "XGhost" Schönbächler | CC BY-SA 3.0 | Resized to 64 x 64, RGB565 |
| `assets/tux_shadow.png` | `stk-assets/karts/tux/tux_shadow.png` | Julian "XGhost" Schönbächler | CC BY-SA 3.0 | Resized to 64 x 64, flattened on white, L8, multiplied |
| `source/gift-box.mesh` | `media/trunk/models/gift-box.blend` | MiniBjorn, 2008-2010 (slight changes by Marianne Gagnon) | CC BY-SA 3.0 | Triangulated, axes converted (`source/export_blend.py`) |
| `assets/gift-box.png` | `media/trunk/models/gift-box.png` | MiniBjorn | CC BY-SA 3.0 | Resized to 64 x 32, RGBA8, tinted through a cycling colour |
| `assets/racetrack_asphalt.png` | `stk-assets/textures/racetrack_asphalt.jpg` | Thomas Oppl (horace), 2008 | CC BY-SA 3.0 | Resized to 128 x 64, RGBA5551 |
| `assets/racetrack_grass.png` | `stk-assets/textures/racetrack_grass.jpg` | Thomas Oppl (horace), 2008 | CC BY-SA 3.0 | Resized to 64 x 64, ETC1, striped by vertex colour |
| `assets/city_checker.png` | `stk-assets/textures/city_checker.png` | Thomas Oppl (horace), 2008 | CC BY-SA 3.0 | Resized to 32 x 32, L4, coloured red and white for curbs |
| `assets/sky2.png` | `stk-assets/textures/sky2.jpg` | MiniBjorn, 2008-2010 | CC BY-SA 3.0 | Resized to 128 x 64, RGB8 |
| `assets/nitro-particle.png` | `stk-assets/textures/nitro-particle.png` | MiniBjorn, 2008-2010 | CC BY-SA 3.0 | Resized to 64 x 64, RGB565, tinted |
| `assets/flaring-star.png` | `stk-assets/textures/flaring-star.png` | MiniBjorn, 2008-2010 | CC BY-SA 3.0 | Resized to 64 x 64, RGBA4 |
| `assets/smoke.png` | `stk-assets/textures/smoke.png` | MiniBjorn, 2008-2010 | CC BY-SA 3.0 | Resized to 64 x 64, A8 over a dust colour |
| `assets/pino.png` | `stk-assets/textures/pino.png` | Moser Juan José | CC BY-SA 3.0 | Resized to 128 x 128. Its foliage half becomes the tiling pine row (ETC1A4) |
| `assets/item_spark.png` | `stk-assets/textures/item_spark.png` | Semphris | CC0 | Resized to 64 x 64, LA8, added to gold |
| `assets/logo_slim.png` | stk-code `data/gui/icons/logo_slim.png` | rastapax | CC BY-SA 3.0 or later | Resized to 128 x 64, RGBA4, shown as the banner's title in every language |

Not used: `tux_icon.png` (GPL only), and any art whose licence was unclear. There is no Nintendo, publisher or other
third-party art, font or logo. The banner is silent (a silent BCWAV).

The scene itself (layout, animation and materials) is `crates/gamemeta-fixtures/src/stk_banner.rs`, under this repository's
MPL 2.0. The frames in `banner.home/` are the emulator's renders of this banner. Everything outside the banner window
(HOME's wall and status bars) is masked out in pure magenta.
