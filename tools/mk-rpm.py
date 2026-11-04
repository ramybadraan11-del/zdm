#!/usr/bin/env python3
"""Write a valid RPM v3 package without rpmbuild.

Developed by Ramy Badraan

Why this exists
---------------
`rpmbuild` is a Fedora/RHEL tool. The packaging pipeline must still be able to
produce a real `.rpm` on Arch, Debian or inside a container, so this module
emits the binary format directly:

    lead (96 B) | signature header | main header | gzip'd cpio payload

Every field Fedora's rpm/dnf actually validates is emitted: header structure,
file metadata arrays, dependency arrays and the signature header digests. The
package is then re-read with an independent parser in CI to prove it is not
malformed.

Layout reference: RPM "v3" package format as implemented by rpm 4.x.
"""

from __future__ import annotations

import gzip
import hashlib
import io
import os
import stat
import struct
import time
from dataclasses import dataclass, field
from pathlib import Path

# ------------------------------------------------------------------ constants

# Header value types
NULL, CHAR, INT8, INT16, INT32, INT64, STRING, BIN, STRING_ARRAY, I18NSTRING = range(10)
ALIGNMENT = {INT16: 2, INT32: 4, INT64: 8}

# Header magic
HEADER_MAGIC = b"\x8e\xad\xe8\x01"

# Main header tags
TAG_NAME = 1000
TAG_VERSION = 1001
TAG_RELEASE = 1002
TAG_EPOCH = 1003
TAG_SUMMARY = 1004
TAG_DESCRIPTION = 1005
TAG_BUILDTIME = 1006
TAG_BUILDHOST = 1007
TAG_SIZE = 1009
TAG_LICENSE = 1014
TAG_PACKAGER = 1015
TAG_GROUP = 1016
TAG_URL = 1020
TAG_OS = 1021
TAG_ARCH = 1022
TAG_PREIN = 1023
TAG_POSTIN = 1024
TAG_PREUN = 1025
TAG_POSTUN = 1026
TAG_FILESIZES = 1028
TAG_FILEMODES = 1030
TAG_FILERDEVS = 1033
TAG_FILEMTIMES = 1034
TAG_FILEDIGESTS = 1035
TAG_FILELINKTOS = 1036
TAG_FILEFLAGS = 1037
TAG_FILEUSERNAME = 1039
TAG_FILEGROUPNAME = 1040
TAG_SOURCERPM = 1044
TAG_FILEVERIFYFLAGS = 1045
TAG_ARCHIVESIZE = 1046
TAG_PROVIDENAME = 1047
TAG_REQUIREFLAGS = 1048
TAG_REQUIRENAME = 1049
TAG_REQUIREVERSION = 1050
TAG_RPMVERSION = 1064
TAG_FILEDEVICES = 1095
TAG_FILEINODES = 1096
TAG_FILELANGS = 1097
TAG_PROVIDEFLAGS = 1112
TAG_PROVIDEVERSION = 1113
TAG_DIRINDEXES = 1116
TAG_BASENAMES = 1117
TAG_DIRNAMES = 1118
TAG_PAYLOADFORMAT = 1124
TAG_PAYLOADCOMPRESSOR = 1125
TAG_PAYLOADFLAGS = 1126
TAG_PREINPROG = 1085
TAG_POSTINPROG = 1086
TAG_PREUNPROG = 1087
TAG_POSTUNPROG = 1088
TAG_FILEDIGESTALGO = 5011
TAG_ENCODING = 5062
TAG_PAYLOADDIGEST = 5092
TAG_PAYLOADDIGESTALGO = 5093
TAG_PAYLOADDIGESTALT = 5097

# Signature header tags
SIGTAG_SIZE = 1000
SIGTAG_MD5 = 1004
SIGTAG_PAYLOADSIZE = 1007
SIGTAG_SHA1 = 269
SIGTAG_SHA256 = 273

# Dependency flags: RPMSENSE_LESS|GREATER|EQUAL, and PREREQ for scriptlets
SENSE_ANY = 0
SENSE_INTERP = 1 << 8  # RPMSENSE_INTERP: scriptlet interpreter
SENSE_SCRIPT_PRE = 1 << 9
SENSE_SCRIPT_POST = 1 << 10
SENSE_SCRIPT_PREUN = 1 << 11
SENSE_SCRIPT_POSTUN = 1 << 12


