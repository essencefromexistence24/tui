"""Extract the Ubuntu sysroot debs for Windows->Linux cross builds.

Usage: python scripts/linux-sysroot/extract.py
Reads *.deb from scripts/linux-sysroot/debs/, extracts data.tar.* under
scripts/linux-sysroot/sysroot/ with symlinks resolved to real files
(Windows checkouts cannot rely on symlink privileges).
"""
import io
import os
import tarfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEBS = os.path.join(HERE, "debs")
DST = os.path.join(HERE, "sysroot")


def ar_members(path):
    with open(path, "rb") as fh:
        assert fh.read(8) == b"!<arch>\n"
        while True:
            hdr = fh.read(60)
            if len(hdr) < 60:
                return
            name = hdr[:16].decode().strip()
            size = int(hdr[48:58].decode().strip())
            assert hdr[58:60] == b"`\n"
            data = fh.read(size)
            if size % 2:
                fh.read(1)
            yield name.rstrip("/"), data


def main():
    import zstandard  # pip install zstandard
    os.makedirs(DST, exist_ok=True)
    for deb in sorted(os.listdir(DEBS)):
        if not deb.endswith(".deb"):
            continue
        for name, data in ar_members(os.path.join(DEBS, deb)):
            if not name.startswith("data.tar"):
                continue
            if name.endswith(".zst"):
                data = zstandard.ZstdDecompressor().stream_reader(
                    io.BytesIO(data)).read()
            tf = tarfile.open(fileobj=io.BytesIO(data), mode="r:")
            for member in tf.getmembers():
                # Resolve symlinks/hardlinks to real file bytes.
                if member.issym() or member.islnk():
                    target = os.path.join(
                        os.path.dirname(member.name), member.linkname)
                    target = os.path.normpath(target)
                    try:
                        src = tf.extractfile(target)
                    except KeyError:
                        continue
                    if src is None:
                        continue
                    out = os.path.join(DST, member.name)
                    os.makedirs(os.path.dirname(out), exist_ok=True)
                    with open(out, "wb") as fh:
                        fh.write(src.read())
                    print("resolved-link", member.name)
                    continue
                tf.extract(member, DST, filter="data")
            print("extracted", deb)


if __name__ == "__main__":
    main()
