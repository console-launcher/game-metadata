#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Banner plumbing for the HOME Menu captures. Unencrypted (NoCrypto) NCCH only: this never decrypts anything.

  banner.py extract <file.cbmd|.3ds|.cia|.cxi> <out.cbmd>   the ExeFS 'banner' of a 3DS file (a .cbmd is copied)
  banner.py swap <slot.cia> <banner.cbmd> <out.cia>        the slot app's CIA with its ExeFS banner replaced, hashes fixed
  banner.py swap-app <installed-content.app> <banner.cbmd> the installed slot app's first content rewritten in place

HOME lists only title ids it already knows, so a capture borrows the banner of a visible HOME app (its own, decrypted CIA, from
your own console) instead of installing a new title. Those files stay where you keep them: this script reads them and writes
only the paths you give it; it prints sizes and names, never contents."""
import hashlib
import os
import struct
import sys

SIG = {0x10000: 0x240, 0x10001: 0x140, 0x10002: 0x80, 0x10003: 0x240, 0x10004: 0x140, 0x10005: 0x80}


def al(x, a=64):
    return (x + a - 1) // a * a


def sha(b):
    return hashlib.sha256(b).digest()


def exefs_files(ncch):
    if ncch[0x100:0x104] != b'NCCH':
        sys.exit('not an NCCH')
    if not ncch[0x188 + 7] & 4:
        sys.exit('the NCCH is encrypted; only NoCrypto contents are read (nothing here decrypts)')
    eo, es = [x * 0x200 for x in struct.unpack('<II', ncch[0x1A0:0x1A8])]
    ex = bytes(ncch[eo:eo + es])
    files = []
    for i in range(10):
        nm = ex[i * 16:i * 16 + 8].rstrip(b'\0')
        if nm:
            off, sz = struct.unpack('<II', ex[i * 16 + 8:i * 16 + 16])
            files.append((nm, ex[0x200 + off:0x200 + off + sz]))
    return files


def cia_contents(d):
    """(offset, size, tmd record offset) of each present content of a CIA, plus its layout."""
    hs, _, _, cc, tk, tm, meta = struct.unpack('<IHHIIII', d[:24])
    to = al(hs) + al(cc)
    tmo = to + al(tk)
    mo = tmo + al(tm)
    co = mo + al(meta)
    tmd = bytearray(d[tmo:tmo + tm])
    if tm < 4:  # a minimal CIA (no ticket or TMD): its one content follows the header
        return [(co, len(d) - co, None)], (tmo, tm, mo, co, 0, 0, tmd)
    h = SIG[struct.unpack('>I', tmd[:4])[0]]
    n = struct.unpack('>H', tmd[h + 0x9E:h + 0xA0])[0]
    ch = h + 0xC4 + 64 * 0x24
    present = d[0x20:0x2020]
    out, pos = [], co
    for i in range(n):
        rec = ch + i * 0x30
        _, idx, _, size = struct.unpack('>IHHQ', tmd[rec:rec + 16])
        if present[idx // 8] & (0x80 >> (idx % 8)):
            out.append((pos, size, rec))
            pos += size
    return out, (tmo, tm, mo, co, h, ch, tmd)


def first_ncch(path):
    d = open(path, 'rb').read()
    if d[:4] == b'CBMD':
        return None, d
    if d[0x100:0x104] == b'NCSD':
        off = struct.unpack('<I', d[0x120:0x124])[0] * 0x200
        return d[off:], None
    if d[0x100:0x104] == b'NCCH':
        return d, None
    contents, _ = cia_contents(d)
    for pos, size, _ in contents:
        if d[pos + 0x100:pos + 0x104] == b'NCCH':
            return d[pos:pos + size], None
    sys.exit(f'{path}: no NCCH found')


def extract(src, out):
    ncch, banner = first_ncch(src)
    if banner is None:
        banner = dict(exefs_files(ncch)).get(b'banner') or sys.exit(f'{src}: no ExeFS banner')
    open(out, 'wb').write(banner)
    print(f'{os.path.basename(out)}: {len(banner)} bytes')


def new_ncch(c, banner):
    c = bytearray(c)
    files = [(nm, banner if nm == b'banner' else b) for nm, b in exefs_files(c)]
    assert any(f[0] == b'banner' for f in files), 'content has no banner'
    eo = struct.unpack('<I', c[0x1A0:0x1A4])[0] * 0x200
    ro, rs = [x * 0x200 for x in struct.unpack('<II', c[0x1B0:0x1B8])]
    hdr, data, off = bytearray(0x200), bytearray(), 0
    for i, (nm, b) in enumerate(files):
        hdr[i * 16:i * 16 + 16] = nm.ljust(8, b'\0') + struct.pack('<II', off, len(b))
        hdr[0xC0 + (9 - i) * 32:0xC0 + (10 - i) * 32] = sha(b)
        data += b.ljust(al(len(b), 0x200), b'\0')
        off += al(len(b), 0x200)
    new_ex = bytes(hdr) + bytes(data)
    rom = bytes(c[ro:ro + rs]) if rs else b''
    c2 = bytearray(bytes(c[:eo]) + new_ex + rom)
    c2[0x1A0:0x1A8] = struct.pack('<II', eo // 0x200, len(new_ex) // 0x200)
    c2[0x1C0:0x1E0] = sha(bytes(hdr))
    if rs:
        c2[0x1B0:0x1B8] = struct.pack('<II', (eo + len(new_ex)) // 0x200, rs // 0x200)
    c2[0x104:0x108] = struct.pack('<I', len(c2) // 0x200)
    return bytes(c2)


def swap(src, banner_path, out):
    d = open(src, 'rb').read()
    banner = open(banner_path, 'rb').read()
    contents, (tmo, tm, mo, co, h, ch, tmd) = cia_contents(d)
    if tm < 4:
        sys.exit('swap needs a CIA with a TMD (the slot app\'s own)')
    outc, done, total = bytearray(), False, 0
    for pos, size, rec in contents:
        c = d[pos:pos + size]
        if not done and c[0x100:0x104] == b'NCCH':
            c = new_ncch(c, banner)
            done = True
            tmd[rec + 8:rec + 16] = struct.pack('>Q', len(c))
            tmd[rec + 16:rec + 48] = sha(c)
        outc += c
        total += len(c)
    assert done, 'no NCCH content'
    for j in range(64):
        r = h + 0xC4 + j * 0x24
        idx, cnt = struct.unpack('>HH', tmd[r:r + 4])
        if cnt:
            tmd[r + 4:r + 36] = sha(bytes(tmd[ch + idx * 0x30:ch + (idx + cnt) * 0x30]))
    tmd[h + 0xA4:h + 0xC4] = sha(bytes(tmd[h + 0xC4:h + 0xC4 + 64 * 0x24]))
    hdr_cia = bytearray(d[:tmo])
    hdr_cia[0x18:0x20] = struct.pack('<Q', total)
    open(out, 'wb').write(bytes(hdr_cia) + bytes(tmd) + d[tmo + tm:tmo + al(tm)] + d[mo:co] + bytes(outc))


def swap_app(app, banner_path):
    open(app + '.tmp', 'wb').write(new_ncch(open(app, 'rb').read(), open(banner_path, 'rb').read()))
    os.replace(app + '.tmp', app)


if __name__ == '__main__':
    cmds = {'extract': (extract, 2), 'swap': (swap, 3), 'swap-app': (swap_app, 2)}
    if len(sys.argv) < 2 or sys.argv[1] not in cmds or len(sys.argv) != 2 + cmds[sys.argv[1]][1]:
        sys.exit(__doc__)
    cmds[sys.argv[1]][0](*sys.argv[2:])
