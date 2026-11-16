import ctypes, json, os, pathlib, plistlib, subprocess, sys

root = pathlib.Path('/private/tmp/codebasegraph-identity-evidence-20261117/apfs-probe')
image = root / 'identity.dmg'
mount = root / 'mount'
mount.mkdir(exist_ok=True)
records = {'create': {'command': 'hdiutil create -size 32m -fs APFS -volname CBGIdentityProbe -type UDIF -layout GPTSPUD identity.dmg', 'initial_sandbox': 'Device not configured', 'escalated': 'created'}, 'commands': [], 'observations': []}
class AttrList(ctypes.Structure):
    _fields_ = [('bitmapcount', ctypes.c_ushort), ('reserved', ctypes.c_ushort), ('commonattr', ctypes.c_uint32), ('volattr', ctypes.c_uint32), ('dirattr', ctypes.c_uint32), ('fileattr', ctypes.c_uint32), ('forkattr', ctypes.c_uint32)]
libc = ctypes.CDLL(None, use_errno=True)
libc.fgetattrlist.argtypes = [ctypes.c_int, ctypes.POINTER(AttrList), ctypes.c_void_p, ctypes.c_size_t, ctypes.c_ulong]
def command(args):
    p = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    records['commands'].append({'argv': args, 'returncode': p.returncode, 'stdout': p.stdout.decode(errors='replace'), 'stderr': p.stderr.decode(errors='replace')})
    if p.returncode: raise RuntimeError(f'{args[0]} failed {p.returncode}')
    return p.stdout
def observe(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        before = os.fstat(fd)
        attrs = AttrList(5, 0, 0, 0x80040000, 0, 0, 0)
        buf = ctypes.create_string_buffer(20)
        rc = libc.fgetattrlist(fd, ctypes.byref(attrs), buf, len(buf), 0)
        errno = ctypes.get_errno() if rc else 0
        after = os.fstat(fd)
        return {'path': str(path.relative_to(root)), 'returncode': rc, 'errno': errno, 'returned_size': int.from_bytes(buf.raw[:4], sys.byteorder), 'uuid_hex': buf.raw[4:20].hex(), 'before': {'device': before.st_dev, 'inode': before.st_ino}, 'after': {'device': after.st_dev, 'inode': after.st_ino}}
    finally: os.close(fd)
attached = False
try:
    records['prior_attempt'] = 'Initial run failed writing observations: OS command outputs were bytes and not JSON serializable; detach was attempted in finally. No observation retained.'
    records['os'] = {'sw_vers': command(['sw_vers']).decode(), 'uname': command(['uname', '-a']).decode()}
    for attempt in (1, 2):
        raw = command(['hdiutil', 'attach', '-plist', '-nobrowse', '-mountpoint', str(mount), str(image)])
        attached = True
        entities = plistlib.loads(raw)['system-entities']
        records['observations'].append({'attempt': attempt, 'entities': entities})
        directory = mount / 'testdir'
        if attempt == 1:
            directory.mkdir(exist_ok=True)
            (directory / 'testfile').write_text('identity fixture\n')
        records['observations'][-1]['resources'] = [observe(directory), observe(directory / 'testfile')]
        command(['hdiutil', 'detach', str(mount)])
        attached = False
except Exception as exc:
    records['error'] = repr(exc)
finally:
    if attached:
        try: command(['hdiutil', 'detach', str(mount)])
        except Exception as exc: records['cleanup_error'] = repr(exc)
    (root / 'observations.json').write_text(json.dumps(records, indent=2) + '\n')
print(json.dumps({'observations': records['observations'], 'error': records.get('error'), 'cleanup_error': records.get('cleanup_error')}, indent=2))
sys.exit(1 if 'error' in records or 'cleanup_error' in records else 0)
