#!/usr/bin/python3
"""Verify locally built GitHub Release assets before atomically publishing them."""
import argparse
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import zipfile

REPO = 'Senkai350/animex-extensions'
IDENTITY = 'Senkai350.animex-extensions'
BASE = 'https://senkai.fun/extensions/'
PACKAGE_MAX = 64 * 1024**2
INDEX_MAX = 8 * 1024**2
TOTAL_MAX = 1024 * 1024**2
RELEASES = Path('/var/www/senkai.fun/extensions-releases')
LIVE = Path('/var/www/senkai.fun/public/extensions')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def decode(value):
    return (base64.b64decode(value[8:], validate=True) if value.startswith('ed25519:')
            else bytes.fromhex(value))


def verify_signature(public_key, signature, payload):
    require(len(public_key) == 32 and len(signature) == 64, 'invalid Ed25519 key/signature length')
    with tempfile.TemporaryDirectory(prefix='animex-verify-') as temporary:
        root = Path(temporary)
        (root / 'public.der').write_bytes(bytes.fromhex('302a300506032b6570032100') + public_key)
        (root / 'signature').write_bytes(signature)
        (root / 'payload').write_bytes(payload)
        openssl = os.environ.get('ANIMEX_OPENSSL', '/usr/bin/openssl')
        result = subprocess.run([openssl, 'pkeyutl', '-verify', '-pubin', '-rawin',
                                 '-keyform', 'DER', '-inkey', str(root / 'public.der'),
                                 '-sigfile', str(root / 'signature'), '-in', str(root / 'payload')],
                                capture_output=True, timeout=15)
        require(result.returncode == 0, 'invalid Ed25519 signature')


def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def parse(data):
    return json.loads(data, object_pairs_hook=pairs,
                      parse_float=lambda _: (_ for _ in ()).throw(ValueError('floats unsupported')),
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('invalid number')))


def canonical(value):
    # Match the SDK byte-for-byte, including uppercase control escapes.
    def write(item):
        if isinstance(item, str):
            escapes = {'"': '\\"', '\\': '\\\\', '\b': '\\b', '\t': '\\t', '\n': '\\n', '\f': '\\f', '\r': '\\r'}
            return '"' + ''.join(escapes.get(c, ('\\u%04X' % ord(c)) if ord(c) < 32 else c) for c in item) + '"'
        if isinstance(item, dict):
            return '{' + ','.join(write(k) + ':' + write(item[k]) for k in sorted(item)) + '}'
        if isinstance(item, list):
            return '[' + ','.join(write(v) for v in item) + ']'
        require(item is None or type(item) in (int, bool), 'unsupported canonical type')
        return json.dumps(item, separators=(',', ':'))
    return write(value).encode('utf-8')


def manifest_value(raw):
    # Reproduce serde's Manifest serialization (defaults + omitted optional fields).
    required = ('id', 'kind', 'name', 'version', 'api_version', 'author', 'released_at', 'entry')
    result = {key: raw[key] for key in required}
    result['author'] = {key: raw['author'][key] for key in ('name', 'public_key')}
    if raw['author'].get('contact') is not None:
        result['author']['contact'] = raw['author']['contact']
    permission = raw.get('permissions', {})
    result['permissions'] = {'network': permission.get('network', []), 'storage': permission.get('storage', False)}
    for key in ('description', 'signature'):
        if raw.get(key) is not None:
            result[key] = raw[key]
    for key in ('qualities', 'protocols', 'languages'):
        if raw.get(key):
            result[key] = raw[key]
    if raw.get('dependencies'):
        result['dependencies'] = [{key: dep[key] for key in ('id', 'requirement')}
                                  for dep in raw['dependencies']]
    if raw.get('app_compatibility') is not None:
        result['app_compatibility'] = {key: raw['app_compatibility'][key]
                                       for key in ('min_app_version', 'max_app_version')
                                       if raw['app_compatibility'].get(key) is not None}
    return result


