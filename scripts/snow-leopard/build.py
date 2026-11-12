#!/usr/bin/env python3
"""Cross-build this branch for x86_64/Core 2/Mac OS X 10.6 from Linux."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--sdk', type=Path, required=True)
parser.add_argument('--tools', type=Path, required=True, help='OSXCross target/bin')
parser.add_argument('--tests', action='store_true')
parser.add_argument('--clippy', action='store_true', help='lint the native target instead of building')
args = parser.parse_args()
version=subprocess.check_output(['rustc','--version'],text=True).split()[1]
if version!='1.99.1':parser.error('This port is validated with Rust 1.99.1 and its matching rust-src component')
root = Path(__file__).resolve().parents[2]
build = root/'target/snow-leopard'
build.mkdir(parents=True, exist_ok=True)
sdk = args.sdk.resolve()
tools = args.tools.resolve()
linker = tools/'x86_64-apple-darwin10-ld'
if not (sdk/'usr/include/mach/mach.h').exists() or not linker.is_file():
    parser.error('Supply the MacOSX10.6 SDK and compatible OSXCross toolchain')
subprocess.run([
    'clang', '--target=x86_64-apple-macosx10.6', '-march=core2', '-isysroot',str(sdk),
    '-std=c99','-Wall','-Wextra','-O2','-fno-builtin-sin','-fno-builtin-cos',
    '-c',str(root/'scripts/snow-leopard/compat.c'),'-o',str(build/'compat.o')
],check=True)
sysroot=Path(subprocess.check_output(['rustc','--print','sysroot'],text=True).strip())
vendor=sysroot/'lib/rustlib/src/rust/library/vendor'
env=os.environ.copy()
env.update(RUSTC_BOOTSTRAP='1',MACOSX_DEPLOYMENT_TARGET='10.6',SDKROOT=str(sdk),
    CARGO_TARGET_DIR=str(build/'cargo'),
    CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=str(root/'scripts/snow-leopard/linker.py'),
    BTOPRS_DARWIN_LINKER=str(linker),BTOPRS_COMPAT_OBJECT=str(build/'compat.o'),
    CARGO_PROFILE_RELEASE_PANIC='abort',
    RUSTFLAGS='-C target-cpu=core2 -C panic=abort -Z has-thread-local=no')
command=['cargo','build','-Zbuild-std=std,panic_abort','--release','--offline',
    '--target','x86_64-apple-darwin','--config','source.crates-io.replace-with="rust-vendor"',
    '--config',f'source.rust-vendor.directory="{vendor}"']
if args.clippy:
    command[1]='clippy'
    command+=['-Zpanic-abort-tests','--all-targets','--','-D','warnings']
    raise SystemExit(subprocess.run(command,cwd=root,env=env).returncode)
with (build/'build.log').open('w') as log:
    result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
if result.returncode:
    print((build/'build.log').read_text()[-16000:])
    raise SystemExit(result.returncode)
exe=build/'cargo/x86_64-apple-darwin/release/btoprs'
headers=subprocess.check_output(['llvm-objdump','--macho','--private-headers',str(exe)],text=True)
assert 'LC_UNIXTHREAD' in headers and 'LC_MAIN' not in headers
assert re.search(r'LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version 10\.6\b',headers)
(build/'load-commands.txt').write_text(headers)
manifest={'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),
    'sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'target':'x86_64-apple-darwin',
    'cpu':'core2','minimum_macos':'10.6'}
(build/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Built:',exe,flush=True)
if args.tests:
    subprocess.run(['clang','--target=x86_64-apple-macosx10.6','-march=core2','-isysroot',str(sdk),
        '--ld-path='+str(linker),'-Wall','-Wextra','-O2',str(root/'scripts/snow-leopard/ui-probe.m'),
        '-framework','Cocoa','-framework','ApplicationServices','-framework','Carbon','-o',str(build/'ui-probe')],check=True)
if args.tests:
    command[1]='test'
    command+=['--no-run','-Zpanic-abort-tests','--message-format=json']
    with (build/'tests.jsonl').open('w') as out,(build/'tests.log').open('w') as err:
        result=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=err)
    if result.returncode:
        print((build/'tests.log').read_text()[-8000:])
        for line in (build/'tests.jsonl').read_text().splitlines():
            record=json.loads(line)
            if record.get('reason')=='compiler-message' and record['message']['level']=='error':
                print(record['message'].get('rendered',''))
        raise SystemExit(result.returncode)
    paths=[]
    for line in (build/'tests.jsonl').read_text().splitlines():
        r=json.loads(line)
        if r.get('reason')=='compiler-artifact' and r.get('profile',{}).get('test') and r.get('executable'):
            paths.append(r['executable'])
    (build/'test-executables.json').write_text(json.dumps(paths,indent=2)+'\n')
    print('Test executables:',*paths,sep='\n')
