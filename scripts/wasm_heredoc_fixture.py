"""Independent shell scripts and byte oracles for actual ash here-documents."""


def heredoc_cases():
    cases = []
    for size in [4095, 4096, 4097, 65535, 65536, 65537, 196613]:
        body = bytes(97 + (index * 7 + index // 11) % 26 for index in range(size - 1)) + b"\n"
        script = b"ash_child copy <<'END_DOC'\n" + body + b"END_DOC\n"
        cases.append((f"literal-{size}", script, body, b"", 0))
    for size in [30001, 65537, 196613]:
        body = bytes(97 + (index * 13 + index // 17) % 26 for index in range(size - 1)) + b"\n"
        command = b"ash_child copy <<'END_TASK_DOC'\n" + body + b"END_TASK_DOC\n"
        wrappers = {
            "subshell": b"(\n" + command + b");:\n",
            "pipeline": b"(\n" + command + b") | ash_child copy\n",
            "substitution": b"value=$(\n" + command + b"); printf '%s\\n' \"$value\"\n",
            "function": b"f() {\n" + command + b"}; f | ash_child copy\n",
        }
        for name, script in wrappers.items():
            cases.append((f"task-{name}-{size}", script, body, b"", 0))
    cases.extend([
        ("expansion", b"x='two words'; unset y\nash_child copy <<EOF\n$x\n${y:=changed}\n$(printf 'substitute\\n')\n\\$x\nEOF\nprintf '%s\\n' \"$y\"\n",
         b"two words\nchanged\nsubstitute\n$x\nchanged\n", b"", 0),
        ("quoted", b"ash_child copy <<'EOF'\n$x $(echo forbidden) `echo forbidden` $((7+2)) \\literal\nEOF\n",
         b"$x $(echo forbidden) `echo forbidden` $((7+2)) \\literal\n", b"", 0),
        ("tab-strip", b"ash_child copy <<-'EOF'\n\t\tleading\n \tspace\ninside\ttab\n\tEOF\n",
         b"leading\n \tspace\ninside\ttab\n", b"", 0),
        ("multiple", b"ash_child copy <<FIRST <<SECOND\n$(echo first >&2)unused\nFIRST\n$(echo second >&2)active\nSECOND\n",
         b"active\n", b"first\nsecond\n", 0),
        ("pipeline", b"ash_child copy <<EOF | ash_child copy\npipe words\nEOF\n",
         b"pipe words\n", b"", 0),
        ("repeat", b"for i in " + b"x " * 300 + b"; do\nread value <<EOF\nline\nEOF\ncase $value in line) :;; *) exit 93;; esac\ndone\necho HEREDOC_REPEAT_OK\n",
         b"HEREDOC_REPEAT_OK\n", b"", 0),
        ("overflow", b"x=abcdefgh; for i in " + b"x " * 16 + b"; do x=\"$x$x\"; done\nread value <<EOF\n$x\nEOF\n",
         b"", b"", 1),
    ])
    return cases


def stage_heredocs(directory):
    for name, script, *_ in heredoc_cases():
        (directory / f"heredoc-{name}.sh").write_bytes(script)
