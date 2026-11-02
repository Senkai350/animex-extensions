"""Security integration fixtures: real Ed25519 signatures and ZIP packages."""
import base64
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
spec = importlib.util.spec_from_file_location('sync', Path(__file__).with_name('animex-extensions-sync.py'))
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)

class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.key = Ed25519PrivateKey.generate()
        self.pin = self.encode(self.key.public_key().public_bytes_raw())
        self.manifest = dict(id='source.test', kind='source', name='Test', version='1.0.0', api_version=1,
                             author=dict(name='Test', public_key=self.pin), released_at='2026-01-01', entry='module.wasm', permissions=dict(network=[], storage=False))
        wasm = b'\0asm\1\0\0\0'
        import hashlib
        payload = hashlib.sha256(hashlib.sha256(sync.canonical(self.manifest)).digest() + hashlib.sha256(wasm).digest()).digest()
        self.manifest['signature'] = self.encode(self.key.sign(payload))
        with zipfile.ZipFile(self.root / 'test.anmx', 'w') as package:
            package.writestr('module.json', json.dumps(self.manifest))
            package.writestr('module.wasm', wasm)
        package = (self.root / 'test.anmx').read_bytes()
        self.index = dict(schema_version=1, repository_id=sync.IDENTITY, revision=1,
                          modules=[dict(manifest=self.manifest, url=sync.BASE+'test.anmx', size=len(package), sha256=hashlib.sha256(package).hexdigest())])
        descriptor = dict(schema_version=1, id=sync.IDENTITY, name='Test', index_url=sync.BASE+'index.json', public_key=self.pin)
        (self.root / 'repo.json').write_text(json.dumps(descriptor))
        self.sign()

    @staticmethod
    def encode(value):
        return 'ed25519:' + base64.b64encode(value).decode()

    def sign(self):
        unsigned = copy.deepcopy(self.index)
        unsigned.pop('signature', None)
        self.index['signature'] = self.encode(self.key.sign(b'AniMex repository index v1\0'+sync.canonical(unsigned)))
        (self.root / 'index.json').write_text(json.dumps(self.index))

    def test_valid_signed_artifact(self):
        self.assertEqual(sync.validate(self.root, self.pin)['revision'], 1)

    def test_modified_package(self):
        with (self.root / 'test.anmx').open('ab') as f:
            f.write(b'tampered')
        with self.assertRaises(ValueError):
            sync.validate(self.root, self.pin)

    def test_modified_index_signature(self):
        self.index['revision'] = 2
        (self.root / 'index.json').write_text(json.dumps(self.index))
        with self.assertRaises(Exception):
            sync.validate(self.root, self.pin)

    def test_even_signed_unsafe_urls_rejected(self):
        for url in ('https://evil.test/test.anmx', sync.BASE+'../test.anmx', sync.BASE+'test.anmx?token=x', sync.BASE+'nested/test.anmx', sync.BASE+'test.anmx#x'):
            with self.subTest(url=url):
                self.index['modules'][0]['url'] = url
                self.sign()
                with self.assertRaises(ValueError):
                    sync.validate(self.root, self.pin)

    def test_symlink_rejected(self):
        (self.root / 'extra.anmx').symlink_to(self.root / 'test.anmx')
        with self.assertRaises(ValueError):
            sync.validate(self.root, self.pin)

    def test_module_signature_checked_even_with_valid_index_and_hash(self):
        self.manifest['signature'] = self.encode(bytes(64))
        with zipfile.ZipFile(self.root / 'test.anmx', 'w') as package:
            package.writestr('module.json', json.dumps(self.manifest))
            package.writestr('module.wasm', b'\0asm\1\0\0\0')
        import hashlib
        package = (self.root / 'test.anmx').read_bytes()
        self.index['modules'][0].update(manifest=self.manifest, size=len(package), sha256=hashlib.sha256(package).hexdigest())
        self.sign()
        with self.assertRaises(Exception):
            sync.validate(self.root, self.pin)

if __name__ == '__main__':
    unittest.main()
