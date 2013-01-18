#!/usr/bin/env python
# xfail-license

import os, tarfile, hashlib, re, shutil, sys
from snapshot import *

f = open(snapshotfile)
date = None
rev = None
platform = None
snap = None
i = 0

newestSet = {}


for line in f.readlines():
    i += 1
    parsed = parse_line(i, line)
    if (not parsed): continue

    if parsed["type"] == "snapshot":
        if (len(newestSet) == 0 or parsed["date"] > newestSet["date"]):
            newestSet["date"] = parsed["date"]
            newestSet["rev"] = parsed["rev"]
            newestSet["files"] = []
            addingMode = True
        else:
            addingMode = False

    elif addingMode == True and parsed["type"] == "file":
        tux = re.compile("linux", re.IGNORECASE)
        if (tux.match(parsed["platform"]) != None):
           ff = {}
           ff["platform"] = parsed["platform"]
           ff["hash"] = parsed["hash"]
           newestSet["files"] += [ff]


def download_new_file (date, rev, platform, hsh):
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

for ff in newestSet["files"]:
   download_new_file (newestSet["date"], newestSet["rev"],
                      ff["platform"], ff["hash"])


