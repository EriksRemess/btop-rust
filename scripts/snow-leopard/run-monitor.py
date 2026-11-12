"""Run only in the isolated test PTY created by ui-probe."""
import json
from pathlib import Path
import resource
import subprocess
import sys
import termios
import time

before=termios.tcgetattr(0)
started=time.monotonic()
p=subprocess.run([sys.argv[1],'--config',sys.argv[2]])
after=termios.tcgetattr(0)
# PENDIN is transient Darwin line-discipline state, not a changed setting.
mask=getattr(termios,'PENDIN',0)
before[3]&=~mask
after[3]&=~mask
usage=resource.getrusage(resource.RUSAGE_CHILDREN)
Path(sys.argv[3]).write_text(json.dumps({'exit_status':p.returncode,
    'terminal_restored':before==after,'seconds':time.monotonic()-started,
    'cpu_seconds':usage.ru_utime+usage.ru_stime})+'\n')
sys.exit(p.returncode)
