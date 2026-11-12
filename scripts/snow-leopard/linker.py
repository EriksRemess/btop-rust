#!/usr/bin/env python3
"""Link rebuilt Rust std against the real Snow Leopard SDK and stable ld64."""
import os
import sys

args = [a for a in sys.argv[1:] if not a.startswith('-mmacosx-version-min=')]
os.execvp('clang', [
    'clang', '--target=x86_64-apple-macosx10.6', '-march=core2',
    '-isysroot', os.environ['SDKROOT'],
    '--ld-path=' + os.environ['BTOPRS_DARWIN_LINKER'],
    *args, os.environ['BTOPRS_COMPAT_OBJECT'], '-lproc',
    '-mmacosx-version-min=10.6',
])
