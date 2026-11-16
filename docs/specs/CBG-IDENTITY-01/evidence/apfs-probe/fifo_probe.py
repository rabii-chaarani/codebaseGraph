import ctypes, json, os, pathlib, platform, subprocess, sys

root = pathlib.Path('/private/tmp/codebasegraph-identity-evidence-20261117/apfs-probe')
directory = root / 'fifo-fixture'
directory.mkdir(exist_ok=True)
fifo = directory / 'probe.fifo'
if fifo.exists():
    raise RuntimeError('fixture already exists; preserve existing evidence')
os.mkfifo(fifo, 0o600)
class AttrList(ctypes.Structure):
    _fields_ = [('bitmapcount', ctypes.c_ushort), ('reserved', ctypes.c_ushort), ('commonattr', ctypes.c_uint32), ('volattr', ctypes.c_uint32), ('dirattr', ctypes.c_uint32), ('fileattr', ctypes.c_uint32), ('forkattr', ctypes.c_uint32)]
libc = ctypes.CDLL(None, use_errno=True)
libc.fgetattrlist.argtypes = [ctypes.c_int, ctypes.POINTER(AttrList), ctypes.c_void_p, ctypes.c_size_t, ctypes.c_ulong]
def capture(fd):
    before = os.fstat(fd)
    attrs = AttrList(5, 0, 0, 0x80040000, 0, 0, 0)
    buf = ctypes.create_string_buffer(20)
    ctypes.set_errno(0)
    rc = libc.fgetattrlist(fd, ctypes.byref(attrs), buf, len(buf), 0)
    error = ctypes.get_errno() if rc else 0
    after = os.fstat(fd)
    return {'returncode': rc, 'errno': error, 'error': os.strerror(error) if error else None, 'returned_size': int.from_bytes(buf.raw[:4], sys.byteorder) if rc == 0 else None, 'uuid_hex': buf.raw[4:20].hex() if rc == 0 else None, 'before': {'device': before.st_dev, 'inode': before.st_ino, 'mode': oct(before.st_mode)}, 'after': {'device': after.st_dev, 'inode': after.st_ino, 'mode': oct(after.st_mode)}}
result = {'command': 'python3 /private/tmp/codebasegraph-identity-evidence-20261117/apfs-probe/fifo_probe.py', 'sw_vers': subprocess.run(['sw_vers'], capture_output=True, text=True).stdout, 'uname': platform.uname()._asdict(), 'flags': 'FIFO O_RDWR | O_NONBLOCK; parent O_RDONLY', 'fixture': str(fifo)}
directory_fd = os.open(directory, os.O_RDONLY)
fifo_fd = None
try:
    fifo_fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
    result['parent'] = capture(directory_fd)
    result['fifo'] = capture(fifo_fd)
    result['same_nonzero_uuid'] = bool(result['parent']['uuid_hex'] and result['fifo']['uuid_hex'] and result['parent']['uuid_hex'] == result['fifo']['uuid_hex'] and int(result['fifo']['uuid_hex'], 16))
except OSError as exc:
    result['exception'] = {'errno': exc.errno, 'message': str(exc)}
finally:
    if fifo_fd is not None: os.close(fifo_fd)
    os.close(directory_fd)
    (root / 'fifo-observations.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
