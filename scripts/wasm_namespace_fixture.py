"""Independent readers for committed logical workspace bytes."""
import json
import struct
import zlib


class Workspace:
    def __init__(self):
        assert self.raw('.efi-agent-fs/format') == b'EFI Agent filesystem 1\n'
        generations = []
        for slot in range(2):
            try:
                data = self.raw(f'.efi-agent-fs/state{slot}')
            except FileNotFoundError:
                continue
            if len(data) < 24 or data[:8] != b'EFIFS001':
                continue
            generation, length, crc = struct.unpack('<QII', data[8:24])
            if not generation or length != len(data) - 24 or zlib.crc32(data[24:]) != crc:
                continue
            generations.append((generation, json.loads(data[24:])))
        assert generations, 'No valid committed filesystem generation'
        assert len({generation for generation, _ in generations}) == len(generations)
        state = max(generations, key=lambda item: item[0])[1]
        nodes = state['nodes']
        assert nodes['1']['parent'] is None
        self.entries = {}
        for inode, node in nodes.items():
            if inode == '1' or node['parent'] is None:
                continue
            parts = []
            seen = set()
            current = inode
            while current != '1':
                assert current not in seen, 'Cyclic directory parent graph'
                seen.add(current)
                entry = nodes[current]
                name = entry['name']
                assert name and name not in ('.', '..') and '/' not in name and '\\' not in name
                parts.append(name)
                current = str(entry['parent'])
            path = '/'.join(reversed(parts))
            assert path not in self.entries
            self.entries[path] = node

    def read(self, path):
        node = self.entries[path]
        assert node['kind'] == 'File', (path, node)
        return self.raw(node['backing'])


class FatWorkspace(Workspace):
    def __init__(self, filesystem):
        self.filesystem = filesystem
        super().__init__()

    def raw(self, path):
        return self.filesystem.get('/work/' + path).open().read()


class DirectoryWorkspace(Workspace):
    def __init__(self, directory):
        self.directory = directory
        super().__init__()

    def raw(self, path):
        parts = path.split('/')
        assert parts and all(part not in ('', '.', '..') for part in parts)
        assert '\\' not in path and '\0' not in path
        return self.directory.joinpath(*parts).read_bytes()