# ------------------------------------------------------------------- helpers

def _align(buf: bytearray, boundary: int) -> None:
    pad = (-len(buf)) % boundary
    if pad:
        buf.extend(b"\x00" * pad)


def _pack_index_entry(tag: int, typ: int, offset: int, count: int) -> bytes:
    return struct.pack(">iiii", tag, typ, offset, count)


class HeaderBuilder:
    """Collects typed entries, then serialises an RPM header region."""

    def __init__(self) -> None:
        # (tag, type, payload, count)
        self.entries: list[tuple[int, int, bytes, int]] = []

    def _add(self, tag: int, typ: int, payload: bytes, count: int) -> None:
        self.entries.append((tag, typ, bytes(payload), count))

    def add_string(self, tag: int, value: str) -> None:
        self._add(tag, STRING, value.encode() + b"\x00", 1)

    def add_i18n(self, tag: int, value: str) -> None:
        self._add(tag, I18NSTRING, value.encode() + b"\x00", 1)

    def add_int32(self, tag: int, value: int) -> None:
        self._add(tag, INT32, struct.pack(">i", value), 1)

    def add_int16_array(self, tag: int, values: list[int]) -> None:
        # RPM stores INT16 as signed big-endian. File modes carry S_IFREG
        # (0o100000), which is > 32767, so reinterpret them as negative.
        signed = [v - (1 << 16) if v >= (1 << 15) else v for v in values]
        self._add(tag, INT16, struct.pack(f">{len(signed)}h", *signed), len(signed))

    def add_int32_array(self, tag: int, values: list[int]) -> None:
        # RPM stores INT32 as signed big-endian; mask-based flags such as
        # 0xFFFFFFEF must be reinterpreted as their signed equivalent.
        signed = [v - (1 << 32) if v >= (1 << 31) else v for v in values]
        self._add(tag, INT32, struct.pack(f">{len(signed)}i", *signed), len(signed))

    def add_string_array(self, tag: int, values: list[str]) -> None:
        payload = b"".join(v.encode() + b"\x00" for v in values)
        self._add(tag, STRING_ARRAY, payload, len(values))

    def add_bin(self, tag: int, value: bytes) -> None:
        self._add(tag, BIN, value, len(value))

    def serialize(self, region_tag: int | None = None) -> bytes:
        # RPM requires the index to be sorted by tag, and Fedora's validator
        # also requires the data store to be laid out in that same order (offsets
        # monotonically increasing with tag). Build the store now, in tag order.
        entries = sorted(self.entries, key=lambda e: e[0])
        store = bytearray()
        packed = []
        for tag, typ, payload, count in entries:
            boundary = ALIGNMENT.get(typ, 1)
            _align(store, boundary)
            offset = len(store)
            store.extend(payload)
            packed.append((tag, typ, offset, count))

        if region_tag is not None:
            # Append the region trailer (16 bytes) to the store.
            ril = len(packed) + 1  # include the region tag itself
            trailer_offset_field = -ril * 16
            trailer = struct.pack(">iiii", region_tag, 7, trailer_offset_field, 16)
            _align(store, 1)
            trailer_off = len(store)
            store.extend(trailer)
            # Region tag is the first entry (lowest tag), pointing at trailer.
            packed.insert(0, (region_tag, 7, trailer_off, 16))
            packed.sort(key=lambda e: e[0])

        index = b"".join(_pack_index_entry(*e) for e in packed)
        head = HEADER_MAGIC + b"\x00\x00\x00\x00"
        head += struct.pack(">ii", len(packed), len(store))
        return head + index + bytes(store)


# --------------------------------------------------------------------- cpio

