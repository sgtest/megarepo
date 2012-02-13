#!/usr/bin/env python

import os, tarfile, hashlib, re, shutil, sys
from snapshot import *

def unpack_snapshot(triple, snap):
  dl_path = os.path.join(download_dir_base, snap)
  print("opening snapshot " + dl_path)
  tar = tarfile.open(dl_path)
  kernel = get_kernel(triple)
  for p in tar.getnames():

    # FIXME: Fix this once win32 snapshot globs are fixed.
    name = p.replace("rust-stage0/stage3/", "", 1);
    name = name.replace("rust-stage0/", "", 1);

    stagep = os.path.join(triple, "stage0")
    fp = os.path.join(stagep, name)
    print("extracting " + p)
    tar.extract(p, download_unpack_base)
    tp = os.path.join(download_unpack_base, p)
    shutil.move(tp, fp)
  tar.close()
  shutil.rmtree(download_unpack_base)

def determine_curr_snapshot(triple):
  i = 0
  platform = get_platform(triple)

  found_file = False
  found_snap = False
  hsh = None
  date = None
  rev = None

  f = open(snapshotfile)
  for line in f.readlines():
    i += 1
    parsed = parse_line(i, line)
    if (not parsed): continue

    if found_snap and parsed["type"] == "file":
      if parsed["platform"] == platform:
        hsh = parsed["hash"]
        found_file = True
        break;
    elif parsed["type"] == "snapshot":
      date = parsed["date"]
      rev = parsed["rev"]
      found_snap = True

  if not found_snap:
    raise Exception("no snapshot entries in file")

  if not found_file:
    raise Exception("no snapshot file found for platform %s, rev %s" %
                    (platform, rev))

  return full_snapshot_name(date, rev, platform, hsh)

# Main

triple = sys.argv[1]
snap = determine_curr_snapshot(triple)
dl = os.path.join(download_dir_base, snap)
url = download_url_base + "/" + snap
# FIXME: Temporary until we have FreeBSD snapshots on the official server
if "freebsd" in triple:
  url = "http://plaslab.cs.nctu.edu.tw/~jyyou/rust" + "/" + snap
print("determined most recent snapshot: " + snap)

if (not os.path.exists(dl)):
  get_url_to_file(url, dl)

if (snap_filename_hash_part(snap) == hash_file(dl)):
  print("got download with ok hash")
else:
  raise Exception("bad hash on download")

unpack_snapshot(triple, snap)
