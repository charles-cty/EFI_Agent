# /// script
# requires-python = ">=3.11"
# ///
"""Run installed jq through the real WASM CLI, files, pipes and subprocess ABI."""
import argparse
import json
from pathlib import Path
import random
import subprocess
import tempfile
import unittest


class JqIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix='efi-jq-')
        cls.work = Path(cls.temporary.name) / 'work'
        cls.work.mkdir()
        (cls.work / 'input.json').write_text(
            json.dumps({'items': [{'name': 'alpha', 'value': 3},
                                  {'name': 'βeta', 'value': -7},
                                  {'name': 'last', 'value': 0}]}), encoding='utf-8')
        (cls.work / 'invalid.json').write_text('{"a":', encoding='utf-8')
        filters = cls.work / 'filters folder'
        filters.mkdir()
        (filters / 'ops.jq').write_text('def positive: select(.value > 0);\n', encoding='utf-8')
        (filters / 'main.jq').write_text(
            'include "ops"; [.items[] | positive | .name]\n', encoding='utf-8')
        generator = random.Random(871)
        cls.records = [{'index': index, 'value': generator.randrange(-1000, 1001),
                        'text': 'αβλ=' + 'x' * (index % 79)} for index in range(2003)]
        (cls.work / 'records.jsonl').write_text(
            ''.join(json.dumps(row, ensure_ascii=False) + '\n' for row in cls.records),
            encoding='utf-8')
        subprocess.run([str(cls.options.launcher), 'install', str(cls.options.prepared),
                        str(cls.work), '--guest'], check=True, timeout=120)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def run_guest(self, *arguments):
        result = subprocess.run([str(self.options.runner), str(self.work), *arguments],
                                capture_output=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
        self.assertEqual(result.stderr, b'')
        return result.stdout

    def test_version_alias_and_explicit_path(self):
        for command in ['jq', '/bin/jq', './jq.wasm']:
            self.assertEqual(self.run_guest(command, '--version'), b'jq-1.8.1\n')

    def test_filters_arguments_and_unicode_regex(self):
        result = self.run_guest('jq', '-c', '--arg', 'label', '测试=λ',
                                '{label:$label,names:[.items[]|select(.value>=0)|.name]}',
                                '/input.json')
        self.assertEqual(json.loads(result), {'label': '测试=λ', 'names': ['alpha', 'last']})
        result = self.run_guest('jq', '-cn',
                                '[("αβ42"|capture("^(?<letters>\\\\p{L}+)(?<number>[0-9]+)$")),'
                                '("a12 b34"|gsub("[0-9]+";"#"))]')
        self.assertEqual(json.loads(result), [{'letters': 'αβ', 'number': '42'}, 'a# b#'])

    def test_relative_files_modules_and_pipeline(self):
        result = self.run_guest('ash', '-c',
                                'cd "/filters folder" && jq -L . -cf main.jq ../input.json')
        self.assertEqual(json.loads(result), ['alpha'])
        self.assertEqual(self.run_guest('ash', '-c',
                                        'printf \'{"n":7}\\n{"n":-2}\\n\' | jq -s \'map(.n)|add\''),
                         b'5\n')

    def test_large_stream_against_independent_oracle(self):
        result = self.run_guest('jq', '-cs', '.', '/records.jsonl')
        self.assertEqual(json.loads(result), self.records)
        result = self.run_guest('ash', '-c',
                                "cat /records.jsonl | jq -s 'map(select(.value>=0)|.value)|add'")
        self.assertEqual(json.loads(result), sum(row['value'] for row in self.records
                                                if row['value'] >= 0))

    def test_exit_statuses_and_process_capture(self):
        source = r'''
import json, subprocess
cases = [(['-en', '0'], 0), (['-en', 'false'], 1), (['-en', 'null'], 1),
         (['-en', 'empty'], 4), (['-n', '. +'], 3), (['.', '/invalid.json'], 5)]
for arguments, expected in cases:
    result = subprocess.run(['jq'] + arguments, capture_output=True)
    assert result.returncode == expected, (arguments, result)
    if expected in (3, 5):
        assert result.stderr, (arguments, result)
result = subprocess.run(['jq', '-c', '.items[1]'], input='{"items":[1,"λ"]}'.encode(),
                        capture_output=True)
assert (result.returncode, result.stdout, result.stderr) == (0, '"λ"\n'.encode(), b''), result
print('JQ_STATUS_AND_CAPTURE_OK')
'''
        self.assertEqual(self.run_guest('python', '-c', source), b'JQ_STATUS_AND_CAPTURE_OK\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('runner', 'launcher', 'prepared'):
        parser.add_argument('--' + name, type=Path, required=True)
    args, remaining = parser.parse_known_args()
    for name in ('runner', 'launcher', 'prepared'):
        setattr(args, name, getattr(args, name).resolve())
    JqIntegration.options = args
    unittest.main(argv=['test_jq', *remaining])


if __name__ == '__main__':
    main()
