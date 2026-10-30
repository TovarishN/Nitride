"""Pin this process and inherited benchmark children to Windows CPU 0."""
import ctypes
import os
import subprocess

kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.GetCurrentProcess.restype = ctypes.c_void_p
kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), 1):
    raise ctypes.WinError(ctypes.get_last_error())
os.environ["NITRIDE_BENCH_AFFINITY"] = "Windows logical CPU 0, inherited process affinity mask 1"
raise SystemExit(subprocess.call(["node", "scripts/compiler-audit/scanner-runs.mjs"]))
