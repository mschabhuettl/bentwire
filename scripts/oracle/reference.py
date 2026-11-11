"""Find the reference parsers by file name below BENTWIRE_REFERENCE (architecture A-TEST-1)."""
import os

SKIP = {"cache", "__pycache__", "venv", ".venv", "site-packages"}


def find_dir(reference, *names):
    """The first directory (in sorted walk order) below `reference` holding all of `names`."""
    for d, subdirs, files in os.walk(reference):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP)
        if all(n in files for n in names):
            return d
    raise SystemExit("NOT RUN: %s not found below %s" % (", ".join(names), reference))
