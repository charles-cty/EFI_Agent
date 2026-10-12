"""Structured directory inputs and an independent artifact oracle."""
from pathlib import Path
import random


def stage_directory(root: Path):
    directory = root / "directory-fixture"
    directory.mkdir()
    names = [".hidden.txt", "λ.txt", "é.txt", "e\u0301.txt", "space name.txt",
             "[literal].txt", "quo'te.txt", "dollar$.txt", "semi;colon.txt", "x" * 150 + ".txt"]
    generator = random.Random(731)
    for index in range(67):
        length = generator.choice((1, 2, 3, 7, 31, 63, 127, 180))
        text = "".join(generator.choice("abcxyzABCXYZ_-+") for _ in range(length))
        names.append(f"n{index:03}_{text}.dat")
    entries = [(".", 3), ("..", 3)]
    for name in names:
        (directory / name).write_bytes(b"entry\n")
        entries.append((name, 4))
    for name in ("empty", "nested", "folder.txt"):
        (directory / name).mkdir()
        entries.append((name, 3))
    (directory / "nested/inside.dat").write_bytes(b"nested\n")
    return entries


def verify_directory_artifacts(read_file, entries):
    def listing(name):
        records = []
        for line in read_file(name).decode("ascii").splitlines():
            kind, encoded = line.split(":", 1)
            records.append((bytes.fromhex(encoded).decode("utf-8"), int(kind)))
        assert len(records) == len(set(records)), (name, "duplicate entries")
        return set(records)

    initial = set(entries)
    current = initial | {("added.txt", 4)}
    assert listing("directory-list.txt") == initial
    assert listing("directory-current.txt") == current
    assert listing("directory-libc.txt") == current
    assert read_file("directory-fixture/added.txt") == b""


def expected_glob(entries):
    names = {name for name, _ in entries} | {"added.txt"}
    return "".join(f"<directory-fixture/{name}>\n" for name in sorted(names)
                   if name.endswith(".txt") and not name.startswith(".")).encode("utf-8")
