"""Deliver Ctrl+C only to a newly created test console containing the supplied CLI and this helper."""
import ctypes
from ctypes import wintypes
import os
import sys
import time


def main(pid):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.AttachConsole.argtypes = [wintypes.DWORD]
    kernel.GetConsoleProcessList.argtypes = [ctypes.POINTER(wintypes.DWORD), wintypes.DWORD]
    kernel.GetConsoleProcessList.restype = wintypes.DWORD
    kernel.SetConsoleCtrlHandler.argtypes = [ctypes.c_void_p, wintypes.BOOL]
    kernel.GenerateConsoleCtrlEvent.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel.FreeConsole()
    if not kernel.AttachConsole(pid):
        raise RuntimeError('Cannot attach to owned synthetic console')
    try:
        members = (wintypes.DWORD * 16)()
        count = kernel.GetConsoleProcessList(members, len(members))
        if count != 2 or set(members[:count]) != {pid, os.getpid()}:
            raise RuntimeError('Console ownership check failed; refusing broadcast')
        if not kernel.SetConsoleCtrlHandler(None, True):
            raise RuntimeError('Cannot protect signal helper')
        if not kernel.GenerateConsoleCtrlEvent(0, 0):
            raise RuntimeError('Cannot send scoped Ctrl+C')
        time.sleep(.2)
    finally:
        kernel.FreeConsole()


if __name__ == '__main__':
    main(int(sys.argv[1]))
