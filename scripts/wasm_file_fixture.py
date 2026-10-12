"""Independent final contents for the guest's structured resize workload."""


def resize_contents():
    content = bytearray()
    for index in range(71):
        size = (index * 997 + 13) % 9000
        if size < len(content):
            del content[size:]
        else:
            content.extend(bytes(size - len(content)))
        content[(index * 71 + 3) % size] = (index * 17 + 1) % 256
    return bytes(content)