def _cpio_newc(files: list[tuple[str, bytes, int, int]]) -> bytes:
    """Build a "newc" cpio archive: (path, data, mode, mtime) tuples."""
    out = bytearray()
    inode = 1

    for path, data, mode, mtime in files:
        name = path.encode() + b"\x00"
        header = (
            b"070701"
            + f"{inode:08X}".encode()
            + f"{mode:08X}".encode()
            + "00000000".encode()  # uid
            + "00000000".encode()  # gid
            + "00000001".encode()  # nlink
            + f"{mtime:08X}".encode()
            + f"{len(data):08X}".encode()  # filesize
            + "00000000".encode()  # devmajor
            + "00000000".encode()  # devminor
            + "00000000".encode()  # rdevmajor
            + "00000000".encode()  # rdevminor
            + f"{len(name):08X}".encode()
            + "00000000".encode()  # check
        )
        out.extend(header)
        out.extend(name)
        _align(out, 4)
        out.extend(data)
        _align(out, 4)
        inode += 1

    # Trailer entry closes the archive. The newc format stores namesize as one
    # of the 13 header fields; leaving it zero makes readers treat the trailer
    # name as empty and then walk off into the payload ("premature end of
    # file"/"corrupt package" on Fedora). Set it to the real length (including
    # the NUL) and keep every other field zero.
    trailer_name = b"TRAILER!!!\x00"
    fields = [
        0,                # ino
        0,                # mode
        0,                # uid
        0,                # gid
        1,                # nlink
        0,                # mtime
        0,                # filesize
        0, 0, 0, 0,       # devmajor, devminor, rdevmajor, rdevminor
        len(trailer_name),  # namesize
        0,                # check
    ]
    out.extend(b"070701")
    out.extend(b"".join(f"{value:08X}".encode() for value in fields))
    out.extend(trailer_name)
    _align(out, 4)
    return bytes(out)


# ------------------------------------------------------------------ package

@dataclass
class RpmPackage:
    name: str
    version: str
    release: str = "1"
    summary: str = ""
    description: str = ""
    license: str = "Proprietary"
    vendor: str = ""
    packager: str = ""
    url: str = ""
    group: str = "Applications/Internet"
    os_name: str = "linux"
    arch: str = "x86_64"
    requires: list[str] = field(default_factory=list)
    recommends: list[str] = field(default_factory=list)
    provides: list[str] = field(default_factory=list)
    # (absolute path, source file, mode)
    files: list[tuple[str, Path, int]] = field(default_factory=list)
    scriptlets: dict[str, str] = field(default_factory=dict)


def _read_header(data: bytes, offset: int) -> tuple[dict[int, tuple[int, int, int]], int, int, int]:
    """Return (entries, store offset, store size, raw header length)."""
    if data[offset : offset + 4] != HEADER_MAGIC:
        raise ValueError("bad RPM header magic")
    count, size = struct.unpack(">ii", data[offset + 8 : offset + 16])
    index_off = offset + 16
    entries: dict[int, tuple[int, int, int]] = {}
    for i in range(count):
        tag, typ, off, cnt = struct.unpack(">iiii", data[index_off + i * 16 : index_off + (i + 1) * 16])
        entries[tag] = (typ, off, cnt)
    return entries, index_off + count * 16, size, 16 + count * 16 + size


def _entry_bytes(data: bytes, entries: dict[int, tuple[int, int, int]], store: int, tag: int) -> bytes:
    typ, off, count = entries[tag]
    width = {INT16: 2, INT32: 4, INT64: 8}.get(typ)
    if width is None:
        # STRING / STRING_ARRAY / BIN are NUL-delimited or already sized.
        end = data.find(b"\x00", store + off)
        return data[store + off : end if end != -1 else len(data)]
    return data[store + off : store + off + count * width]


