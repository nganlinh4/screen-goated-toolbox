"""Execute the production C primitive with a host permission/descriptor model."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--cc', default='cc')
args = parser.parse_args()
compiler = shutil.which(args.cc)
if compiler is None:
    raise SystemExit('A host C compiler is required')
root = Path(__file__).resolve().parents[5]
native = Path(__file__).resolve().parents[1]
fixture = json.loads((root / 'parity-fixtures/history-ui/namespace-access.json').read_text(encoding='utf-8'))
assert fixture['schema_version'] == 1
keys = ['directory_read', 'directory_search', 'directory_write', 'file_read', 'file_write',
        'expected_directory_open', 'expected_file_open', 'expected_create', 'expected_directory_sync', 'expected_writable']
lines = ['static const struct contract_case contract_cases[] = {']
for case in fixture['cases']:
    assert all(isinstance(case[key], bool) for key in keys)
    lines.append('    {' + json.dumps(case['name']) + ', ' + ', '.join(str(int(case[k])) for k in keys) + '},')
lines.append('};')
temporary_parent = Path(tempfile.gettempdir()).resolve()
with tempfile.TemporaryDirectory(prefix='sgt-namespace-contract-', dir=temporary_parent) as scratch:
    output = Path(scratch).resolve()
    assert output.parent == temporary_parent and output.name.startswith('sgt-namespace-contract-')
    (output / 'namespace_cases.h').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    executable = output / ('namespace_contract.exe' if os.name == 'nt' else 'namespace_contract')
    command = [compiler, '-std=c11', '-D_GNU_SOURCE', '-Wall', '-Wextra', '-Werror',
               '-Wconversion', '-Wshadow', '-Wstrict-prototypes']
    primitives = ['open', 'openat', 'close', 'fstat', 'fstatat', 'unlinkat', 'fsync', 'flock', 'mkdirat', 'syscall']
    command += ['-D' + name + '=test_' + name for name in primitives]
    command += ['-I' + str(native / 'tests/host/include'), '-I' + str(output), '-I' + str(native),
                str(native / 'namespace.c'), str(native / 'tests/host/namespace_contract_test.c'), '-o', str(executable)]
    subprocess.run(command, check=True)
    result = subprocess.run([str(executable)], check=False)
raise SystemExit(result.returncode)
