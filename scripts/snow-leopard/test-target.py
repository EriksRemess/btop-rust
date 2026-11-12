#!/usr/bin/env python3
"""Run on Snow Leopard with Python 3.13; all writable fixtures are isolated."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--tests',type=Path,required=True)
parser.add_argument('--ui-probe',type=Path,required=True)
parser.add_argument('--snow-ghostty',type=Path,default=Path('/Applications/Snow Ghostty.app/Contents/MacOS/snow-ghostty'))
args=parser.parse_args()
work=Path(tempfile.mkdtemp(prefix='btoprs-imac-validation-',dir='/tmp'))
(work/'tmp').mkdir()
env=os.environ.copy()
env.update(HOME=str(work),XDG_CONFIG_HOME=str(work/'config'),TMPDIR=str(work/'tmp'),TERM='xterm-ghostty',LANG='en_US.UTF-8')
print('TEST_WORK='+str(work),flush=True)
results={}
def run(name,argv,timeout=180):
    with (work/(name+'.log')).open('w') as log:
        p=subprocess.run(list(map(str,argv)),env=env,stdout=log,stderr=subprocess.STDOUT,timeout=timeout)
    results[name]='PASS' if p.returncode==0 else 'FAIL'
    print(name+': '+results[name],flush=True)
    if p.returncode:raise RuntimeError((work/(name+'.log')).read_text())
run('unit',[args.tests.resolve(),'--test-threads=1'])
run('live',[args.tests.resolve(),'collect::macos::tests::live_collector_returns_core_system_data','--ignored','--nocapture','--test-threads=1'])
run('ports',[args.tests.resolve(),'live_refresh_releases_host_ports','--ignored','--test-threads=1'])
run('diagnostics',[args.binary.resolve(),'--diagnostics'])
for name in ['run-monitor.py','btoprs.conf']:
    shutil.copy2(Path(__file__).parent/name,work/('btop.conf' if name=='btoprs.conf' else name))
(work/'snow.conf').write_text('font-family = FiraCode Nerd Font Mono\nfont-size = 12\n')
run('snow-ghostty-ui',[args.ui_probe.resolve(),args.snow_ghostty.resolve(),args.binary.resolve(),work],60)
result=json.loads((work/'result.json').read_text())
assert result['exit_status']==0 and result['terminal_restored'],result
report={'sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'results':results,'ui':result}
(work/'validation.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2),flush=True)
