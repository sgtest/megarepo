# xfail-license

import subprocess
import os
import sys

os.putenv('RUSTC', os.path.abspath(sys.argv[2]))
os.putenv('TMPDIR', os.path.abspath(sys.argv[3]))
os.putenv('CC', sys.argv[4])
os.putenv('RUSTDOC', os.path.abspath(sys.argv[5]))
filt = sys.argv[6]

if not filt in sys.argv[1]:
    sys.exit(0)
print('maketest: ' + os.path.basename(os.path.dirname(sys.argv[1])))

proc = subprocess.Popen(['make', '-C', sys.argv[1]],
                        stdout = subprocess.PIPE,
                        stderr = subprocess.PIPE)
out, err = proc.communicate()
i = proc.wait()

if i != 0:

    print '----- ' + sys.argv[1] + """ --------------------
------ stdout ---------------------------------------------
""" + out + """
------ stderr ---------------------------------------------
""" + err + """
------        ---------------------------------------------
"""
    sys.exit(i)

