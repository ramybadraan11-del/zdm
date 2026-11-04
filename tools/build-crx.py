#!/usr/bin/env python3
"""Build a signed CRX3 package for the ZDM browser extension.

Developed by Ramy Badraan

Why this exists
---------------
Chromium will not auto-install an unpacked folder. The supported paths are:

  * the Web Store (needs a developer account and review), or
  * an enterprise "external extension" entry pointing at a **CRX3** file that is
    signed with the same key whose hash produced the extension id.

The previous implementation wrote `{"external_path": "..."}` which is not a
Chromium field at all, so nothing ever auto-installed. This script produces the
CRX3 file that pairs with the `external_crx` + `external_version` manifests the
installers write.

CRX3 format (as implemented by Chromium):
    "Cr24" | version(4 LE) | header_len(4 LE) | CrxFileHeader(protobuf) | zip

Only the fields Chromium actually verifies are emitted:
    CrxFileHeader {
      repeated AsymmetricKeyProof sha256_with_rsa = 2;
      bytes signed_header_data = 10000;
    }
    AsymmetricKeyProof { bytes public_key = 1; bytes signature = 2; }
    SignedData { bytes crx_id = 1; }
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import io
import json
import os
import struct
import sys
import zipfile
from pathlib import Path

try:
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import padding, rsa
except ImportError:  # pragma: no cover
    print("This tool needs the 'cryptography' package (pip install cryptography).", file=sys.stderr)
    sys.exit(2)

# ----------------------------------------------------------------- protobuf

def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def field_bytes(number: int, payload: bytes) -> bytes:
    return varint((number << 3) | 2) + varint(len(payload)) + payload


def extension_id_hex(public_key_der: bytes) -> bytes:
    """The 16 raw bytes Chromium uses as the extension id.

    The id is SHA-256(public key) truncated to 16 bytes, rendered with each
    nibble mapped 0-f -> a-p. `SignedData.crx_id` stores those raw bytes, not the
    letters, so the nibbles must be packed two per byte.
    """
    return hashlib.sha256(public_key_der).digest()[:16]


def extension_id(public_key_der: bytes) -> str:
    """Registry/host form of the id: 32 lowercase letters a-p."""
    return "".join(
        chr(ord("a") + int(char, 16)) for char in extension_id_hex(public_key_der).hex()
    )


# --------------------------------------------------------------------- keys

def load_or_create_key(path: Path) -> rsa.RSAPrivateKey:
    if path.exists():
        data = path.read_bytes()
        if b"BEGIN" in data:
            return serialization.load_pem_private_key(data, None)
        return serialization.load_der_private_key(data, None)

    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.PKCS8,
            encryption_algorithm=serialization.NoEncryption(),
        )
    )
    os.chmod(path, 0o600)
    print(f"  generated a new signing key: {path}")
    return key


# ---------------------------------------------------------------------- crx

def build_zip(source: Path) -> bytes:
    """Zip the extension with the manifest first (Chromium prefers it)."""
    buffer = io.BytesIO()
    files: list[Path] = []
    for root, _dirs, names in os.walk(source):
        for name in names:
            path = Path(root) / name
            relative = path.relative_to(source)
            if relative.parts and relative.parts[0] in {"node_modules", ".git"}:
                continue
            if name.endswith((".crx", ".pem", ".zip")):
                continue
            files.append(path)

    files.sort(key=lambda p: (p.name != "manifest.json", str(p)))
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files:
            archive.write(path, path.relative_to(source).as_posix())
    return buffer.getvalue()


def build_crx(zip_bytes: bytes, key: rsa.RSAPrivateKey) -> tuple[bytes, str]:
    public_der = key.public_key().public_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    crx_id_bytes = extension_id_hex(public_der)

    # SignedData { crx_id } — the id Chromium checks against the manifest key.
    signed_header_data = field_bytes(1, crx_id_bytes)

    # The signature covers: "CRX3 SignedData\x00" | len(LE u32) | signed_header_data | zip
    prefix = b"CRX3 SignedData\x00"
    signed_payload = (
        prefix + struct.pack("<I", len(signed_header_data)) + signed_header_data + zip_bytes
    )
    signature = key.sign(signed_payload, padding.PKCS1v15(), hashes.SHA256())

    proof = field_bytes(1, public_der) + field_bytes(2, signature)
    header = field_bytes(2, proof) + field_bytes(10000, signed_header_data)

    crx = b"Cr24" + struct.pack("<I", 3) + struct.pack("<I", len(header)) + header + zip_bytes
    return crx, extension_id(public_key_der=public_der)


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a signed CRX3 for the ZDM extension.")
    parser.add_argument("--source", default="apps/extension", help="extension folder")
    parser.add_argument("--output", default="assets/zdm.crx", help="destination .crx")
    parser.add_argument("--key", default="packaging/keys/zdm-extension.pem", help="signing key")
    parser.add_argument("--xpi", default="assets/zdm@zyzoom.com.xpi", help="Firefox package")
    parser.add_argument("--manifest", default="apps/extension/manifest.json")
    args = parser.parse_args()

    source = Path(args.source)
    if not (source / "manifest.json").exists():
        print(f"No manifest.json in {source}", file=sys.stderr)
        return 1

    manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    declared_key = manifest.get("key")
    if not declared_key:
        print("manifest.json has no 'key': the extension id would be unstable.", file=sys.stderr)
        return 1

    key = load_or_create_key(Path(args.key))
    public_der = key.public_key().public_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )

    # The manifest key and the signing key must agree, otherwise Chromium
    # installs the CRX under a different id than the native host allows.
    if base64.b64decode(declared_key) != public_der:
        print(
            "The manifest 'key' does not match the signing key.\n"
            f"  manifest id: {extension_id(base64.b64decode(declared_key))}\n"
            f"  signing  id: {extension_id(public_der)}",
            file=sys.stderr,
        )
        return 1

    zip_bytes = build_zip(source)
    crx_bytes, crx_id = build_crx(zip_bytes, key)

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(crx_bytes)

    # Firefox wants a plain zip named .xpi with a stable gecko id.
    xpi = Path(args.xpi)
    xpi.parent.mkdir(parents=True, exist_ok=True)
    xpi.write_bytes(zip_bytes)

    # The same CRX under its id as filename: Chromium's external_crx path.
    id_named = output.parent / f"{crx_id}.crx"
    if id_named != output:
        id_named.write_bytes(crx_bytes)

    print(f"  extension id : {crx_id}")
    print(f"  crx          : {output} ({len(crx_bytes):,} bytes)")
    print(f"  crx by id    : {id_named}")
    print(f"  firefox xpi  : {xpi} ({len(zip_bytes):,} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
