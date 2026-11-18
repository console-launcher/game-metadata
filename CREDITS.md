# Credits and licence

All art in these files comes from SuperTuxKart's own free art (https://supertuxkart.net): the Tux kart, and in the 3DS banner also the gift box, track textures, effects and the logo. The demo files are licensed **CC-BY-SA 3.0**, like the art they carry; the generator code is MPL-2.0 (the repository's licence).

No Nintendo, Sony, Sega, Microsoft or publisher art, fonts or logos are used. A few headers must carry fixed signature bytes to be recognised (the Game Boy logo bytes, `TMR SEGA`, `SEGA MEGA DRIVE`); those are format magic, not art.

The line the website shows under its demo (the "> " line, read by `scripts/site-build`):

> Tux files: art from SuperTuxKart (https://supertuxkart.net) by Julian "XGhost" Schönbächler and others (the 3DS banner: MiniBjorn, Thomas Oppl, Moser Juan José, Semphris, rastapax), CC-BY-SA 3.0; the demo files are CC-BY-SA 3.0 too.

| Asset | What | Author | Licence | Source |
|---|---|---|---|---|
| tux.blend, tux.spm (the posed mesh) | Tux kart model | Julian "XGhost" Schönbächler | CC-BY-SA 3.0 | https://github.com/supertuxkart/stk-assets (karts/tux) |
| tux_body.png, tuxkart.png, wheel.png, engine.png | Tux kart textures | Julian "XGhost" Schönbächler | CC-BY-SA 3.0 | https://github.com/supertuxkart/stk-assets (karts/tux) |
| the 3DS HOME banner in tux.3ds and tux.cia | Tux kart, gift box, track textures, sky, effects and the logo (fixtures/showcase/supertuxkart/CREDITS.md lists each file) | Julian "XGhost" Schönbächler, MiniBjorn, Thomas Oppl, Moser Juan José, Semphris, rastapax | CC-BY-SA 3.0 (item_spark: CC0; logo: CC-BY-SA 3.0+) | https://svn.code.sf.net/p/supertuxkart/code, https://github.com/supertuxkart/stk-code |

What was changed: the mesh was posed as the game's "straight" animation and rendered from eight angles by `render_tux.py` (a small software rasteriser; `export_blend.py` writes the mesh from the .blend); the frames are cropped, scaled, composited on a sky-blue gradient, palette-quantized or packed into each platform's pixel format by the generator. Left out: `tux_icon.png` (GPL only).