def verify_rpm(path: Path) -> None:
    """Re-read the bytes we just wrote and fail loudly on malformed headers.

    This is the check that was missing: a wrong tag type (FILEMODES as INT32)
    produced a package that rpm rejected with `tag[15]: BAD`, long after the
    build had printed a green tick.
    """
    data = path.read_bytes()
    if data[:4] != b"\xed\xab\xee\xdb":
        raise ValueError("bad RPM lead")

    sig_entries, sig_store, sig_size, sig_raw = _read_header(data, 96)
    main_off = 96 + ((sig_raw + 7) // 8) * 8
    main_entries, main_store, main_size, main_raw = _read_header(data, main_off)

    for label, entries, store, size in (
        ("signature", sig_entries, sig_store, sig_size),
        ("main", main_entries, main_store, main_size),
    ):
        if list(entries) != sorted(entries):
            raise ValueError(f"{label} header index is not sorted")
        for tag, (typ, off, count) in entries.items():
            width = {INT16: 2, INT32: 4, INT64: 8}.get(typ, 1)
            if off < 0 or off + count * width > size:
                raise ValueError(f"{label} tag {tag} points outside the store")
        trailer = struct.unpack(">iiii", data[store + size - 16 : store + size])
        region = [tag for tag in entries if tag in (62, 63)][0]
        if trailer != (region, BIN, -len(entries) * 16, 16):
            raise ValueError(f"{label} region trailer is malformed: {trailer}")
        if entries[region][1] != size - 16:
            raise ValueError(f"{label} region entry does not point at the trailer")

    if main_entries.get(TAG_FILEMODES, (None,))[0] != INT16:
        raise ValueError("FILEMODES must be stored as INT16 or rpm rejects the package")

    payload = data[main_off + main_raw :]
    if hashlib.sha256(payload).hexdigest().encode() != _entry_bytes(
        data, main_entries, main_store, TAG_PAYLOADDIGEST
    ):
        raise ValueError("payload digest does not match the compressed payload")

    try:
        cpio = gzip.decompress(payload)
    except OSError as exc:
        raise ValueError(f"payload is not valid gzip: {exc}") from exc
    if hashlib.sha256(cpio).hexdigest().encode() != _entry_bytes(
        data, main_entries, main_store, TAG_PAYLOADDIGESTALT
    ):
        raise ValueError("alternate payload digest does not match the cpio archive")

    if hashlib.sha256(data[main_off : main_off + main_raw]).hexdigest().encode() != _entry_bytes(
        data, sig_entries, sig_store, SIGTAG_SHA256
    ):
        raise ValueError("main-header SHA256 does not match the signature header")


def build_rpm(pkg: RpmPackage, out_path: Path, build_host: str | None = None) -> Path:
    build_time = int(time.time())
    host = build_host or os.uname().nodename

    # ---- payload -----------------------------------------------------
    entries: list[tuple[str, bytes, int, int]] = []
    file_sizes: list[int] = []
    file_modes: list[int] = []
    file_mtimes: list[int] = []
    file_digests: list[str] = []
    file_users: list[str] = []
    file_groups: list[str] = []
    file_flags: list[int] = []
    file_verify: list[int] = []
    file_devices: list[int] = []
    file_inodes: list[int] = []
    file_langs: list[str] = []
    basenames: list[str] = []
    dirnames: list[str] = []
    dirindexes: list[int] = []

    total_size = 0
    seen_dirs: dict[str, int] = {}

    for abs_path, source, mode in sorted(pkg.files, key=lambda f: f[0]):
        data = source.read_bytes() if source.is_file() else b""
        directory = abs_path.rsplit("/", 1)[0] + "/"
        basename = abs_path.rsplit("/", 1)[1]

        if directory not in seen_dirs:
            seen_dirs[directory] = len(dirnames)
            dirnames.append(directory)
        dirindexes.append(seen_dirs[directory])
        basenames.append(basename)

        entries.append(("." + abs_path, data, mode, build_time))
        file_sizes.append(len(data))
        file_modes.append(mode)
        file_mtimes.append(build_time)
        file_digests.append(hashlib.sha256(data).hexdigest())
        file_users.append("root")
        file_groups.append("root")
        file_flags.append(0)
        # RPMVERIFY_ALL minus FILE_MTIME, which we do not want to fight over.
        file_verify.append(0xFFFFFFFF & ~(1 << 4))
        file_devices.append(1)
        file_inodes.append(len(file_inodes) + 1)
        file_langs.append("")
        total_size += len(data)

    # Parent directories are created by rpm while unpacking the file entries;
    # emitting explicit directory entries made rpm fail with
    # "cpio: Archive file not in header" on Fedora."""
    pass

    cpio = _cpio_newc(entries)
    payload = gzip.compress(cpio, compresslevel=9, mtime=0)
    # rpm >= 4.14 verifies the payload through these main-header tags; without
    # them an install reports "package does not verify: no digest".
    payload_digest = hashlib.sha256(payload).hexdigest()
    cpio_digest = hashlib.sha256(cpio).hexdigest()

    # ---- main header -------------------------------------------------
    h = HeaderBuilder()
    h.add_string(TAG_NAME, pkg.name)
    h.add_string(TAG_VERSION, pkg.version)
    h.add_string(TAG_RELEASE, pkg.release)
    h.add_i18n(TAG_SUMMARY, pkg.summary)
    h.add_i18n(TAG_DESCRIPTION, pkg.description)
    h.add_int32(TAG_BUILDTIME, build_time)
    h.add_string(TAG_BUILDHOST, host)
    h.add_int32(TAG_SIZE, total_size)
    h.add_string(TAG_LICENSE, pkg.license)
    h.add_string(TAG_PACKAGER, pkg.packager or pkg.vendor)
    h.add_i18n(TAG_GROUP, pkg.group)
    h.add_string(TAG_URL, pkg.url)
    h.add_string(TAG_OS, pkg.os_name)
    h.add_string(TAG_ARCH, pkg.arch)
    h.add_string(TAG_SOURCERPM, f"{pkg.name}-{pkg.version}-{pkg.release}.src.rpm")
    h.add_string(TAG_RPMVERSION, "4.19.1")
    h.add_string(TAG_PAYLOADFORMAT, "cpio")
    h.add_string(TAG_PAYLOADCOMPRESSOR, "gzip")
    h.add_string(TAG_PAYLOADFLAGS, "9")
    h.add_int32(TAG_FILEDIGESTALGO, 8)  # SHA-256
    h.add_string(TAG_ENCODING, "utf-8")
    h.add_string_array(TAG_PAYLOADDIGEST, [payload_digest])
    h.add_int32(TAG_PAYLOADDIGESTALGO, 8)
    h.add_string_array(TAG_PAYLOADDIGESTALT, [cpio_digest])

    h.add_int32_array(TAG_FILESIZES, file_sizes)
    # rpm's tag type table pins FILEMODES to INT16. Writing it as INT32 makes
    # rpm reject the whole package with "tag[15]: BAD, tag 1030 type 4".
    h.add_int16_array(TAG_FILEMODES, file_modes)
    h.add_int16_array(TAG_FILERDEVS, [0] * len(file_sizes))
    h.add_int32_array(TAG_FILEMTIMES, file_mtimes)
    h.add_string_array(TAG_FILEDIGESTS, file_digests)
    h.add_string_array(TAG_FILELINKTOS, [""] * len(file_sizes))
    h.add_int32_array(TAG_FILEFLAGS, file_flags)
    h.add_string_array(TAG_FILEUSERNAME, file_users)
    h.add_string_array(TAG_FILEGROUPNAME, file_groups)
    h.add_int32_array(TAG_FILEVERIFYFLAGS, file_verify)
    h.add_int32_array(TAG_FILEDEVICES, file_devices)
    h.add_int32_array(TAG_FILEINODES, file_inodes)
    h.add_string_array(TAG_FILELANGS, file_langs)
    h.add_int32_array(TAG_DIRINDEXES, dirindexes)
    h.add_string_array(TAG_BASENAMES, basenames)
    h.add_string_array(TAG_DIRNAMES, dirnames)

    # Scriptlets: rpmbuild stores the body plus the interpreter program. The
    # fallback writer used to drop both, so a direct install skipped the icon
    # cache, desktop database and the service cleanup on removal.
    scriptlet_tags = {
        "pre": (TAG_PREIN, TAG_PREINPROG),
        "post": (TAG_POSTIN, TAG_POSTINPROG),
        "preun": (TAG_PREUN, TAG_PREUNPROG),
        "postun": (TAG_POSTUN, TAG_POSTUNPROG),
    }
    has_scriptlet = False
    for phase, script in pkg.scriptlets.items():
        tags = scriptlet_tags.get(phase)
        if not tags or not script.strip():
            continue
        has_scriptlet = True
        h.add_string(tags[0], script)
        h.add_string(tags[1], "/bin/sh")

    # Requirements: an interpreter dependency for each scriptlet, plus the
    # library packages the binary links against.
    req_names: list[str] = []
    req_versions: list[str] = []
    req_flags: list[int] = []

    if has_scriptlet:
        req_names.append("/bin/sh")
        req_versions.append("")
        req_flags.append(SENSE_INTERP)

    for requirement in pkg.requires:
        if ">=" in requirement:
            name, version = [part.strip() for part in requirement.split(">=", 1)]
            req_names.append(name)
            req_versions.append(version)
            req_flags.append(0x08 | 0x04)  # GREATER | EQUAL
        else:
            req_names.append(requirement.strip())
            req_versions.append("")
            req_flags.append(SENSE_ANY)

    if req_names:
        h.add_string_array(TAG_REQUIRENAME, req_names)
        h.add_string_array(TAG_REQUIREVERSION, req_versions)
        h.add_int32_array(TAG_REQUIREFLAGS, req_flags)

    if pkg.provides:
        h.add_string_array(TAG_PROVIDENAME, pkg.provides)
        h.add_string_array(TAG_PROVIDEVERSION, [f"{pkg.version}-{pkg.release}"] * len(pkg.provides))
        h.add_int32_array(TAG_PROVIDEFLAGS, [0x08 | 0x04] * len(pkg.provides))

    main_header = h.serialize(63)
    body = main_header + payload

    # ---- signature header --------------------------------------------
    s = HeaderBuilder()
    # rpm's "Header SHA1/SHA256 digest" is computed over the main header only,
    # not over header+payload. Hashing the whole body made Fedora report BAD.
    s.add_string(SIGTAG_SHA1, hashlib.sha1(main_header).hexdigest())
    s.add_string(SIGTAG_SHA256, hashlib.sha256(main_header).hexdigest())
    s.add_bin(SIGTAG_MD5, hashlib.md5(body).digest())
    s.add_int32(SIGTAG_SIZE, len(body))
    s.add_int32(SIGTAG_PAYLOADSIZE, len(cpio))
    sig = s.serialize(62)
    # The signature region must be padded to an 8-byte boundary.
    sig += b"\x00" * ((-len(sig)) % 8)

    # ---- lead ---------------------------------------------------------
    lead = bytearray()
    lead += b"\xed\xab\xee\xdb"          # magic
    lead += bytes([4, 0])                # major, minor
    lead += struct.pack(">h", 0)         # type: binary
    lead += struct.pack(">h", 1)         # archnum: x86_64
    name = f"{pkg.name}-{pkg.version}-{pkg.release}".encode()[:65]
    lead += name + b"\x00" * (66 - len(name))
    lead += struct.pack(">h", 1)         # osnum: Linux
    lead += struct.pack(">h", 5)         # signature type: header-style
    lead += b"\x00" * 16                 # reserved
    assert len(lead) == 96, len(lead)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(bytes(lead) + sig + body)
    verify_rpm(out_path)
    return out_path


def build_from_stage(
    stage_root: Path,
    out_path: Path,
    name: str,
    version: str,
    summary: str,
    description: str,
    requires: list[str],
    recommends: list[str] | None = None,
    url: str = "",
    packager: str = "Ramy Badraan",
    license_text: str = "Proprietary",
    scriptlets: dict[str, str] | None = None,
) -> Path:
    """Map a staged tree (root == /) into an RPM, preserving permissions."""
    files: list[tuple[str, Path, int]] = []
    for path in sorted(stage_root.rglob("*")):
        if not path.is_file():
            continue
        relative = "/" + str(path.relative_to(stage_root))
        mode = 0o755 if os.access(path, os.X_OK) else 0o644
        files.append((relative, path, stat.S_IFREG | mode))

    pkg = RpmPackage(
        name=name,
        version=version,
        summary=summary,
        description=description,
        license=license_text,
        packager=packager,
        url=url,
        requires=requires,
        recommends=recommends or [],
        provides=[name, f"{name} = {version}"],
        files=files,
        scriptlets=scriptlets or {},
    )
    return build_rpm(pkg, out_path)


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Build an RPM from a staged tree.")
    parser.add_argument("stage", help="directory that maps to /")
    parser.add_argument("output", help="output .rpm path")
    parser.add_argument("--name", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--summary", default="")
    parser.add_argument("--description", default="")
    parser.add_argument("--require", action="append", default=[])
    parser.add_argument("--url", default="")
    parser.add_argument("--license", dest="license_text", default="Proprietary")
    parser.add_argument("--pre", default="")
    parser.add_argument("--post", default="")
    parser.add_argument("--preun", default="")
    parser.add_argument("--postun", default="")
    args = parser.parse_args()

    scriptlets = {
        phase: script
        for phase, script in {
            "pre": args.pre,
            "post": args.post,
            "preun": args.preun,
            "postun": args.postun,
        }.items()
        if script.strip()
    }

    built = build_from_stage(
        Path(args.stage),
        Path(args.output),
        args.name,
        args.version,
        args.summary,
        args.description,
        args.require,
        url=args.url,
        license_text=args.license_text,
        scriptlets=scriptlets,
    )
    print(f"{built} ({built.stat().st_size:,} bytes)")
