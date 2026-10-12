"""Independent byte oracles for actual ash arithmetic and test builtins."""
import random


def math_cases():
    cases = [
        ("precedence", "printf '%s\\n' $((7+3*5)) $(((7+3)*5)) $((2**10)) $((16#ff+2#101)) $((077+0x10))\n",
         b"22\n50\n1024\n260\n79\n", b"", 0),
        ("wide", "printf '%s\\n' $((2147483647+1)) $((4294967296*3+7)) $((-9223372036854775807-1)) $((9223372036854775807))\n",
         b"2147483648\n12884901895\n-9223372036854775808\n9223372036854775807\n", b"", 0),
        ("wrap", "printf '%s\\n' $((9223372036854775807+1)) $((-9223372036854775807-2)) $(((-9223372036854775807-1)/-1)) $(((-9223372036854775807-1)%-1)) $((4294967296*4294967296))\n",
         b"-9223372036854775808\n9223372036854775807\n-9223372036854775808\n0\n0\n", b"", 0),
        ("assignments", "x=7; printf '%s\\n' $((x+=4)) $((x++)) $((++x)) $((x*=3)) $((x>>=1)); echo $x; unset u; echo $((u+9)); a='b+2'; b=5; echo $((a*3))\n",
         b"11\n11\n13\n39\n19\n19\n9\n21\n", b"", 0),
        ("short-circuit", "x=0; printf '%s\\n' $((0 && (x=7))) $((1 || (x=8))) $((1 ? 19 : 1/0)) $((0 ? 1/0 : 23)); echo $x\n",
         b"0\n1\n19\n23\n0\n", b"", 0),
        ("nested-short-circuit", "x=0; printf '%s\\n' $((0 && (1 || (x=7)))) $((1 || (0 ? (x=8) : (x=9)))) $((0 && 1/0 || 17)) $((1 && (x=11))) $((0 || (x=13))) $((0 ? (1 || (x=15)) : 19)); echo $x\n",
         b"0\n1\n1\n1\n1\n19\n13\n", b"", 0),
        ("loop", "i=0; sum=0; while [ $i -lt 21 ]; do sum=$((sum+i*i)); i=$((i+1)); done; echo $sum; [ $sum -eq 2870 ]\n",
         b"2870\n", b"", 0),
        ("test-strings", "test ''; echo $?; test '!'; echo $?; [ -n 'two words' ]; echo $?; [ -z '' ]; echo $?; [ '(' = '(' ]; echo $?; [ one != two ]; echo $?; [ ! -n '' ]; echo $?\n",
         b"1\n0\n0\n0\n0\n0\n0\n", b"", 0),
        ("test-wide", "[ 9223372036854775807 -gt 2147483648 ]; echo $?; [ -9223372036854775808 -lt -2147483649 ]; echo $?; [ 4294967296 -eq 0 ]; echo $?; [ 19 -ge 19 ]; echo $?; [ 19 -le 18 ]; echo $?\n",
         b"0\n0\n1\n0\n1\n", b"", 0),
        ("test-files", "[ -f math-file.txt ]; echo $?; [ -d math-dir ]; echo $?; [ -s math-file.txt ]; echo $?; [ -s math-empty.txt ]; echo $?; [ -e missing-file ]; echo $?; [ -f math-dir ]; echo $?; [ -d math-file.txt ]; echo $?\n",
         b"0\n0\n0\n1\n1\n1\n1\n", b"", 0),
        ("task-state", "x=7; (x=$((x+10)); [ $x -eq 17 ] && echo child:$x); echo parent:$x; value=$(echo $((x*6))); echo substitute:$value; printf '%s\\n' $((x+2)) | ash_child copy\n",
         b"child:17\nparent:7\nsubstitute:42\n9\n", b"", 0),
        ("test-recovery", "i=0; while [ $i -lt 300 ]; do test invalid -eq 1 2>math-errors.txt; [ $? -eq 2 ] || exit 93; [ 7 -eq 7 ] || exit 94; i=$((i+1)); done; echo TEST_RECOVERY_OK\n",
         b"TEST_RECOVERY_OK\n", b"", 0),
        ("divide-zero", "echo $((7/0)); echo forbidden\n", b"", b"divide by zero", 2),
        ("recursion", "a=b; b=a; echo $((a)); echo forbidden\n", b"", b"recursion loop", 2),
        ("syntax", "echo $((7+)); echo forbidden\n", b"", b"arithmetic syntax", 2),
        ("readonly", "readonly x=7; echo $((x=9)); echo forbidden\n", b"", b"read only", 2),
        ("test-overflow", "test 9223372036854775808 -eq 0; echo status:$?; test -9223372036854775809 -eq 0; echo status:$?; [ 1 -eq 1 ]; echo recovered:$?\n",
         b"status:2\nstatus:2\nrecovered:0\n", b"out of range", 0),
        ("test-bracket", "[ 1 -eq 1; echo status:$?; [ 1 -eq 1 ]; echo recovered:$?\n",
         b"status:2\nrecovered:0\n", b"missing ]", 0),
    ]
    generator = random.Random(71203)
    expressions = []
    expected = []
    for _ in range(80):
        a, b, c = [generator.randint(-10000, 10000) for _ in range(3)]
        expressions.append(f"$((({a})*({b})+({c})))")
        expected.append(str(a * b + c))
    cases.append(("structured-random", "printf '%s\\n' " + " ".join(expressions) + "\n",
                  ("\n".join(expected) + "\n").encode(), b"", 0))
    # Derive side effects from an independent recursive tree evaluator rather
    # than from BusyBox's stack/precedence parser. Skipped writes change the
    # state if wrongly evaluated; explicit cases above cover skipped division.
    def generate(depth):
        if depth == 0:
            return generator.choice([("constant", 0), ("constant", 1),
                                     ("constant", 7), ("assign", 3), ("assign", 11)])
        kind = generator.choice(["and", "or", "conditional"])
        if kind == "conditional":
            return (kind, generate(depth - 1), generate(depth - 1), generate(depth - 1))
        return (kind, generate(depth - 1), generate(depth - 1))

    def render(node):
        kind = node[0]
        if kind == "constant":
            return str(node[1])
        if kind == "assign":
            return f"(x+={node[1]})"
        if kind == "conditional":
            return f"({render(node[1])}?{render(node[2])}:{render(node[3])})"
        operator = "&&" if kind == "and" else "||"
        return f"({render(node[1])}{operator}{render(node[2])})"

    def evaluate(node, state):
        kind = node[0]
        if kind == "constant":
            return node[1]
        if kind == "assign":
            state[0] += node[1]
            return state[0]
        left = evaluate(node[1], state)
        if kind == "conditional":
            return evaluate(node[2] if left else node[3], state)
        if kind == "and":
            return int(bool(left) and bool(evaluate(node[2], state)))
        return int(bool(left) or bool(evaluate(node[2], state)))

    scripts, outputs = [], []
    for _ in range(60):
        tree = generate(3)
        state = [0]
        value = evaluate(tree, state)
        scripts.append(f"x=0; result=$(({render(tree)})); echo $result:$x")
        outputs.append(f"{value}:{state[0]}")
    cases.append(("random-short-circuit", "\n".join(scripts) + "\n",
                  ("\n".join(outputs) + "\n").encode(), b"", 0))
    return cases


def stage_math(directory):
    (directory / "math-dir").mkdir(exist_ok=True)
    (directory / "math-file.txt").write_bytes(b"independent-data\n")
    (directory / "math-empty.txt").write_bytes(b"")
    for name, script, *_ in math_cases():
        (directory / f"math-{name}.sh").write_bytes(script.encode("utf-8"))
