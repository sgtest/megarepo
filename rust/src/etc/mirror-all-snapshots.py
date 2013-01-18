#!/usr/bin/env python
# xfail-license

import os, tarfile, hashlib, re, shutil
from snapshot import *

f = open(snapshotfile)
date = None
rev = None
platform = None
snap = None
i = 0

for line in f.readlines():
    i += 1
    parsed = parse_line(i, line)
    if (not parsed): continue

    if parsed["type"] == "snapshot":
        date = parsed["date"]
        rev = parsed["rev"]

    elif rev != None and parsed["type"] == "file":
        platform = parsed["platform"]
        hsh = parsed["hash"]
        snap = full_snapshot_name(date, rev, platform, hsh)
        dl = os.path.join(download_dir_base, snap)
        url = download_url_base + "/" + snap
        if (not os.path.exists(dl)):
            print("downloading " + url)
            get_url_to_file(url, dl)
        if (snap_filename_hash_part(snap) == hash_file(dl)):
            print("got download with ok hash")
        else:
            raise Exception("bad hash on download")



