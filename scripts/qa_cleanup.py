"""Bounded cleanup retry for owned test fixtures, after each caller validates paths."""
import shutil
import time


def remove_fixture(path):
    for attempt in range(20):
        try:
            shutil.rmtree(path)
            return
        except FileNotFoundError:
            return
        except PermissionError:
            if attempt == 19:
                raise
            time.sleep(0.2)
