"""Shell descriptor inheritance cases with independent exact byte results."""


def descriptor_cases():
    return [
        ("subshell", "printf 'first\\nsecond\\n'>fd-input.txt;exec 7<fd-input.txt;(read -u 7 value;echo child:$value);read -u 7 value;echo parent:$value\n",
         b"child:first\nparent:second\n"),
        ("fd4", "printf 'four\\n'>fd-input.txt;exec 4<fd-input.txt;(read -u 4 value;echo $value);:\n", b"four\n"),
        ("substitution", "printf 'first\\nsecond\\n'>fd-input.txt;exec 7<fd-input.txt;value=$(read -u 7 line;echo $line);read -u 7 line;echo $value:$line\n", b"first:second\n"),
        ("pipeline", "printf 'pipe\\n'>fd-input.txt;exec 7<fd-input.txt;(read -u 7 value;echo $value)|ash_child copy\n", b"pipe\n"),
        ("external", "printf 'external\\n'>fd-input.txt;exec 7<fd-input.txt;ash_child copy_fd 7\n", b"external\n"),
        ("nested", "printf 'nested\\n'>fd-input.txt;exec 4<fd-input.txt;((read -u 4 value;echo $value);:);:\n", b"nested\n"),
        ("output", "exec 7>fd-output.txt;(echo child >&7);echo parent >&7;exec 7>&-;ash_child copy<fd-output.txt\n", b"child\nparent\n"),
        ("close-child", "printf 'kept\\n'>fd-input.txt;exec 7<fd-input.txt;(exec 7<&-);read -u 7 value;echo $value\n", b"kept\n"),
        ("closed-stdin", "exec 0<&-;(echo NO_STDIN);:\n", b"NO_STDIN\n"),
        ("repeat", "printf 'repeat\\n'>fd-input.txt;i=0;while [ $i -lt 300 ];do exec 4<fd-input.txt;(read -u 4 value;case $value in repeat) :;; *) exit 91;; esac);exec 4<&-;i=$((i+1));done;echo FD_REPEAT_OK\n", b"FD_REPEAT_OK\n"),
    ]


def stage_descriptors(directory):
    for name, script, _ in descriptor_cases():
        (directory / f"fd-{name}.sh").write_bytes(script.encode("utf-8"))
