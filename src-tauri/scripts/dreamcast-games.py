#!/usr/bin/env python3
"""Regenerates src/emulator/dreamcast.tsv: the Dreamcast games with box art in
libretro-thumbnails, by their Redump name, and the serials Redump gives each.

    python3 scripts/dreamcast-games.py > src/emulator/dreamcast.tsv
"""

import collections
import json
import re
import urllib.request

TREE = "https://api.github.com/repos/libretro-thumbnails/Sega_-_Dreamcast/git/trees/master?recursive=1"
DAT = "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/redump/Sega%20-%20Dreamcast.dat"


def get(url):
    with urllib.request.urlopen(url) as response:
        return response.read().decode()


tree = json.loads(get(TREE))
assert not tree["truncated"]
covers = sorted(
    entry["path"][len("Named_Boxarts/") : -len(".png")]
    for entry in tree["tree"]
    if entry["path"].startswith("Named_Boxarts/") and entry["path"].endswith(".png")
)

serials = collections.defaultdict(list)
name = None
for line in get(DAT).splitlines():
    if m := re.match(r'\tname "(.*)"$', line):
        name = m.group(1)
    elif (m := re.match(r'\tserial "(.*)"$', line)) and m.group(1) not in serials[name]:
        serials[name].append(m.group(1))


def plain(name):
    """`name` without its languages, `(En,Ja)`, as older names were."""
    return re.sub(r" \([A-Z][a-z](,[A-Z][a-z])*\)", "", name)


plain_serials = collections.defaultdict(list)
for name, codes in serials.items():
    plain_serials[plain(name)] += [c for c in codes if c not in plain_serials[plain(name)]]

for name in covers:
    codes = serials.get(name) or plain_serials.get(plain(name), [])
    print(f"{name}\t{' '.join(codes)}")
