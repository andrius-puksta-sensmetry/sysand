#!/usr/bin/env python3
# SPDX-FileCopyrightText: © 2025 Sysand contributors <opensource@sensmetry.com>
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# maturin incorrectly determines that the sysand
# binary depends on pyo3:
# https://github.com/PyO3/maturin/issues/876
# and thus tags the wheel as depending on a specific python
# version.
# This script fixes the wheel produced by maturin to work
# on any python/pypy version freethreaded or not.

import zipfile
import os
import sys
import hashlib
import base64
import shutil
import tempfile
from pathlib import Path


def urlsafe_b64encode_nopad(data):
    """Standard PEP 427 base64 encoding for wheel hashes (no padding)."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def universalize_wheel(wheel_path_str):
    wheel_path = Path(wheel_path_str).resolve()

    if wheel_path.suffix != ".whl":
        raise ValueError(f"'{wheel_path.name}' is not a .whl file.")

    # 1. Filename Parsing
    # Expected: {dist}-{version}-{python}-{abi}-{platform}.whl
    parts = wheel_path.stem.split("-")
    if len(parts) != 5:
        raise ValueError(
            f"Expected 5 parts in filename, found {len(parts)}. Check for build tags."
        )

    dist, version, _, _, platform = parts
    new_filename = f"{dist}-{version}-py3-none-{platform}.whl"
    new_path = wheel_path.parent / new_filename

    print(f"Processing: {wheel_path.name}")
    print(f"Target:     {new_filename}")

    # 2. Setup Temporary Workspace
    # Creating a temp file in the same directory ensures we can move/rename atomically.
    fd, temp_ptr = tempfile.mkstemp(suffix=".whl", dir=str(wheel_path.parent))
    os.close(fd)
    temp_path = Path(temp_ptr)

    try:
        with zipfile.ZipFile(wheel_path, "r") as zin, zipfile.ZipFile(
            temp_path, "w", compression=zipfile.ZIP_DEFLATED
        ) as zout:
            namelist = zin.namelist()
            # Find the WHEEL and RECORD files inside the .dist-info directory
            wheel_meta_path = next(
                (n for n in namelist if n.endswith(".dist-info/WHEEL")), None
            )
            record_path = next(
                (n for n in namelist if n.endswith(".dist-info/RECORD")), None
            )

            if not wheel_meta_path or not record_path:
                raise RuntimeError(
                    "Required metadata (WHEEL or RECORD) not found in archive."
                )

            updated_wheel_bytes = b""

            # Copy all files to the new archive
            for item in zin.infolist():
                if item.filename == record_path:
                    continue  # We will manually reconstruct the RECORD file last

                content = zin.read(item.filename)

                # Edit the WHEEL metadata content
                if item.filename == wheel_meta_path:
                    lines = content.decode("utf-8").splitlines()
                    new_lines = []
                    for line in lines:
                        if line.startswith("Tag:"):
                            new_lines.append(f"Tag: py3-none-{platform}")
                        else:
                            new_lines.append(line)
                    content = "\n".join(new_lines).encode("utf-8")
                    updated_wheel_bytes = content

                zout.writestr(item, content)

            # 3. Reconstruct the RECORD file to include the updated files
            record_lines = zin.read(record_path).decode("utf-8").splitlines()
            new_record_content = []

            # Calculate new hash/size for the modified WHEEL file
            new_hash = urlsafe_b64encode_nopad(
                hashlib.sha256(updated_wheel_bytes).digest()
            )
            new_size = len(updated_wheel_bytes)

            for line in record_lines:
                # RECORD lines are: path,hash,size
                fields = line.split(",")
                if fields[0] == wheel_meta_path:
                    new_record_content.append(
                        f"{wheel_meta_path},sha256={new_hash},{new_size}"
                    )
                else:
                    new_record_content.append(line)

            zout.writestr(record_path, "\n".join(new_record_content))

        # 4. Finalize: Replace original or create new
        if wheel_path.exists():
            wheel_path.unlink()
        shutil.move(str(temp_path), str(new_path))
        print("Success: wheel is now Python-agnostic.")

    except Exception:
        if temp_path.exists():
            temp_path.unlink()
        raise


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python universal_wheel.py <path_to_wheel>", file=sys.stderr)
        sys.exit(1)
    universalize_wheel(sys.argv[1])
