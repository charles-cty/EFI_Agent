"""Check packaged GPT/FAT32 images with independent Linux disk tools.

Run with uv in a Linux-local checkout. Needs sgdisk, fsck.fat, and mtools.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile


def run(*command):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"{command}:\n{result.stdout.decode(errors='replace')}")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("launcher", "efi", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    launcher = str(args.launcher.resolve())
    with tempfile.TemporaryDirectory(prefix="efi-pack-") as temporary:
        tree = Path(temporary) / "tree"
        (tree / "EFI/BOOT").mkdir(parents=True)
        (tree / "EFI/AGENT").mkdir()
        shutil.copyfile(args.efi, tree / "EFI/BOOT/BOOTX64.EFI")
        (tree / "EFI/AGENT/VM.TXT").write_text("serial\n", encoding="utf-8")
        (tree / "work/sub folder").mkdir(parents=True)
        names = ["long source filename.rs", "中文文件.txt", "empty.bin", "中"]
        payloads = [b"first\nsecond\x00\xff", "native 中\n".encode(), b"", b"one"]
        for size in (511, 512, 513, 4095, 4096, 4097):
            names.append(f"boundary-{size}.bin")
            payloads.append(bytes(index % 251 for index in range(size)))
        for name, payload in zip(names, payloads):
            (tree / "work/sub folder" / name).write_bytes(payload)
        image = args.output / "packed.img"
        if image.exists():
            image.unlink()
        run(launcher, "pack", str(tree), str(image))
        validation = run("sgdisk", "-v", str(image))
        if b"No problems found" not in validation:
            raise AssertionError(validation.decode())
        layout = run("sgdisk", "-i", "1", str(image))
        assert b"EFI system partition" in layout and b"First sector: 2048" in layout
        assert image.stat().st_size == 66 * 1024 * 1024
        partition = Path(temporary) / "partition.img"
        with image.open("rb") as source, partition.open("wb") as target:
            source.seek(1024 * 1024)
            remaining = 64 * 1024 * 1024
            while remaining:
                chunk = source.read(min(remaining, 1024 * 1024))
                if not chunk:
                    raise AssertionError("Truncated ESP partition")
                target.write(chunk)
                remaining -= len(chunk)
        filesystem = run("fsck.fat", "-n", str(partition))
        image_spec = f"{image.resolve()}@@1048576"
        assert run("mtype", "-i", image_spec, "::/EFI/BOOT/BOOTX64.EFI") == args.efi.read_bytes()
        assert run("mtype", "-i", image_spec, "::/EFI/AGENT/VM.TXT") == b"serial\n"
        for name, payload in zip(names, payloads):
            assert run("mtype", "-i", image_spec, f"::/work/sub folder/{name}") == payload
        print("PASS GPT checksums, EFI partition, FAT consistency, and exact binary/Unicode/empty files", flush=True)
        before = hashlib.sha256(image.read_bytes()).digest()
        refused = subprocess.run([launcher, "pack", str(tree), str(image)], capture_output=True)
        assert refused.returncode != 0 and hashlib.sha256(image.read_bytes()).digest() == before
        print("PASS existing image is not overwritten", flush=True)

        def refuses(path):
            rejected = Path(temporary) / "rejected.img"
            result = subprocess.run([launcher, "pack", str(tree), str(rejected)], capture_output=True)
            assert result.returncode != 0 and not rejected.exists(), (path, result.stderr)
            path.unlink()

        link = tree / "linked"
        link.symlink_to(args.efi.resolve())
        refuses(link)
        (tree / "case.rs").write_text("lower")
        collision = tree / "CASE.RS"
        collision.write_text("upper")
        refuses(collision)
        (tree / "case.rs").unlink()
        invalid = tree / "bad:name"
        invalid.touch()
        refuses(invalid)
        unsupported = tree / "emoji-🚀.txt"
        unsupported.touch()
        refuses(unsupported)
        large = tree / "large.bin"
        with large.open("wb") as file:
            file.truncate(48 * 1024 * 1024)
        refuses(large)
        nested_output = tree / "nested.img"
        result = subprocess.run([launcher, "pack", str(tree), str(nested_output)], capture_output=True)
        assert result.returncode != 0 and not nested_output.exists()
        print("PASS links, case collisions, invalid names, size overflow, and output inside source are rejected", flush=True)
        (args.output / "validation.txt").write_bytes(validation + layout + filesystem)


if __name__ == "__main__":
    main()