def validate(directory, pinned, *, base=BASE, index_url=None):
    files = {}
    total = 0
    for path in directory.iterdir():
        info = path.lstat()
        require(stat.S_ISREG(info.st_mode), 'artifact contains non-regular file')
        require(path.name in ('repo.json', 'index.json') or re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*\.anmx', path.name), 'unexpected artifact filename')
        limit = PACKAGE_MAX if path.suffix == '.anmx' else INDEX_MAX
        require(0 < info.st_size <= limit, 'artifact file size outside bounds')
        total += info.st_size
        require(total <= TOTAL_MAX, 'artifact total exceeds limit')
        files[path.name] = path
    require({'repo.json', 'index.json'} <= files.keys(), 'missing catalog files')
    descriptor = parse(files['repo.json'].read_bytes())
    require(set(descriptor) == {'schema_version', 'id', 'name', 'index_url', 'public_key'}, 'descriptor fields')
    require(descriptor['schema_version'] == 1 and descriptor['id'] == IDENTITY and descriptor['index_url'] == (index_url or base + 'index.json'), 'descriptor identity')
    require(isinstance(descriptor['name'], str) and 0 < len(descriptor['name']) <= 200, 'descriptor name')
    require(decode(descriptor['public_key']) == decode(pinned), 'repository key does not match pin')
    index = parse(files['index.json'].read_bytes())
    require(set(index) == {'schema_version', 'repository_id', 'revision', 'modules', 'signature'}, 'index fields')
    require(index['schema_version'] == 1 and index['repository_id'] == IDENTITY, 'index identity')
    require(type(index['revision']) is int and 0 <= index['revision'] < 2**64, 'index revision')
    require(isinstance(index['modules'], list) and len(index['modules']) <= 5000, 'index entries')
    unsigned = copy.deepcopy(index)
    signature = unsigned.pop('signature')
    # The Rust index serializes each Manifest before canonicalization.
    for entry in unsigned['modules']:
        entry['manifest'] = manifest_value(entry['manifest'])
    verify_signature(decode(pinned), decode(signature), b'AniMex repository index v1\0' + canonical(unsigned))
    expected = {'repo.json', 'index.json'}
    ids = set()
    for entry in index['modules']:
        require(set(entry) == {'manifest', 'url', 'sha256', 'size'}, 'entry fields')
        manifest = manifest_value(entry['manifest'])
        module_id = manifest['id']
        require(isinstance(module_id, str) and re.fullmatch(r'[A-Za-z0-9._-]{1,160}', module_id) and '..' not in module_id and module_id not in ids, 'module id')
        ids.add(module_id)
        require(isinstance(entry['url'], str) and entry['url'].startswith(base), 'module URL base')
        filename = entry['url'][len(base):]
        require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*\.anmx', filename) and filename not in expected, 'module URL filename')
        expected.add(filename)
        require(filename in files, 'missing module package')
        require(type(entry['size']) is int and 0 < entry['size'] <= PACKAGE_MAX and files[filename].stat().st_size == entry['size'], 'package size mismatch')
        require(isinstance(entry['sha256'], str) and re.fullmatch(r'[0-9a-fA-F]{64}', entry['sha256']), 'invalid hash')
        require(hashlib.sha256(files[filename].read_bytes()).hexdigest() == entry['sha256'].lower(), 'package hash mismatch')
        with zipfile.ZipFile(files[filename]) as package:
            infos = package.infolist()
            require(len(infos) == 2 and {x.filename for x in infos} == {'module.json', 'module.wasm'}, 'package members')
            for info in infos:
                mode = info.external_attr >> 16
                require(not info.is_dir() and (not stat.S_IFMT(mode) or stat.S_ISREG(mode)) and not info.flag_bits & 1, 'unsafe package member')
                require(0 < info.file_size <= (INDEX_MAX if info.filename == 'module.json' else PACKAGE_MAX), 'expanded package size')
            packaged = manifest_value(parse(package.read('module.json')))
            require(packaged == manifest and manifest['entry'] == 'module.wasm', 'packaged manifest differs')
            wasm = package.read('module.wasm')
            unsigned_manifest = copy.deepcopy(manifest)
            module_signature = unsigned_manifest.pop('signature')
            payload = hashlib.sha256(hashlib.sha256(canonical(unsigned_manifest)).digest() + hashlib.sha256(wasm).digest()).digest()
            verify_signature(decode(manifest['author']['public_key']), decode(module_signature), payload)
    require(expected == files.keys(), 'unlisted artifact files')
    return index


def gh(*args):
    result = subprocess.run(['/usr/bin/gh', *args], check=True, capture_output=True, timeout=120)
    return parse(result.stdout)


def download_limits():
    import resource
    resource.setrlimit(resource.RLIMIT_FSIZE, (PACKAGE_MAX, PACKAGE_MAX))


def release_assets(release):
    """Accept a complete published local release, never drafts or partial uploads."""
    require(not release['draft'] and not release['prerelease'] and release.get('published_at'), 'release is not published')
    require(type(release['id']) is int and release['id'] > 0, 'release id')
    require(re.fullmatch(r'extensions-[0-9]+-[0-9a-f]{12}', release['tag_name']), 'unexpected release tag')
    names = set()
    downloads = []
    total = 0
    for asset in release['assets']:
        name = asset['name']
        require(name not in names, 'duplicate release asset')
        names.add(name)
        package = re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*\.anmx', name)
        require(package or name in ('repo.json', 'index.json', 'repo-vps.json', 'index-vps.json'), 'unexpected release asset')
        limit = PACKAGE_MAX if package else INDEX_MAX
        require(asset.get('state') == 'uploaded' and type(asset['size']) is int and 0 < asset['size'] <= limit, 'incomplete or oversized release asset')
        total += asset['size']
        require(total <= TOTAL_MAX, 'release total exceeds limit')
        if package or name in ('repo-vps.json', 'index-vps.json'):
            downloads.append(name)
    require({'repo.json', 'index.json', 'repo-vps.json', 'index-vps.json'} <= names and len(names) > 4, 'missing release catalog or packages')
    return downloads


def check_forward(previous, incoming):
    require(previous['revision'] <= incoming['revision'], 'refusing deployment rollback')
    require(previous['revision'] != incoming['revision'] or previous['signature'] == incoming['signature'], 'conflicting catalog revision')


def main():
    import fcntl
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--public-key', required=True, help='pinned ed25519:<base64> public key (not a secret)')
    args = parser.parse_args()
    require(len(decode(args.public_key)) == 32, 'pinned public key must be 32 bytes')
    # Lock before network IO; systemd and manual invocations cannot overlap.
    with open('/run/animex-extensions-sync.lock', 'a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        publication = gh('api', f'repos/{REPO}/releases/latest')
        assets = release_assets(publication)
        release_id = 'release-' + str(publication['id'])
        release = RELEASES / release_id
        if LIVE.is_symlink() and LIVE.resolve() == release:
            print(f'Already deployed {release_id}')
            return
        with tempfile.TemporaryDirectory(prefix='animex-sync-') as temporary:
            incoming = Path(temporary)
            command = ['/usr/bin/gh', 'release', 'download', publication['tag_name'], '--repo', REPO, '--dir', str(incoming)]
            for name in assets:
                command.extend(['--pattern', name])
            subprocess.run(command, check=True, timeout=240, preexec_fn=download_limits)
            (incoming / 'repo-vps.json').rename(incoming / 'repo.json')
            (incoming / 'index-vps.json').rename(incoming / 'index.json')
            index = validate(incoming, args.public_key)
            if LIVE.is_symlink():
                current = LIVE.resolve()
                require(current.parent == RELEASES and re.fullmatch(r'(?:release-)?[0-9]+', current.name), 'unexpected live symlink target')
                previous = validate(current, args.public_key)
                check_forward(previous, index)
            else:
                require(not LIVE.exists(), 'live path exists and is not a symlink')
            RELEASES.mkdir(parents=True, exist_ok=True)
            LIVE.parent.mkdir(parents=True, exist_ok=True)
            if release.exists():
                validate(release, args.public_key)
                require({p.name: p.read_bytes() for p in release.iterdir()} == {p.name: p.read_bytes() for p in incoming.iterdir()}, 'existing release differs')
            else:
                staging = Path(tempfile.mkdtemp(prefix='.incoming-', dir=RELEASES))
                try:
                    for source in incoming.iterdir():
                        shutil.copyfile(source, staging / source.name)
                        (staging / source.name).chmod(0o644)
                    staging.chmod(0o755)
                    os.rename(staging, release)
                finally:
                    if staging.exists():
                        shutil.rmtree(staging)
            temporary_link = LIVE.parent / ('.extensions-' + release_id)
            require(not temporary_link.exists() and not temporary_link.is_symlink(), 'temporary link already exists')
            try:
                temporary_link.symlink_to(release)
                os.replace(temporary_link, LIVE)
            finally:
                if temporary_link.is_symlink():
                    temporary_link.unlink()
            print(f'Deployed verified {release_id}')


if __name__ == '__main__':
    main()
